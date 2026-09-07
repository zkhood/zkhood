//! Fetches a cryptographically verified Robinhood Chain (L2) block hash, anchored only in
//! Ethereum L1 consensus via a Helios light client sidecar. See module-level docs in `lib.rs`
//! for the full trust argument.
//!
//! Helios runs as a separate CLI process (`helios ethereum --network sepolia ...`), exposing a
//! local JSON-RPC endpoint (default `http://127.0.0.1:8545`) that only ever returns data it has
//! independently verified against Ethereum L1 consensus. This crate never links Helios's own
//! (heavy) dependency tree; it is just a plain, lightweight JSON-RPC client pointed at that
//! local endpoint. See `scripts/run-helios-sepolia.sh` for how to start the sidecar.
//!
//! Verification steps:
//! 1. `eth_call` (via the trusted local Helios endpoint) to `latestConfirmed()` on the Rollup
//!    contract -> `confirmed_hash`. This is the ONLY step that must be trusted; Helios itself
//!    checks it against Ethereum L1 consensus before ever returning it.
//! 2. Fetch the `AssertionCreated` log for `confirmed_hash` from an UNTRUSTED RPC (any provider,
//!    does not need to be Helios). This gives a candidate `afterState` (which contains the L2
//!    block hash) plus the `parentAssertionHash` and `inboxAcc` used to create it.
//! 3. Self-verify: call the Rollup contract's own `computeAssertionHash` (a `pure` function,
//!    still executed via the trusted local Helios endpoint) with the candidate data, and check
//!    it equals `confirmed_hash` from step 1. A mismatch means the untrusted data from step 2
//!    was wrong/malicious and must be rejected; it can never produce a false positive.

use crate::abi::{AssertionState, IRollupCore};
use crate::config::NetworkConfig;
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, B256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Filter, TransactionRequest};
use alloy::sol_types::{SolCall, SolEvent};
use eyre::{eyre, Result};

/// Default local RPC endpoint exposed by the Helios sidecar process.
pub const DEFAULT_HELIOS_RPC: &str = "http://127.0.0.1:8545";

/// A cryptographically verified snapshot of Robinhood Chain's latest confirmed L2 state, as
/// anchored by Ethereum L1 consensus (via Helios) plus the Rollup contract's own hash check.
#[derive(Debug, Clone)]
pub struct VerifiedL2Anchor {
    pub confirmed_assertion_hash: B256,
    pub l2_block_hash: B256,
    pub l2_send_root: B256,
}

/// Runs the full trustless verification flow described in the module docs and returns the
/// verified L2 anchor.
///
/// `helios_rpc_url` MUST be a locally-run Helios sidecar (see module docs) — it is the only
/// trusted input. `untrusted_l1_rpc` is used only to fetch the candidate event data in step 2;
/// its correctness is never assumed, only self-checked in step 3, so any RPC works there.
pub async fn verify_latest_l2_anchor(
    helios_rpc_url: &str,
    untrusted_l1_rpc: &str,
    network: &NetworkConfig,
) -> Result<VerifiedL2Anchor> {
    let rollup_address = network.rollup_address;

    let confirmed_assertion_hash = call_latest_confirmed(helios_rpc_url, rollup_address).await?;

    let (parent_assertion_hash, after_state, inbox_acc) = fetch_assertion_created(
        untrusted_l1_rpc,
        rollup_address,
        confirmed_assertion_hash,
        network.rollup_deploy_block,
    )
    .await?;

    let recomputed_hash = call_compute_assertion_hash(
        helios_rpc_url,
        rollup_address,
        parent_assertion_hash,
        &after_state,
        inbox_acc,
    )
    .await?;

    if recomputed_hash != confirmed_assertion_hash {
        return Err(eyre!(
            "assertion self-verification failed: contract computed {recomputed_hash:#x}, \
             expected {confirmed_assertion_hash:#x} (untrusted event data was wrong or malicious)"
        ));
    }

    Ok(VerifiedL2Anchor {
        confirmed_assertion_hash,
        l2_block_hash: after_state.globalState.l2_block_hash(),
        l2_send_root: after_state.globalState.bytes32Vals[1],
    })
}

async fn call_latest_confirmed(helios_rpc_url: &str, rollup_address: Address) -> Result<B256> {
    let provider = ProviderBuilder::new().connect_http(helios_rpc_url.parse()?);
    let calldata = IRollupCore::latestConfirmedCall {}.abi_encode();
    let tx = TransactionRequest {
        to: Some(rollup_address.into()),
        input: calldata.into(),
        ..Default::default()
    };
    let result = provider
        .call(tx)
        .block(BlockNumberOrTag::Latest.into())
        .await
        .map_err(|e| eyre!("Helios-verified latestConfirmed() call failed: {e}"))?;
    let decoded = IRollupCore::latestConfirmedCall::abi_decode_returns(&result)
        .map_err(|e| eyre!("failed to decode latestConfirmed() return value: {e}"))?;
    Ok(decoded)
}

async fn call_compute_assertion_hash(
    helios_rpc_url: &str,
    rollup_address: Address,
    prev_assertion_hash: B256,
    state: &AssertionState,
    inbox_acc: B256,
) -> Result<B256> {
    let provider = ProviderBuilder::new().connect_http(helios_rpc_url.parse()?);
    let calldata = IRollupCore::computeAssertionHashCall {
        prevAssertionHash: prev_assertion_hash,
        state: state.clone(),
        inboxAcc: inbox_acc,
    }
    .abi_encode();
    let tx = TransactionRequest {
        to: Some(rollup_address.into()),
        input: calldata.into(),
        ..Default::default()
    };
    let result = provider
        .call(tx)
        .block(BlockNumberOrTag::Latest.into())
        .await
        .map_err(|e| eyre!("Helios-verified computeAssertionHash() call failed: {e}"))?;
    let decoded = IRollupCore::computeAssertionHashCall::abi_decode_returns(&result)
        .map_err(|e| eyre!("failed to decode computeAssertionHash() return value: {e}"))?;
    Ok(decoded)
}

/// eth_getLogs block-range window. Kept under common free-tier caps (e.g. publicnode's 50k)
/// while still finding the latest confirmed assertion in one or two windows in practice.
const LOG_WINDOW: u64 = 45_000;

/// Fetches the `AssertionCreated` log matching `assertion_hash` from an untrusted RPC. Returns
/// `(parentAssertionHash, afterState, afterInboxBatchAcc)`. The caller MUST self-verify this
/// data (see `verify_latest_l2_anchor`); it is never trusted on its own.
///
/// Scans backward from the chain tip in bounded windows (down to `deploy_block`) so it works
/// against RPCs that cap the eth_getLogs range. The latest confirmed assertion is always recent,
/// so this normally hits on the first window.
async fn fetch_assertion_created(
    untrusted_l1_rpc: &str,
    rollup_address: Address,
    assertion_hash: B256,
    deploy_block: u64,
) -> Result<(B256, AssertionState, B256)> {
    let provider = ProviderBuilder::new().connect_http(untrusted_l1_rpc.parse()?);
    let latest = provider.get_block_number().await?;

    let mut to_block = latest;
    loop {
        let from_block = to_block.saturating_sub(LOG_WINDOW - 1).max(deploy_block);

        let filter = Filter::new()
            .address(rollup_address)
            .event_signature(IRollupCore::AssertionCreated::SIGNATURE_HASH)
            .topic1(assertion_hash)
            .from_block(from_block)
            .to_block(to_block);

        let logs = provider.get_logs(&filter).await?;
        if let Some(log) = logs.first() {
            let decoded = IRollupCore::AssertionCreated::decode_log(&log.inner)
                .map_err(|e| eyre!("failed to decode AssertionCreated log: {e}"))?;
            return Ok((
                decoded.parentAssertionHash,
                decoded.assertion.afterState.clone(),
                decoded.afterInboxBatchAcc,
            ));
        }

        if from_block <= deploy_block {
            return Err(eyre!(
                "no AssertionCreated log found for assertion {assertion_hash:#x} \
                 between blocks {deploy_block} and {latest}"
            ));
        }
        to_block = from_block - 1;
    }
}


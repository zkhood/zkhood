//! Batch execution for the TEE fast path: runs a batch with real REVM against verified state and
//! returns the same [`PublicValues`] the SP1 guest will later prove. Keeping this logic here (not
//! inline in the HTTP layer) means the request handler stays a thin adapter.
//!
//! Two paths share the same executor:
//! - dev: an explicit in-memory `mock_state` (verifies nothing, opt-in only);
//! - prod: the enclave builds a cryptographically verified provider itself (L1 via Helios, L2 via
//!   Merkle proofs) from the request's `access_list`, so no base state is ever trusted from the
//!   caller.

use revm::context::TxEnv;
use revm::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use zkhood_revm_executor::{
    apply_batch_with_overlay, BatchContext, OverlayState, PublicValues,
    MockRobinhoodStateProvider,
};
use zkhood_robinhood_bridge::{
    fetch_verified_state_root, verify_latest_l2_anchor, NetworkConfig, VerifiedStateProvider,
};

/// The accounts (and storage slots) a batch reads, so the enclave can prefetch and verify exactly
/// those against the L1-anchored L2 state root before executing.
#[derive(Clone, Deserialize)]
pub struct AccountAccess {
    pub address: Address,
    #[serde(default)]
    pub slots: Vec<U256>,
}

/// A batch to execute on the TEE fast path.
#[derive(Clone, Deserialize)]
pub struct ExecuteBatchRequest {
    pub ctx: BatchContext,
    pub txs: Vec<TxEnv>,
    /// Production path: accounts/slots to verify against the L1-anchored L2 state root.
    #[serde(default)]
    pub access_list: Vec<AccountAccess>,
    /// Development-only unverified base state. Must be explicitly opted into.
    #[serde(default)]
    pub mock_state: Option<MockRobinhoodStateProvider>,
}

/// Result of a fast-path execution: exactly the public values that will later be proven by SP1.
#[derive(Debug, Clone, Serialize)]
pub struct ExecuteBatchResponse {
    pub public_values: PublicValues,
    pub tx_count: u32,
}

/// RPC + network configuration the enclave uses to build verified state. Absent in dev-only mode.
#[derive(Clone)]
pub struct WorkerConfig {
    pub helios_rpc: String,
    pub untrusted_l1_rpc: String,
    pub robinhood_rpc: String,
    pub network: NetworkConfig,
}

#[derive(Debug)]
pub enum ExecError {
    NoState,
    Verification(String),
    Execution(String),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::NoState => write!(
                f,
                "no base state: send `access_list` with the enclave configured for verified state \
                 (prod), or `mock_state` (dev)"
            ),
            ExecError::Verification(e) => write!(f, "state verification failed: {e}"),
            ExecError::Execution(e) => write!(f, "batch execution failed: {e}"),
        }
    }
}

/// Dev fast path: execute against an explicit, unverified in-memory base state. Opt-in only.
/// The persistent `overlay` carries rollup-private balances across batches; the updated overlay
/// is returned so the caller can store it.
pub fn execute_batch_mock(
    req: ExecuteBatchRequest,
    overlay: OverlayState,
) -> Result<(ExecuteBatchResponse, OverlayState), ExecError> {
    let base = req.mock_state.clone().ok_or(ExecError::NoState)?;
    run(base, overlay, &req.ctx, req.txs)
}

/// Production fast path: the enclave itself builds a cryptographically verified base state
/// (L1-anchored via Helios, L2 via Merkle proofs) for exactly the batch's `access_list`, then
/// executes on top of the persistent rollup overlay. Nothing about the base state is trusted
/// from the caller.
pub async fn execute_batch_verified(
    req: ExecuteBatchRequest,
    overlay: OverlayState,
    cfg: &WorkerConfig,
) -> Result<(ExecuteBatchResponse, OverlayState), ExecError> {
    let anchor = verify_latest_l2_anchor(&cfg.helios_rpc, &cfg.untrusted_l1_rpc, &cfg.network)
        .await
        .map_err(|e| ExecError::Verification(e.to_string()))?;
    let block = fetch_verified_state_root(&cfg.robinhood_rpc, anchor.l2_block_hash)
        .await
        .map_err(|e| ExecError::Verification(e.to_string()))?;

    let mut provider = VerifiedStateProvider::new(block);
    for acc in &req.access_list {
        provider
            .fetch_account(&cfg.robinhood_rpc, acc.address, &acc.slots)
            .await
            .map_err(|e| ExecError::Verification(e.to_string()))?;
    }

    run(provider, overlay, &req.ctx, req.txs)
}

fn run<P: revm::database::DatabaseRef>(
    base: P,
    overlay: OverlayState,
    ctx: &BatchContext,
    txs: Vec<TxEnv>,
) -> Result<(ExecuteBatchResponse, OverlayState), ExecError> {
    let (outcomes, public_values, new_overlay) = apply_batch_with_overlay(base, overlay, ctx, txs)
        .map_err(|e| ExecError::Execution(format!("{e:?}")))?;
    Ok((
        ExecuteBatchResponse {
            tx_count: outcomes.len() as u32,
            public_values,
        },
        new_overlay,
    ))
}

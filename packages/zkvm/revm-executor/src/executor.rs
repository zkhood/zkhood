//! Executes an ordered batch of transactions with real REVM semantics against a database that
//! layers rollup-private state over verified Robinhood Chain state.
//!
//! The TEE worker calls this natively for the fast/private path; the SP1 guest program calls
//! the exact same function to produce a proof of the identical execution. Both must produce
//! byte-identical [`BatchResult`]s for the same inputs.

use revm::context::result::{ExecutionResult, HaltReason};
use revm::context::TxEnv;
use revm::database::{CacheDB, DatabaseRef};
use revm::state::EvmState;
use revm::{Context, ExecuteEvm, MainBuilder, MainContext};

use crate::commitment::{keccak256, state_commitment, Hash};
use crate::overlay::OverlayState;

/// Everything both the TEE and the SP1 guest must commit to identically for a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PublicValues {
    pub chain_id: u64,
    pub rollup_address: [u8; 20],
    pub batch_number: u64,
    pub tee_batch_nonce: u64,
    pub old_state_commitment: Hash,
    pub new_state_commitment: Hash,
    pub tx_commitment: Hash,
    pub tx_count: u32,
}

/// Chain/rollup/batch identity a batch of transactions is bound to.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct BatchContext {
    pub chain_id: u64,
    pub rollup_address: [u8; 20],
    pub batch_number: u64,
    pub tee_batch_nonce: u64,
}

/// One transaction's REVM execution outcome, kept alongside the raw `ExecutionResult` for the
/// caller to inspect gas usage, logs, or revert reasons.
pub struct TxOutcome {
    pub result: ExecutionResult<HaltReason>,
}

#[derive(Debug)]
pub enum BatchError<DbError> {
    Database(DbError),
    Execution(String),
}

/// Runs `txs` in order against `provider` (verified Robinhood Chain state) with a
/// rollup-private overlay (deposits, freshly created rollup-only accounts, etc. all live in the
/// overlay via the same `CacheDB`). Returns the per-tx outcomes and the batch's public values.
///
/// `old_state_commitment` must be the commitment of the overlay's state *before* this batch;
/// the caller is responsible for persisting the overlay across batches (the TEE worker keeps it
/// durably; the SP1 guest receives it as part of its witness).
pub fn apply_batch<P>(
    provider: P,
    ctx: &BatchContext,
    old_state_commitment: Hash,
    txs: Vec<TxEnv>,
) -> Result<(Vec<TxOutcome>, PublicValues), BatchError<<P as DatabaseRef>::Error>>
where
    P: DatabaseRef,
{
    let (outcomes, public_values, _state) =
        apply_batch_stateful(provider, ctx, old_state_commitment, txs)?;
    Ok((outcomes, public_values))
}

/// Same as [`apply_batch`] but also returns the raw [`EvmState`] REVM produced, so a stateful
/// caller (the TEE worker) can merge those changes into a durable overlay that persists across
/// batches. The SP1 guest uses [`apply_batch`] (it does not carry state between proofs).
pub fn apply_batch_stateful<P>(
    provider: P,
    ctx: &BatchContext,
    old_state_commitment: Hash,
    txs: Vec<TxEnv>,
) -> Result<(Vec<TxOutcome>, PublicValues, EvmState), BatchError<<P as DatabaseRef>::Error>>
where
    P: DatabaseRef,
{
    let db = CacheDB::new(provider);
    // Configure the EVM for Robinhood Chain (not Ethereum mainnet) so tx chain-id checks pass.
    let mut evm = Context::mainnet()
        .modify_cfg_chained(|cfg| cfg.chain_id = ctx.chain_id)
        .with_db(db)
        .build_mainnet();

    let mut outcomes = Vec::with_capacity(txs.len());
    let mut tx_hashes = Vec::with_capacity(txs.len());

    for tx in txs {
        let tx_hash = tx_commitment_entry(&tx);
        // `transact_one` accumulates state in the journal; `transact` would finalize (and clear)
        // after every tx, so the single `finalize()` below would then see an empty state.
        let result = evm
            .transact_one(tx)
            .map_err(|_| BatchError::Execution("revm transaction failed".into()))?;
        outcomes.push(TxOutcome { result });
        tx_hashes.push(tx_hash);
    }

    let state = evm.finalize();
    let new_state_commitment = state_commitment(&state);
    let tx_commitment = keccak256_concat_hashes(&tx_hashes);

    let public_values = PublicValues {
        chain_id: ctx.chain_id,
        rollup_address: ctx.rollup_address,
        batch_number: ctx.batch_number,
        tee_batch_nonce: ctx.tee_batch_nonce,
        old_state_commitment,
        new_state_commitment,
        tx_commitment,
        tx_count: outcomes.len() as u32,
    };

    Ok((outcomes, public_values, state))
}

/// Executes a batch on top of a durable rollup [`OverlayState`]: the overlay is layered over the
/// verified base state (so prior rollup writes persist), the batch runs, and its changes are
/// merged back into the overlay. Chains the state commitment (`old` = the overlay's running
/// commitment, `new` = this batch's diff commitment). Returns the updated overlay for the caller
/// to persist. This is what the TEE worker uses to keep real rollup balances across batches.
pub fn apply_batch_with_overlay<P>(
    base: P,
    mut overlay: OverlayState,
    ctx: &BatchContext,
    txs: Vec<TxEnv>,
) -> Result<(Vec<TxOutcome>, PublicValues, OverlayState), BatchError<<P as DatabaseRef>::Error>>
where
    P: DatabaseRef,
{
    let old_state_commitment = overlay.commitment();
    let (outcomes, public_values, state) = {
        let layered = overlay.layer_over(base);
        apply_batch_stateful(layered, ctx, old_state_commitment, txs)?
    };
    overlay.apply_evm_state(&state);
    overlay.set_commitment(public_values.new_state_commitment);
    Ok((outcomes, public_values, overlay))
}

fn tx_commitment_entry(tx: &TxEnv) -> Hash {
    keccak256(format!("{tx:?}").as_bytes())
}

fn keccak256_concat_hashes(hashes: &[Hash]) -> Hash {
    let mut buf = Vec::with_capacity(hashes.len() * 32);
    for h in hashes {
        buf.extend_from_slice(h);
    }
    keccak256(&buf)
}

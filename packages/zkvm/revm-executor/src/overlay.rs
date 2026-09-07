//! Rollup-private overlay state that persists across batches.
//!
//! Each batch executes against the verified Robinhood Chain base state (read-only) layered under
//! this overlay, which accumulates everything the rollup itself has written (deposits, transfers
//! between rollup accounts, freshly created rollup-only accounts). After a batch runs, its state
//! changes are merged back in so the next batch sees them — giving the TEE worker real, durable
//! rollup state rather than only chaining an opaque commitment. The overlay also carries the
//! running state commitment so successive batches chain (`old` = prev `new`).

use std::collections::{HashMap, HashSet};

use revm::bytecode::Bytecode;
use revm::database::DatabaseRef;
use revm::primitives::{Address, B256, U256};
use revm::state::{AccountInfo, EvmState};

use crate::commitment::Hash;

/// Durable rollup-private state. Serializable so the enclave can persist it (e.g. via Oyster KMS).
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverlayState {
    accounts: HashMap<Address, AccountInfo>,
    storage: HashMap<(Address, U256), U256>,
    code: HashMap<B256, Bytecode>,
    /// Accounts destroyed in the overlay; reads must return "empty", not fall through to the base.
    destroyed: HashSet<Address>,
    /// Running per-batch state commitment (previous batch's `new_state_commitment`).
    last_commitment: Hash,
}

impl OverlayState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The running commitment the next batch chains from.
    pub fn commitment(&self) -> Hash {
        self.last_commitment
    }

    pub fn set_commitment(&mut self, commitment: Hash) {
        self.last_commitment = commitment;
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty() && self.storage.is_empty() && self.destroyed.is_empty()
    }

    /// Balance recorded in the overlay for `address` (0 if the overlay hasn't written it).
    pub fn balance_of(&self, address: &Address) -> U256 {
        self.accounts.get(address).map(|a| a.balance).unwrap_or(U256::ZERO)
    }

    /// Merges the state changes REVM reported for a batch into the overlay. `finalize()` returns
    /// exactly the accounts REVM touched/changed (the same set `state_commitment` hashes).
    pub fn apply_evm_state(&mut self, state: &EvmState) {
        for (address, account) in state.iter() {
            if account.is_selfdestructed() {
                self.accounts.remove(address);
                self.storage.retain(|(addr, _), _| addr != address);
                self.destroyed.insert(*address);
                continue;
            }

            self.destroyed.remove(address);
            if let Some(code) = &account.info.code {
                if !code.is_empty() {
                    self.code.insert(account.info.code_hash, code.clone());
                }
            }
            self.accounts.insert(*address, account.info.clone());
            for (slot, value) in account.storage.iter() {
                self.storage.insert((*address, *slot), value.present_value);
            }
        }
    }

    /// Layers this overlay over `base` so a batch reads overlay-first, base-fallback.
    pub fn layer_over<P: DatabaseRef>(&self, base: P) -> OverlayDb<'_, P> {
        OverlayDb { overlay: self, base }
    }
}

/// Read-only view of an [`OverlayState`] on top of a base state provider.
pub struct OverlayDb<'a, P> {
    overlay: &'a OverlayState,
    base: P,
}

impl<P: DatabaseRef> DatabaseRef for OverlayDb<'_, P> {
    type Error = P::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        if self.overlay.destroyed.contains(&address) {
            return Ok(None);
        }
        if let Some(info) = self.overlay.accounts.get(&address) {
            return Ok(Some(info.clone()));
        }
        self.base.basic_ref(address)
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        if let Some(code) = self.overlay.code.get(&code_hash) {
            return Ok(code.clone());
        }
        self.base.code_by_hash_ref(code_hash)
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        if let Some(value) = self.overlay.storage.get(&(address, index)) {
            return Ok(*value);
        }
        if self.overlay.destroyed.contains(&address) {
            return Ok(U256::ZERO);
        }
        self.base.storage_ref(address, index)
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        self.base.block_hash_ref(number)
    }
}

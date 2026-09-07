//! Read-only access to Robinhood Chain state, as REVM needs it via `revm::DatabaseRef`.
//!
//! The real, cryptographically verified implementation lives in the `robinhood-bridge` crate
//! (`VerifiedStateProvider`): it anchors to Ethereum L1 via a Helios light client, reads Robinhood
//! Chain's latest confirmed L2 state root, and checks each account/slot with a Merkle-Patricia
//! proof against it. [`MockRobinhoodStateProvider`] below is an in-memory stand-in for local
//! development and tests only; it verifies nothing and must never be used outside tests.

use revm::bytecode::Bytecode;
use revm::database::DatabaseRef;
use revm::primitives::{Address, B256, U256};
use revm::state::AccountInfo;
use std::collections::HashMap;

/// Cryptographically verified read access to Robinhood Chain state. A real implementation
/// backs this with an Ethereum-L1-anchored light client (see module docs); it is not merely an
/// RPC passthrough.
pub trait RobinhoodStateProvider: DatabaseRef {}

impl<T: DatabaseRef> RobinhoodStateProvider for T {}

/// Test-only stand-in for the real light-client-backed provider. Answers every query from an
/// in-memory map and performs **no verification whatsoever**. Never use outside tests/local dev.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct MockRobinhoodStateProvider {
    accounts: HashMap<Address, AccountInfo>,
    storage: HashMap<(Address, U256), U256>,
    code: HashMap<B256, Bytecode>,
}

impl MockRobinhoodStateProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_account(&mut self, address: Address, info: AccountInfo) {
        self.accounts.insert(address, info);
    }

    pub fn set_storage(&mut self, address: Address, index: U256, value: U256) {
        self.storage.insert((address, index), value);
    }

    pub fn set_code(&mut self, hash: B256, code: Bytecode) {
        self.code.insert(hash, code);
    }
}

impl DatabaseRef for MockRobinhoodStateProvider {
    type Error = core::convert::Infallible;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(self.accounts.get(&address).cloned())
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        Ok(self.code.get(&code_hash).cloned().unwrap_or_default())
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        Ok(self
            .storage
            .get(&(address, index))
            .copied()
            .unwrap_or_default())
    }

    fn block_hash_ref(&self, _number: u64) -> Result<B256, Self::Error> {
        Ok(B256::ZERO)
    }
}

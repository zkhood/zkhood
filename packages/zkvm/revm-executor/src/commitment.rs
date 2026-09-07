//! A hash-based state commitment layered on top of REVM's raw execution state.
//!
//! This is **not** a real Ethereum-style Merkle-Patricia state trie/root; it is our own simple
//! commitment (sorted account/storage diffs, hashed) so the TEE and the SP1 guest can agree on
//! "what changed", cheaply. Building a real, canonical state trie compatible with other Ethereum
//! tooling is a larger, separate task — flagged here rather than silently assumed to be
//! equivalent to a production Ethereum state root.

use std::collections::BTreeMap;

use revm::primitives::Address;
use revm::state::{Account, EvmState};
use sha3::{Digest, Keccak256};

pub type Hash = [u8; 32];

pub fn keccak256(data: &[u8]) -> Hash {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Deterministically hashes the accounts REVM reports as changed, sorted by address so the
/// commitment does not depend on `HashMap` iteration order.
pub fn state_commitment(state: &EvmState) -> Hash {
    let mut sorted: BTreeMap<Address, &Account> = BTreeMap::new();
    for (address, account) in state.iter() {
        sorted.insert(*address, account);
    }

    let mut buf = Vec::new();
    for (address, account) in sorted {
        buf.extend_from_slice(address.as_slice());
        buf.extend_from_slice(&account.info.balance.to_be_bytes::<32>());
        buf.extend_from_slice(&account.info.nonce.to_be_bytes());
        buf.extend_from_slice(account.info.code_hash.as_slice());

        let mut storage_sorted: BTreeMap<_, _> = BTreeMap::new();
        for (key, slot) in account.storage.iter() {
            storage_sorted.insert(*key, slot.present_value);
        }
        for (key, value) in storage_sorted {
            buf.extend_from_slice(&key.to_be_bytes::<32>());
            buf.extend_from_slice(&value.to_be_bytes::<32>());
        }
    }

    keccak256(&buf)
}

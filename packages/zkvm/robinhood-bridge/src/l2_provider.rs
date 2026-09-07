//! Turns an L1-anchored, verified Robinhood Chain (L2) block hash into cryptographically
//! verified account/storage reads that REVM can consume.
//!
//! Flow (all steps self-verifying — no RPC is trusted):
//! 1. `fetch_verified_state_root`: fetch the L2 block by its (already L1-verified) hash from any
//!    Robinhood RPC, recompute the block hash from the returned header and require it to match,
//!    then read the header's `stateRoot`. A matching hash means the state root is authentic.
//! 2. `VerifiedStateProvider::fetch_account`: for a specific account (and storage slots) call
//!    `eth_getProof` at that block, verify the account's Merkle-Patricia proof against the
//!    verified `stateRoot`, verify each storage proof against the account's `storageHash`, and
//!    verify fetched bytecode against the account's `codeHash`. Only verified values are kept.
//! 3. The provider then answers REVM's synchronous `DatabaseRef` queries purely from that
//!    verified in-memory set, so nothing unverified ever reaches the executor.

use alloy::consensus::BlockHeader;
use alloy::primitives::{keccak256, Address, Bytes, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::BlockId;
use alloy_trie::{proof::verify_proof, Nibbles, TrieAccount, EMPTY_ROOT_HASH};
use eyre::{eyre, Result};
use revm::bytecode::Bytecode;
use revm::database::DatabaseRef;
use revm::state::AccountInfo;
use std::collections::HashMap;

/// A Robinhood Chain (L2) block whose state root is trustlessly bound to the L1-verified hash.
#[derive(Debug, Clone, Copy)]
pub struct VerifiedL2Block {
    pub number: u64,
    pub hash: B256,
    pub state_root: B256,
}

/// Fetches the L2 block for `l2_block_hash` and self-verifies it: the block hash recomputed from
/// the returned header must equal `l2_block_hash` (which the caller obtained from L1 via Helios).
pub async fn fetch_verified_state_root(
    l2_rpc: &str,
    l2_block_hash: B256,
) -> Result<VerifiedL2Block> {
    let provider = ProviderBuilder::new().connect_http(l2_rpc.parse()?);

    let block = provider
        .get_block_by_hash(l2_block_hash)
        .await?
        .ok_or_else(|| eyre!("L2 block {l2_block_hash:#x} not found on Robinhood RPC"))?;

    let recomputed = block.header.inner.hash_slow();
    if recomputed != l2_block_hash {
        return Err(eyre!(
            "L2 block hash self-verification failed: header hashes to {recomputed:#x}, \
             expected L1-anchored {l2_block_hash:#x}"
        ));
    }

    Ok(VerifiedL2Block {
        number: block.header.inner.number(),
        hash: l2_block_hash,
        state_root: block.header.inner.state_root(),
    })
}

/// Verified read access to Robinhood Chain state at a fixed L2 block. Populate it with
/// [`VerifiedStateProvider::fetch_account`] (async, verifies proofs), then hand it to the REVM
/// executor, which reads it synchronously via [`DatabaseRef`].
#[derive(Debug, Clone)]
pub struct VerifiedStateProvider {
    block: VerifiedL2Block,
    accounts: HashMap<Address, AccountInfo>,
    storage: HashMap<(Address, U256), U256>,
    code: HashMap<B256, Bytecode>,
}

impl VerifiedStateProvider {
    pub fn new(block: VerifiedL2Block) -> Self {
        Self {
            block,
            accounts: HashMap::new(),
            storage: HashMap::new(),
            code: HashMap::new(),
        }
    }

    pub fn block(&self) -> VerifiedL2Block {
        self.block
    }

    /// Fetches `address` (and the given storage `slots`) at the verified block, verifies every
    /// proof against the verified state root, and records the verified values. Bytecode is
    /// fetched separately and checked against the account's `codeHash`.
    pub async fn fetch_account(
        &mut self,
        l2_rpc: &str,
        address: Address,
        slots: &[U256],
    ) -> Result<()> {
        let provider = ProviderBuilder::new().connect_http(l2_rpc.parse()?);
        let block_id = BlockId::hash(self.block.hash);

        let slot_keys: Vec<B256> = slots.iter().map(|s| B256::from(*s)).collect();
        let proof = provider
            .get_proof(address, slot_keys.clone())
            .block_id(block_id)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("missing trie node") || msg.contains("is not available") {
                    eyre!(
                        "state for L2 block {} is pruned on this RPC (full nodes keep ~128 recent \
                         states). Reading the confirmed (historical) block needs an ARCHIVE \
                         endpoint — set ROBINHOOD_RPC_URL to an archive Robinhood RPC. Underlying: {msg}",
                        self.block.number
                    )
                } else {
                    eyre!(msg)
                }
            })?;

        // Verify the account proof against the trusted state root.
        let account_key = Nibbles::unpack(keccak256(address));
        let is_empty = proof.nonce == 0
            && proof.balance.is_zero()
            && (proof.code_hash == B256::ZERO || proof.code_hash == KECCAK_EMPTY)
            && (proof.storage_hash == B256::ZERO || proof.storage_hash == EMPTY_ROOT_HASH);

        let expected_account_rlp = if is_empty {
            None
        } else {
            let trie_account = TrieAccount {
                nonce: proof.nonce,
                balance: proof.balance,
                storage_root: proof.storage_hash,
                code_hash: proof.code_hash,
            };
            Some(alloy_rlp::encode(trie_account))
        };
        verify_proof(
            self.block.state_root,
            account_key,
            expected_account_rlp,
            proof.account_proof.iter(),
        )
        .map_err(|e| eyre!("account proof verification failed for {address:#x}: {e}"))?;

        // Verify each storage slot proof against the account's storage root.
        for storage_proof in &proof.storage_proof {
            let slot = U256::from_be_bytes(storage_proof.key.as_b256().0);
            let slot_key = Nibbles::unpack(keccak256(storage_proof.key.as_b256()));
            let expected = if storage_proof.value.is_zero() {
                None
            } else {
                Some(alloy_rlp::encode(storage_proof.value))
            };
            verify_proof(
                proof.storage_hash,
                slot_key,
                expected,
                storage_proof.proof.iter(),
            )
            .map_err(|e| eyre!("storage proof verification failed for {address:#x} slot {slot}: {e}"))?;

            self.storage.insert((address, slot), storage_proof.value);
        }

        // Fetch and verify bytecode against the (proof-verified) code hash.
        let code = if is_empty || proof.code_hash == KECCAK_EMPTY {
            Bytecode::default()
        } else {
            let bytes: Bytes = provider.get_code_at(address).block_id(block_id).await?;
            let got = keccak256(&bytes);
            if got != proof.code_hash {
                return Err(eyre!(
                    "bytecode hash mismatch for {address:#x}: got {got:#x}, expected {:#x}",
                    proof.code_hash
                ));
            }
            let bc = Bytecode::new_raw(bytes.0.into());
            self.code.insert(proof.code_hash, bc.clone());
            bc
        };

        let info = AccountInfo::new(
            proof.balance,
            proof.nonce,
            if is_empty { KECCAK_EMPTY } else { proof.code_hash },
            code,
        );
        self.accounts.insert(address, info);
        Ok(())
    }
}

/// keccak256 of empty input — REVM's canonical "no code" hash.
const KECCAK_EMPTY: B256 = B256::new([
    0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0,
    0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
]);

impl DatabaseRef for VerifiedStateProvider {
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

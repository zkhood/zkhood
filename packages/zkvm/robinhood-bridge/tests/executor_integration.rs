//! Integration: the verified L2 provider is a drop-in for the REVM executor's state source.
//! This proves Phase 2 "D" wiring — `apply_batch` runs against `VerifiedStateProvider` exactly
//! as it does against the mock — using an empty batch so it needs no network access.

use zkhood_revm_executor::{apply_batch, BatchContext};
use zkhood_robinhood_bridge::{VerifiedL2Block, VerifiedStateProvider};

#[test]
fn verified_provider_plugs_into_apply_batch() {
    let block = VerifiedL2Block {
        number: 1,
        hash: Default::default(),
        state_root: Default::default(),
    };
    let provider = VerifiedStateProvider::new(block);

    let ctx = BatchContext {
        chain_id: 46630,
        rollup_address: [0u8; 20],
        batch_number: 1,
        tee_batch_nonce: 0,
    };

    let (outcomes, pv) = apply_batch(provider, &ctx, [0u8; 32], Vec::new())
        .expect("empty batch against verified provider must succeed");

    assert!(outcomes.is_empty());
    assert_eq!(pv.tx_count, 0);
    assert_eq!(pv.chain_id, 46630);
}

//! The TEE worker executes real REVM and persists rollup state across batches. This checks that a
//! value transfer actually moves balances in the overlay and that the batch commitments chain.

use revm::context::TxEnv;
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;
use zkhood_revm_executor::{
    apply_batch_with_overlay, BatchContext, MockRobinhoodStateProvider, OverlayState,
};

fn ctx(n: u64) -> BatchContext {
    BatchContext { chain_id: 46630, rollup_address: [0u8; 20], batch_number: n, tee_batch_nonce: n }
}

fn transfer(sender: Address, recipient: Address, value: u128, nonce: u64) -> TxEnv {
    TxEnv {
        caller: sender,
        kind: TxKind::Call(recipient),
        value: U256::from(value),
        gas_limit: 21_000,
        gas_price: 0,
        chain_id: Some(46630),
        nonce,
        ..Default::default()
    }
}

#[test]
fn fast_path_moves_value_and_persists() {
    let sender = Address::from([0x11u8; 20]);
    let recipient = Address::from([0x33u8; 20]);

    let mut base = MockRobinhoodStateProvider::new();
    base.set_account(
        sender,
        AccountInfo { balance: U256::from(1_000_000u128), nonce: 0, ..Default::default() },
    );
    let overlay = OverlayState::new();

    let (_o, pv1, overlay) =
        apply_batch_with_overlay(base.clone(), overlay, &ctx(1), vec![transfer(sender, recipient, 1000, 0)])
            .expect("batch 1");
    assert_eq!(pv1.tx_count, 1);
    assert_eq!(pv1.chain_id, 46630);
    // The transfer actually moved value (the old bug left balances untouched / empty state).
    assert_eq!(overlay.balance_of(&sender), U256::from(999_000u128));
    assert_eq!(overlay.balance_of(&recipient), U256::from(1000u128));

    let (_o2, pv2, _overlay) =
        apply_batch_with_overlay(base, overlay, &ctx(2), vec![transfer(sender, recipient, 1, 1)])
            .expect("batch 2");
    // Batches chain: batch 2's old commitment == batch 1's new commitment.
    assert_eq!(pv2.old_state_commitment, pv1.new_state_commitment);
    // Real state, not the empty-state artifact keccak256("").
    assert_ne!(pv1.new_state_commitment, [0u8; 32]);
}

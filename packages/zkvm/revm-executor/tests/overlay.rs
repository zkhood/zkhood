//! Proves the rollup overlay carries real state (balances) across batches: a second transfer from
//! the same sender sees the balance the first transfer left behind, not the original base balance.

use revm::context::TxEnv;
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;
use zkhood_revm_executor::{apply_batch_with_overlay, BatchContext, MockRobinhoodStateProvider, OverlayState};

const CHAIN_ID: u64 = 46630;

fn ctx(batch_number: u64) -> BatchContext {
    BatchContext {
        chain_id: CHAIN_ID,
        rollup_address: [0u8; 20],
        batch_number,
        tee_batch_nonce: 0,
    }
}

fn transfer(sender: Address, recipient: Address, value: u64, nonce: u64) -> TxEnv {
    TxEnv {
        caller: sender,
        kind: TxKind::Call(recipient),
        value: U256::from(value),
        gas_limit: 21_000,
        gas_price: 0,
        chain_id: Some(CHAIN_ID),
        nonce,
        ..Default::default()
    }
}

#[test]
fn overlay_persists_balances_across_batches() {
    let sender = Address::from([0x11u8; 20]);
    let recipient = Address::from([0x33u8; 20]);

    let mut base = MockRobinhoodStateProvider::new();
    base.set_account(
        sender,
        AccountInfo {
            balance: U256::from(1_000u64),
            nonce: 0,
            ..Default::default()
        },
    );

    // Batch 1: send 300. Overlay must record sender=700, recipient=300.
    let overlay = OverlayState::new();
    let (_o1, pv1, overlay) =
        apply_batch_with_overlay(base.clone(), overlay, &ctx(1), vec![transfer(sender, recipient, 300, 0)])
            .expect("batch 1 must execute");
    assert_eq!(overlay.balance_of(&sender), U256::from(700u64));
    assert_eq!(overlay.balance_of(&recipient), U256::from(300u64));

    // Batch 2: same base, but the overlay now shadows the sender at 700. Send 300 again (nonce 1).
    // If persistence works, sender ends at 400 and recipient at 600.
    let (_o2, pv2, overlay) =
        apply_batch_with_overlay(base.clone(), overlay, &ctx(2), vec![transfer(sender, recipient, 300, 1)])
            .expect("batch 2 must execute");
    assert_eq!(overlay.balance_of(&sender), U256::from(400u64));
    assert_eq!(overlay.balance_of(&recipient), U256::from(600u64));

    // Commitments chain: batch 2's old == batch 1's new.
    assert_eq!(pv2.old_state_commitment, pv1.new_state_commitment);
    assert_ne!(pv1.new_state_commitment, pv2.new_state_commitment);
}

#[test]
fn overlay_rejects_overspend_after_prior_batch() {
    let sender = Address::from([0x11u8; 20]);
    let recipient = Address::from([0x33u8; 20]);

    let mut base = MockRobinhoodStateProvider::new();
    base.set_account(
        sender,
        AccountInfo {
            balance: U256::from(1_000u64),
            nonce: 0,
            ..Default::default()
        },
    );

    // Spend down to 100, then a 500 transfer must fail because the overlay (not the base) is read.
    let overlay = OverlayState::new();
    let (_o, _pv, overlay) =
        apply_batch_with_overlay(base.clone(), overlay, &ctx(1), vec![transfer(sender, recipient, 900, 0)])
            .expect("batch 1 must execute");
    assert_eq!(overlay.balance_of(&sender), U256::from(100u64));

    let result = apply_batch_with_overlay(base, overlay, &ctx(2), vec![transfer(sender, recipient, 500, 1)]);
    assert!(result.is_err(), "overspend against overlay balance must be rejected");
}

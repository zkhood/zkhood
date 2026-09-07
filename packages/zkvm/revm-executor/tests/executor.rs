//! Validates real REVM execution (not a custom ledger): a simple value transfer, and the core
//! safety property that running the same batch against the same starting state twice produces
//! identical public values.

use revm::context::TxEnv;
use revm::context_interface::result::ExecutionResult;
use revm::primitives::{TxKind, U256};
use revm::state::AccountInfo;
use zkhood_revm_executor::{apply_batch, keccak256, BatchContext, MockRobinhoodStateProvider};

const CHAIN_ID: u64 = 1;
const ROLLUP_ADDRESS: [u8; 20] = [0x22; 20];

fn default_ctx() -> BatchContext {
    BatchContext {
        chain_id: CHAIN_ID,
        rollup_address: ROLLUP_ADDRESS,
        batch_number: 1,
        tee_batch_nonce: 1,
    }
}

fn funded_provider(sender: revm::primitives::Address, balance: u128) -> MockRobinhoodStateProvider {
    let mut provider = MockRobinhoodStateProvider::new();
    provider.set_account(
        sender,
        AccountInfo {
            balance: U256::from(balance),
            nonce: 0,
            ..Default::default()
        },
    );
    provider
}

fn simple_transfer_tx(sender: revm::primitives::Address, recipient: revm::primitives::Address, value: u128) -> TxEnv {
    TxEnv {
        caller: sender,
        kind: TxKind::Call(recipient),
        value: U256::from(value),
        gas_limit: 21_000,
        gas_price: 0,
        chain_id: Some(CHAIN_ID),
        nonce: 0,
        ..Default::default()
    }
}

#[test]
fn simple_value_transfer_succeeds() {
    let sender = revm::primitives::Address::from([0x11u8; 20]);
    let recipient = revm::primitives::Address::from([0x33u8; 20]);
    let provider = funded_provider(sender, 1_000_000_000_000_000_000);
    let old_commitment = keccak256(b"genesis");

    let tx = simple_transfer_tx(sender, recipient, 1_000);
    let (outcomes, public_values) =
        apply_batch(provider, &default_ctx(), old_commitment, vec![tx]).expect("batch applies");

    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0].result, ExecutionResult::Success { .. }));
    assert_eq!(public_values.tx_count, 1);
    assert_ne!(public_values.old_state_commitment, public_values.new_state_commitment);
}

#[test]
fn same_batch_from_same_state_is_deterministic() {
    let sender = revm::primitives::Address::from([0x11u8; 20]);
    let recipient = revm::primitives::Address::from([0x33u8; 20]);
    let old_commitment = keccak256(b"genesis");

    let provider_a = funded_provider(sender, 1_000_000_000_000_000_000);
    let provider_b = funded_provider(sender, 1_000_000_000_000_000_000);

    let tx_a = simple_transfer_tx(sender, recipient, 1_000);
    let tx_b = simple_transfer_tx(sender, recipient, 1_000);

    let (_, public_values_a) =
        apply_batch(provider_a, &default_ctx(), old_commitment, vec![tx_a]).expect("batch applies");
    let (_, public_values_b) =
        apply_batch(provider_b, &default_ctx(), old_commitment, vec![tx_b]).expect("batch applies");

    // The whole point of the TEE+ZK design: independent runs of the same batch against the
    // same starting state must agree byte-for-byte on the resulting public values.
    assert_eq!(public_values_a, public_values_b);
}

#[test]
fn insufficient_balance_transfer_fails() {
    let sender = revm::primitives::Address::from([0x44u8; 20]);
    let recipient = revm::primitives::Address::from([0x55u8; 20]);
    let provider = funded_provider(sender, 10);
    let old_commitment = keccak256(b"genesis");

    let tx = simple_transfer_tx(sender, recipient, 1_000);
    // revm may reject this outright as an invalid transaction (insufficient funds check before
    // execution), or execute it and halt/revert — either is an acceptable rejection here.
    match apply_batch(provider, &default_ctx(), old_commitment, vec![tx]) {
        Ok((outcomes, _)) => assert!(!matches!(outcomes[0].result, ExecutionResult::Success { .. })),
        Err(_) => {}
    }
}

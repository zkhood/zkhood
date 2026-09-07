//! Prints a ready-to-POST JSON body for /execute-batch: a funded sender transferring value.
//! Usage: cargo run --example gen_request  (pipe into curl)

use revm::context::TxEnv;
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;
use serde_json::json;
use zkhood_revm_executor::{keccak256, BatchContext, MockRobinhoodStateProvider};

fn main() {
    let sender = Address::from([0x11u8; 20]);
    let recipient = Address::from([0x33u8; 20]);

    let mut state = MockRobinhoodStateProvider::new();
    state.set_account(
        sender,
        AccountInfo {
            balance: U256::from(1_000_000_000_000_000_000u128),
            nonce: 0,
            ..Default::default()
        },
    );

    let tx = TxEnv {
        caller: sender,
        kind: TxKind::Call(recipient),
        value: U256::from(1_000u64),
        gas_limit: 21_000,
        gas_price: 0,
        chain_id: Some(46630),
        nonce: 0,
        ..Default::default()
    };

    let ctx = BatchContext {
        chain_id: 46630,
        rollup_address: [0u8; 20],
        batch_number: 1,
        tee_batch_nonce: 0,
    };

    let body = json!({
        "ctx": ctx,
        "old_state_commitment": keccak256(b"genesis"),
        "txs": [tx],
        "mock_state": state,
    });

    println!("{}", serde_json::to_string(&body).unwrap());
}

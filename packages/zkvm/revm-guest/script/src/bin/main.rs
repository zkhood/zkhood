//! An end-to-end example of using the SP1 SDK to generate a proof of a program that can be executed
//! or have a core proof generated.
//!
//! You can run this script using the following command:
//! ```shell
//! RUST_LOG=info cargo run --release -- --execute
//! ```
//! or
//! ```shell
//! RUST_LOG=info cargo run --release -- --prove
//! ```

//! End-to-end example: build a small batch (one funded sender, one real EVM value transfer),
//! run it through the SP1 zkVM with real REVM execution, and cross-check the guest's public
//! values against a plain native call to the same `apply_batch` function.
//!
//! ```shell
//! RUST_LOG=info cargo run --release --bin zkhood-revm-guest -- --execute
//! ```
//! or
//! ```shell
//! RUST_LOG=info cargo run --release --bin zkhood-revm-guest -- --prove
//! ```

use alloy_sol_types::SolType;
use clap::Parser;
use revm::context::TxEnv;
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;
use sp1_sdk::{
    blocking::{ProveRequest, Prover, ProverClient},
    include_elf, Elf, ProvingKey, SP1Stdin,
};
use zkhood_revm_executor::{apply_batch, keccak256, BatchContext, MockRobinhoodStateProvider};
use zkhood_revm_guest_lib::{BatchWitness, PublicValuesStruct};

/// The ELF (executable and linkable format) file for the Succinct RISC-V zkVM.
const REVM_ELF: Elf = include_elf!("zkhood-revm-guest-program");

const CHAIN_ID: u64 = 1;
const ROLLUP_ADDRESS: [u8; 20] = [0x22; 20];

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    execute: bool,

    #[arg(long)]
    prove: bool,
}

fn default_ctx() -> BatchContext {
    BatchContext {
        chain_id: CHAIN_ID,
        rollup_address: ROLLUP_ADDRESS,
        batch_number: 1,
        tee_batch_nonce: 1,
    }
}

/// Builds a fresh copy of the example batch each call: a funded sender transferring value to a
/// recipient. Called twice (once for the native cross-check, once for the SP1 witness) instead
/// of cloning, since a real batch's transactions are not expected to be replayed verbatim.
fn build_example_batch() -> (BatchContext, [u8; 32], Vec<TxEnv>, MockRobinhoodStateProvider) {
    let sender = Address::from([0x11u8; 20]);
    let recipient = Address::from([0x33u8; 20]);

    let mut provider = MockRobinhoodStateProvider::new();
    provider.set_account(
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
        chain_id: Some(CHAIN_ID),
        nonce: 0,
        ..Default::default()
    };

    let old_state_commitment = keccak256(b"genesis");
    (default_ctx(), old_state_commitment, vec![tx], provider)
}

fn main() {
    sp1_sdk::utils::setup_logger();
    dotenv::dotenv().ok();

    let args = Args::parse();
    if args.execute == args.prove {
        eprintln!("Error: You must specify either --execute or --prove");
        std::process::exit(1);
    }

    // Cross-check: apply the exact same batch natively, outside the zkVM, so we can compare
    // against whatever the guest program commits to inside SP1.
    let (native_ctx, native_old_commitment, native_txs, native_provider) = build_example_batch();
    let native_public_values = apply_batch(native_provider, &native_ctx, native_old_commitment, native_txs)
        .expect("native batch must be valid")
        .1;

    let client = ProverClient::from_env();

    let (ctx, old_state_commitment, txs, provider) = build_example_batch();
    let witness = BatchWitness {
        ctx,
        old_state_commitment,
        txs,
        provider,
    };
    let mut stdin = SP1Stdin::new();
    stdin.write(&witness);

    if args.execute {
        let (output, report) = client.execute(REVM_ELF, stdin).run().unwrap();
        println!("Program executed successfully.");

        let public_values = PublicValuesStruct::abi_decode(output.as_slice()).unwrap();
        println!("chainId: {}", public_values.chainId);
        println!("batchNumber: {}", public_values.batchNumber);
        println!("oldStateCommitment: {:?}", public_values.oldStateCommitment);
        println!("newStateCommitment: {:?}", public_values.newStateCommitment);
        println!("txCount: {}", public_values.txCount);
        println!("Number of cycles: {}", report.total_instruction_count());

        // The whole point of this design: the TEE's native REVM execution and the SP1 guest's
        // proved REVM execution must agree byte-for-byte on the resulting public values.
        assert_eq!(
            public_values.oldStateCommitment.as_slice(),
            native_public_values.old_state_commitment
        );
        assert_eq!(
            public_values.newStateCommitment.as_slice(),
            native_public_values.new_state_commitment
        );
        assert_eq!(public_values.txCount, native_public_values.tx_count);
        println!("Native and zkVM-guest public values match!");
    } else {
        let pk = client.setup(REVM_ELF).expect("failed to setup elf");
        let proof = client.prove(&pk, stdin).run().expect("failed to generate proof");
        println!("Successfully generated proof!");
        client
            .verify(&proof, pk.verifying_key(), None)
            .expect("failed to verify proof");
        println!("Successfully verified proof!");
    }
}

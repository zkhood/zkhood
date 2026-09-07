//! An end-to-end example of using the SP1 SDK to generate a proof of a program that can have an
//! EVM-Compatible proof generated which can be verified on-chain.
//!
//! You can run this script using the following command:
//! ```shell
//! RUST_LOG=info cargo run --release --bin evm -- --system groth16
//! ```
//! or
//! ```shell
//! RUST_LOG=info cargo run --release --bin evm -- --system plonk
//! ```

//! Generates an EVM-verifiable (Groth16 or Plonk) proof of the example REVM batch, and writes a
//! fixture usable to test Solidity verification once `Rollup.sol` is wired to the SP1 verifier.
//!
//! ```shell
//! RUST_LOG=info cargo run --release --bin evm -- --system groth16
//! ```

use alloy_sol_types::SolType;
use clap::{Parser, ValueEnum};
use revm::context::TxEnv;
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;
use serde::{Deserialize, Serialize};
use sp1_sdk::{
    blocking::{ProveRequest, Prover, ProverClient},
    include_elf, Elf, HashableKey, ProvingKey, SP1ProofWithPublicValues, SP1Stdin, SP1VerifyingKey,
};
use std::path::PathBuf;
use zkhood_revm_executor::{keccak256, BatchContext, MockRobinhoodStateProvider};
use zkhood_revm_guest_lib::{BatchWitness, PublicValuesStruct};

const REVM_ELF: Elf = include_elf!("zkhood-revm-guest-program");
const CHAIN_ID: u64 = 1;
const ROLLUP_ADDRESS: [u8; 20] = [0x22; 20];

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct EVMArgs {
    #[arg(long, value_enum, default_value = "groth16")]
    system: ProofSystem,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum ProofSystem {
    Plonk,
    Groth16,
}

/// A fixture usable to test verification of ZKHood SP1 REVM-batch proofs inside Solidity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZkHoodRevmBatchProofFixture {
    vkey: String,
    public_values: String,
    proof: String,
}

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

    let ctx = BatchContext {
        chain_id: CHAIN_ID,
        rollup_address: ROLLUP_ADDRESS,
        batch_number: 1,
        tee_batch_nonce: 1,
    };
    (ctx, keccak256(b"genesis"), vec![tx], provider)
}

fn main() {
    sp1_sdk::utils::setup_logger();
    let args = EVMArgs::parse();
    let client = ProverClient::from_env();
    let pk = client.setup(REVM_ELF).expect("failed to setup elf");

    let (ctx, old_state_commitment, txs, provider) = build_example_batch();
    let mut stdin = SP1Stdin::new();
    stdin.write(&BatchWitness {
        ctx,
        old_state_commitment,
        txs,
        provider,
    });

    println!("Proof System: {:?}", args.system);
    let proof = match args.system {
        ProofSystem::Plonk => client.prove(&pk, stdin).plonk().run(),
        ProofSystem::Groth16 => client.prove(&pk, stdin).groth16().run(),
    }
    .expect("failed to generate proof");

    create_proof_fixture(&proof, pk.verifying_key(), args.system);
}

fn create_proof_fixture(proof: &SP1ProofWithPublicValues, vk: &SP1VerifyingKey, system: ProofSystem) {
    let bytes = proof.public_values.as_slice();
    let _decoded = PublicValuesStruct::abi_decode(bytes).unwrap();

    let fixture = ZkHoodRevmBatchProofFixture {
        vkey: vk.bytes32().to_string(),
        public_values: format!("0x{}", hex::encode(bytes)),
        proof: format!("0x{}", hex::encode(proof.bytes())),
    };

    println!("Verification Key: {}", fixture.vkey);
    println!("Public Values: {}", fixture.public_values);

    // The proof proves to the verifier that the program was executed with some inputs that led to
    // the give public values.
    println!("Proof Bytes: {}", fixture.proof);

    // Save the fixture to a file.
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/src/fixtures");
    std::fs::create_dir_all(&fixture_path).expect("failed to create fixture path");
    std::fs::write(
        fixture_path.join(format!("{:?}-fixture.json", system).to_lowercase()),
        serde_json::to_string_pretty(&fixture).unwrap(),
    )
    .expect("failed to write fixture");
}

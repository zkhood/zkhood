use sp1_sdk::{blocking::MockProver, blocking::Prover, include_elf, Elf, HashableKey, ProvingKey};

/// The ELF (executable and linkable format) file for the Succinct RISC-V zkVM.
const REVM_ELF: Elf = include_elf!("zkhood-revm-guest-program");

fn main() {
    let prover = MockProver::new();
    let pk = prover.setup(REVM_ELF).expect("failed to setup elf");
    println!("{}", pk.verifying_key().bytes32());
}

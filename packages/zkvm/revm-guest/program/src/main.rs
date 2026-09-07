//! The SP1 guest program: reads a [`BatchWitness`], applies it with the exact same
//! `zkhood-revm-executor` crate the TEE worker uses natively, and commits the resulting public
//! values so a proof of this real REVM execution can be verified on-chain.

#![no_main]
sp1_zkvm::entrypoint!(main);

use zkhood_revm_guest_lib::{encode_public_values, run_batch, BatchWitness};

pub fn main() {
    // Read the batch witness (context, ordered transactions, pre-batch state).
    //
    // Behind the scenes, this compiles down to a custom system call which handles reading
    // inputs from the prover.
    let witness = sp1_zkvm::io::read::<BatchWitness>();

    // Apply the batch with the exact same REVM executor the TEE worker runs natively.
    let public_values = run_batch(witness);

    // Commit to the public values of the program. The final proof will have a commitment to
    // all the bytes that were committed to.
    let bytes = encode_public_values(&public_values);
    sp1_zkvm::io::commit_slice(&bytes);
}

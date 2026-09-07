# zkhood SP1 guest — proves the TEE's REVM execution

This is the [SP1](https://github.com/succinctlabs/sp1) zkVM program for zkhood. It runs the **exact
same** `zkhood-revm-executor` batch execution that the TEE worker runs, and emits public values
that match what the TEE produced. A verifier can therefore check a validity proof that attests to
precisely the execution the TEE performed — "TEE for speed, ZK for finality."

## Layout

| Crate | Role |
|-------|------|
| `lib` | Defines the batch witness + the `PublicValuesStruct` (ABI-encoded, matches the on-chain decoder). |
| `program` | The `#![no_main]` SP1 guest: reads the witness, runs `zkhood-revm-executor`, commits the public values. |
| `script` | Host runner: builds an example batch, runs it both natively and inside the zkVM, and asserts the two agree. |

## Requirements

- [Rust](https://rustup.rs/) and the [SP1 toolchain](https://docs.succinct.xyz/docs/sp1/getting-started/install)

## Execute (no real proof — fast, for development)

```sh
cd script
SP1_PROVER=mock cargo run --release --bin zkhood-revm-guest -- --execute
```

This runs the batch natively and inside the zkVM and prints:

```
Native and zkVM-guest public values match!
```

which is the core correctness property: the proven execution equals the TEE's execution.

## Generate a real proof

Real proving needs the SP1 Prover Network or a large machine/GPU (see `.env.example`):

```sh
cd script
SP1_PROVER=network NETWORK_PRIVATE_KEY=... cargo run --release --bin zkhood-revm-guest -- --prove
```

Print the program's verifying key (pin this in the settlement contract):

```sh
cd script
cargo run --release --bin vkey
```

## License

MIT.

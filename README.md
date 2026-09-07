# zkhood

**Private stock settlement on Robinhood Chain — TEE for speed, ZK for finality.**

zkhood is a privacy layer that settles on
[Robinhood Chain](https://docs.robinhood.com/chain) (an Arbitrum-style L2). Batches are executed
**instantly and privately inside a TEE** running a real EVM, and the *same* execution is then
**proven with a zkVM** and settled on-chain. This is the design summarized as **Option A: "TEE for
speed, ZK for finality."**

## How it fits together

```
        ┌──────────────────────────────────────────────────────────────────┐
        │  users buy stock privately  ──►  batch of transactions            │
        └──────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
   ┌───────────────────────────────────────────────────────────────────────────┐
   │  TEE worker (Marlin Oyster CVM)          packages/zkvm/tee-worker           │
   │  • executes the batch with REAL REVM, instantly & privately                 │
   │  • reads Robinhood Chain state that it VERIFIES itself (never trusts an RPC) │
   │  • serves a remote-attestation so clients know it's a genuine TEE           │
   └───────────────────────────────────────────────────────────────────────────┘
        │ verified L2 state                     │ same execution, proven later
        ▼                                        ▼
   ┌──────────────────────────────┐   ┌──────────────────────────────────────────┐
   │ robinhood-bridge             │   │ revm-guest (SP1 zkVM)                      │
   │ trustless L1→L2 verification │   │ proves the identical REVM execution        │
   │ Helios (Ethereum L1) +       │   │ → produces a validity proof + public values│
   │ Merkle-Patricia proofs       │   └──────────────────────────────────────────┘
   └──────────────────────────────┘                    │ proof + public values
        ▲ real REVM engine                              ▼
   ┌──────────────────────────────┐   ┌──────────────────────────────────────────┐
   │ revm-executor                │   │ contracts (Solidity)                      │
   │ shared REVM batch execution  │   │ ZkHoodRollup verifies the SP1 proof and    │
   │ used identically by TEE & ZK │   │ advances settled state on Robinhood Chain  │
   └──────────────────────────────┘   └──────────────────────────────────────────┘
```

The key property: **the TEE-native execution and the SP1-proven execution produce byte-identical
public values.** They run the exact same `revm-executor` code, so the proof attests to precisely
what the TEE did.

## Components

| Path | What it is | Status |
|------|------------|--------|
| [`packages/zkvm/revm-executor`](packages/zkvm/revm-executor) | Shared REVM batch execution + a durable rollup overlay + state commitment. Used identically by the TEE worker and the SP1 guest. | Working, tested |
| [`packages/zkvm/robinhood-bridge`](packages/zkvm/robinhood-bridge) | Trustless read access to Robinhood Chain (L2) state: Ethereum L1 via a Helios light client, then Merkle-Patricia proofs. No RPC is trusted. | Testnet, validated live |
| [`packages/zkvm/tee-worker`](packages/zkvm/tee-worker) | The TEE fast-path service (HTTP): executes batches with real REVM, keeps durable rollup state, exposes a remote attestation. Packaged for Marlin Oyster CVM. | Working, tested |
| [`packages/zkvm/revm-guest`](packages/zkvm/revm-guest) | SP1 zkVM program that proves the identical REVM execution and emits public values matching the on-chain decoder. | Execute-mode validated |

The on-chain settlement contracts (the SP1 verifier + rollup that settle these public values on
Robinhood Chain) are published in a separate repository.

## Build & test

**Rust workspace** (executor, bridge, TEE worker):

```bash
cd packages/zkvm
cargo test --workspace
```

**SP1 guest** (needs the SP1 toolchain — https://docs.succinct.xyz):

```bash
cd packages/zkvm/revm-guest/script
SP1_PROVER=mock cargo run --release --bin zkhood-revm-guest -- --execute
# prints "Native and zkVM-guest public values match!"
```

## Running against a live network

The bridge verifies L1 through a **Helios light client you run yourself** (it turns an untrusted RPC
into a locally-verified one):

```bash
cd packages/zkvm/robinhood-bridge
cp .env.example .env         # fill in your own RPC endpoints — never commit real keys
./scripts/run-helios-sepolia.sh
cargo run --example verify_testnet_anchor
```

Deploying the TEE worker to Marlin Oyster:

```bash
cd packages/zkvm/tee-worker
export OYSTER_WALLET_KEY=0x...        # env only, never committed; funded on Arbitrum One
./scripts/deploy-oyster.sh
```

## License

MIT — provided "as is", without warranty. See [LICENSE](LICENSE).


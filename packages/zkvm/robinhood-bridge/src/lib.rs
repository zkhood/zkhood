//! Trustless bridge from Ethereum L1 to Robinhood Chain (L2) state, for the TEE-native host
//! only. Never link this crate into the SP1 guest: it does network I/O, which cannot run
//! inside (or be trusted from within) the zkVM.
//!
//! This crate does NOT embed a light client. It talks to a Helios sidecar process
//! (`helios ethereum --network sepolia ...`, run separately) over plain JSON-RPC, keeping this
//! crate's own dependency tree small while still getting Helios's full cryptographic
//! verification (Helios itself checks everything against Ethereum L1 consensus before
//! answering). See `l1_verifier` for the trust argument and sidecar details.
//!
//! Trust model: the SP1 proof only attests that REVM executed correctly given some input state.
//! It does NOT attest that the input state was correctly read from Robinhood Chain — that is
//! this crate's job, and its correctness is instead carried by the TEE's own remote attestation
//! (Option A: "TEE for speed, ZK for finality").

pub mod abi;
pub mod config;
pub mod l1_verifier;
pub mod l2_provider;

pub use config::NetworkConfig;
pub use l1_verifier::{verify_latest_l2_anchor, VerifiedL2Anchor, DEFAULT_HELIOS_RPC};
pub use l2_provider::{fetch_verified_state_root, VerifiedL2Block, VerifiedStateProvider};


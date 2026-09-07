//! Standalone check: verifies the latest confirmed Robinhood Chain (testnet) L2 block hash via
//! a Helios sidecar on Ethereum Sepolia, with zero trust in any single RPC provider.
//!
//! Setup:
//!   1. cp .env.example .env  and set SEPOLIA_EXECUTION_RPC (needs eth_getProof, e.g. Alchemy)
//!   2. ./scripts/run-helios-sepolia.sh   (starts the sidecar; reads the same .env)
//!   3. cargo run --example verify_testnet_anchor

use zkhood_robinhood_bridge::{
    config::NetworkConfig, fetch_verified_state_root, l1_verifier, VerifiedStateProvider,
    DEFAULT_HELIOS_RPC,
};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let helios_rpc = std::env::var("HELIOS_RPC").unwrap_or_else(|_| DEFAULT_HELIOS_RPC.to_string());
    // Untrusted RPC used only for the (self-verified) AssertionCreated log lookup. It needs a
    // generous eth_getLogs range, unlike Helios's execution RPC which needs eth_getProof.
    let untrusted_l1_rpc = std::env::var("UNTRUSTED_L1_RPC")
        .unwrap_or_else(|_| "https://ethereum-sepolia.publicnode.com".to_string());

    let network = NetworkConfig::testnet();

    // Step 1: L1-anchored, verified Robinhood L2 block hash.
    let anchor =
        l1_verifier::verify_latest_l2_anchor(&helios_rpc, &untrusted_l1_rpc, &network).await?;

    println!("Verified Robinhood Chain testnet anchor (via L1/Helios):");
    println!("  confirmed assertion hash: {:#x}", anchor.confirmed_assertion_hash);
    println!("  L2 block hash:            {:#x}", anchor.l2_block_hash);
    println!("  L2 send root:             {:#x}", anchor.l2_send_root);

    // Step 2: bind that hash to a verified L2 state root.
    // The L1-confirmed block is historical; the public Robinhood RPC prunes state, so an ARCHIVE
    // endpoint (e.g. Alchemy for Robinhood Chain) is needed here. Falls back to the public RPC.
    let l2_rpc = std::env::var("ROBINHOOD_RPC_URL").unwrap_or_else(|_| network.robinhood_rpc_url.clone());
    let block = fetch_verified_state_root(&l2_rpc, anchor.l2_block_hash).await?;
    println!("Verified L2 block:");
    println!("  number:     {}", block.number);
    println!("  state root: {:#x}", block.state_root);

    // Step 3: fetch + verify a concrete account (ArbSys precompile, always present) via a
    // Merkle-Patricia proof against the verified state root.
    let mut provider = VerifiedStateProvider::new(block);
    let arb_sys: alloy::primitives::Address =
        "0x0000000000000000000000000000000000000064".parse()?;
    provider.fetch_account(&l2_rpc, arb_sys, &[]).await?;
    println!("Verified account {arb_sys:#x} against L2 state root (MPT proof OK).");

    Ok(())
}


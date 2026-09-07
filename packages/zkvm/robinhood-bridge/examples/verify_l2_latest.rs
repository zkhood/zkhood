//! Validates the L2 verification code path (block-hash self-check + MPT account/storage proof)
//! against Robinhood testnet's CURRENT state, which a pruned public RPC always serves.
//!
//! This is a code-correctness check for the alloy-trie verification only — it anchors at the
//! latest block instead of an L1-confirmed one, so it is NOT the trustless path (see
//! verify_testnet_anchor for that). Run: cargo run --example verify_l2_latest

use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use zkhood_robinhood_bridge::{config::NetworkConfig, fetch_verified_state_root, VerifiedStateProvider};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let network = NetworkConfig::testnet();
    let l2_rpc = std::env::var("ROBINHOOD_RPC_URL").unwrap_or(network.robinhood_rpc_url);

    // Grab the current block hash straight from the RPC (untrusted here — this is only a
    // code-correctness check, not the L1-anchored trustless flow).
    let provider = ProviderBuilder::new().connect_http(l2_rpc.parse()?);
    let latest = provider
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .ok_or_else(|| eyre::eyre!("no latest block"))?;
    let latest_hash = latest.header.hash;

    let block = fetch_verified_state_root(&l2_rpc, latest_hash).await?;
    println!("Block-hash self-check OK. block {}, state root {:#x}", block.number, block.state_root);

    let mut vsp = VerifiedStateProvider::new(block);
    // ArbGasInfo precompile has code; ArbSys too. Verify an account with a storage slot read.
    let arb_sys = address!("0000000000000000000000000000000000000064");
    vsp.fetch_account(&l2_rpc, arb_sys, &[U256::ZERO]).await?;
    println!("MPT account+storage proof verification OK for {arb_sys:#x}.");

    Ok(())
}

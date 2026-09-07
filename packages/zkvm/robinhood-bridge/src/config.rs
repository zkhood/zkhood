//! Real, confirmed network parameters (docs.robinhood.com/chain, verified on Sepolia Etherscan).
//! Do not guess/replace these; if Robinhood Chain redeploys contracts, update here only.

use alloy::primitives::{address, Address};

/// Robinhood Chain testnet's Rollup contract on Ethereum Sepolia (the L1 anchor we verify).
pub const TESTNET_ROLLUP_ADDRESS: Address = address!("dc5F8E399DBd8a9F5F87AeC4C23Beb12431b386D");

/// Robinhood Chain mainnet's Rollup contract on Ethereum mainnet.
pub const MAINNET_ROLLUP_ADDRESS: Address = address!("23A19d23e89166adedbDcB432518AB01e4272D94");

/// Robinhood Chain testnet chain id.
pub const TESTNET_CHAIN_ID: u64 = 46630;

/// Robinhood Chain mainnet chain id.
pub const MAINNET_CHAIN_ID: u64 = 4663;

/// Public, rate-limited Robinhood Chain testnet RPC (fine for development; use a paid
/// provider such as Alchemy for production).
pub const TESTNET_ROBINHOOD_RPC: &str = "https://rpc.testnet.chain.robinhood.com";

/// Public, rate-limited Robinhood Chain mainnet RPC.
pub const MAINNET_ROBINHOOD_RPC: &str = "https://rpc.mainnet.chain.robinhood.com";

/// L1 (Sepolia) block where the testnet Rollup contract was created (verified on Etherscan).
pub const TESTNET_ROLLUP_DEPLOY_BLOCK: u64 = 10_204_516;

/// L1 (Ethereum) block where the mainnet Rollup contract was created.
// TODO: confirm the exact mainnet deploy block on Etherscan before mainnet use; 0 means
// "scan to genesis" as a safe (if slower) fallback.
pub const MAINNET_ROLLUP_DEPLOY_BLOCK: u64 = 0;

/// Network-specific parameters needed to run the L1 verifier + L2 provider against one
/// Robinhood Chain environment (testnet or mainnet).
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub rollup_address: Address,
    pub robinhood_chain_id: u64,
    pub robinhood_rpc_url: String,
    /// L1 block at which the Rollup contract was deployed; used as the floor when scanning
    /// backward for the `AssertionCreated` log so the search is bounded.
    pub rollup_deploy_block: u64,
}

impl NetworkConfig {
    pub fn testnet() -> Self {
        Self {
            rollup_address: TESTNET_ROLLUP_ADDRESS,
            robinhood_chain_id: TESTNET_CHAIN_ID,
            robinhood_rpc_url: TESTNET_ROBINHOOD_RPC.to_string(),
            rollup_deploy_block: TESTNET_ROLLUP_DEPLOY_BLOCK,
        }
    }

    pub fn mainnet() -> Self {
        Self {
            rollup_address: MAINNET_ROLLUP_ADDRESS,
            robinhood_chain_id: MAINNET_CHAIN_ID,
            robinhood_rpc_url: MAINNET_ROBINHOOD_RPC.to_string(),
            rollup_deploy_block: MAINNET_ROLLUP_DEPLOY_BLOCK,
        }
    }
}

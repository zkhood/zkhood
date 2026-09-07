//! Exact mirrors of the Arbitrum BOLD rollup contract's on-chain types (from
//! OffchainLabs/nitro-contracts: `state/GlobalState.sol`, `state/Machine.sol`,
//! `rollup/AssertionState.sol`, `rollup/Assertion.sol`, `rollup/IRollupCore.sol`).
//! Field order and enum variant order must match the Solidity source exactly, since they
//! determine the ABI encoding used to self-verify assertion hashes.

use alloy::sol;

sol! {
    struct GlobalState {
        bytes32[2] bytes32Vals;
        uint64[2] u64Vals;
    }

    enum MachineStatus {
        RUNNING,
        FINISHED,
        ERRORED
    }

    struct AssertionState {
        GlobalState globalState;
        MachineStatus machineStatus;
        bytes32 endHistoryRoot;
    }

    struct ConfigData {
        bytes32 wasmModuleRoot;
        uint256 requiredStake;
        address challengeManager;
        uint64 confirmPeriodBlocks;
        uint64 nextInboxPosition;
    }

    struct BeforeStateData {
        bytes32 prevPrevAssertionHash;
        bytes32 sequencerBatchAcc;
        ConfigData configData;
    }

    struct AssertionInputs {
        BeforeStateData beforeStateData;
        AssertionState beforeState;
        AssertionState afterState;
    }

    #[sol(rpc)]
    interface IRollupCore {
        function latestConfirmed() external view returns (bytes32);

        function computeAssertionHash(
            bytes32 prevAssertionHash,
            AssertionState calldata state,
            bytes32 inboxAcc
        ) external pure returns (bytes32);

        event AssertionCreated(
            bytes32 indexed assertionHash,
            bytes32 indexed parentAssertionHash,
            AssertionInputs assertion,
            bytes32 afterInboxBatchAcc,
            uint256 inboxMaxCount,
            bytes32 wasmModuleRoot,
            uint256 baseStake,
            address challengeManager,
            uint64 confirmPeriodBlocks
        );
    }
}

impl GlobalState {
    /// The Robinhood Chain (L2) block hash committed to by this assertion state.
    pub fn l2_block_hash(&self) -> alloy::primitives::B256 {
        self.bytes32Vals[0]
    }
}

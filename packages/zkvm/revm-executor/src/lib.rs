//! Shared REVM batch execution: the TEE worker runs this natively for the fast/private path,
//! and the SP1 guest program runs the identical code to produce a validity proof of the same
//! execution. Both must produce byte-identical [`executor::PublicValues`] for the same inputs.

pub mod commitment;
pub mod executor;
pub mod overlay;
pub mod state_provider;

pub use commitment::{keccak256, state_commitment, Hash};
pub use executor::{
    apply_batch, apply_batch_stateful, apply_batch_with_overlay, BatchContext, BatchError,
    PublicValues, TxOutcome,
};
pub use overlay::{OverlayDb, OverlayState};
pub use state_provider::{MockRobinhoodStateProvider, RobinhoodStateProvider};

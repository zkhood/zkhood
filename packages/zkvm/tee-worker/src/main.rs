//! TEE fast-path worker for the ZKHood rollup (Option A: "TEE for speed, ZK for finality").
//!
//! Runs inside a Marlin Oyster CVM. It executes batches with real REVM natively (instant, private)
//! and returns the exact [`PublicValues`] that the async SP1 prover will later prove and settle on
//! Robinhood Chain. Callers can verify via `/attestation` that they are talking to a genuine TEE
//! running the expected image before trusting any result.

mod attestation;
mod exec;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::Mutex;
use tracing::{info, warn};
use zkhood_revm_executor::OverlayState;

use crate::attestation::{fetch_attestation, DEFAULT_ATTESTATION_ENDPOINT};
use crate::exec::{execute_batch_mock, execute_batch_verified, ExecuteBatchRequest, WorkerConfig};
use zkhood_robinhood_bridge::{NetworkConfig, DEFAULT_HELIOS_RPC};

#[derive(Clone)]
struct AppState {
    /// Persistent rollup-private overlay (real balances) carried across batches.
    overlay: Arc<Mutex<OverlayState>>,
    /// Oyster attestation server endpoint (loopback inside the enclave).
    attestation_endpoint: String,
    /// Verified-state config; `None` means dev-only (mock) mode.
    worker_config: Option<WorkerConfig>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4000);
    let attestation_endpoint = std::env::var("ATTESTATION_ENDPOINT")
        .unwrap_or_else(|_| DEFAULT_ATTESTATION_ENDPOINT.to_string());

    // Verified-state mode is enabled when a Robinhood L2 RPC is configured; otherwise dev-only.
    let worker_config = std::env::var("ROBINHOOD_RPC_URL").ok().map(|robinhood_rpc| {
        let network = if std::env::var("ROBINHOOD_NETWORK").as_deref() == Ok("mainnet") {
            NetworkConfig::mainnet()
        } else {
            NetworkConfig::testnet()
        };
        WorkerConfig {
            helios_rpc: std::env::var("HELIOS_RPC").unwrap_or_else(|_| DEFAULT_HELIOS_RPC.to_string()),
            untrusted_l1_rpc: std::env::var("UNTRUSTED_L1_RPC")
                .unwrap_or_else(|_| "https://ethereum-sepolia.publicnode.com".to_string()),
            robinhood_rpc,
            network,
        }
    });
    if worker_config.is_some() {
        info!("verified-state mode enabled (Robinhood L2 RPC configured)");
    } else {
        info!("dev-only mode: set ROBINHOOD_RPC_URL to enable verified-state execution");
    }

    let state = AppState {
        overlay: Arc::new(Mutex::new(OverlayState::new())),
        attestation_endpoint,
        worker_config,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/attestation", get(attestation_handler))
        .route("/execute-batch", post(execute_batch_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("ZKHood TEE worker listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "zkhood-tee-worker" }))
}

async fn attestation_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(fetch_attestation(&state.attestation_endpoint).await)
}

async fn execute_batch_handler(
    State(state): State<AppState>,
    Json(req): Json<ExecuteBatchRequest>,
) -> impl IntoResponse {
    let overlay = state.overlay.lock().await.clone();
    // Verified path when the enclave is configured and the request isn't an explicit mock request.
    let result = match (&state.worker_config, req.mock_state.is_some()) {
        (Some(cfg), false) => execute_batch_verified(req, overlay, cfg).await,
        _ => execute_batch_mock(req, overlay),
    };
    match result {
        Ok((resp, new_overlay)) => {
            // Persist the real rollup state for the next batch.
            *state.overlay.lock().await = new_overlay;
            (StatusCode::OK, Json(serde_json::to_value(resp).unwrap())).into_response()
        }
        Err(e) => {
            warn!("execute-batch failed: {e}");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutting down");
}

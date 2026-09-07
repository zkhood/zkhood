//! Retrieves the enclave's remote-attestation document so callers can verify they are talking to
//! a genuine Marlin Oyster TEE running the expected image, before trusting any fast-path result.
//!
//! Inside an Oyster CVM an attestation server is reachable on the loopback interface; we proxy its
//! raw attestation out over our own API. Outside an enclave (local dev) it is simply unavailable.

use serde::Serialize;

/// Default in-enclave Oyster attestation server endpoint (raw attestation bytes).
pub const DEFAULT_ATTESTATION_ENDPOINT: &str = "http://127.0.0.1:1300/attestation/raw";

#[derive(Debug, Clone, Serialize)]
pub struct AttestationInfo {
    /// True if the raw attestation was retrieved from the local Oyster attestation server.
    pub available: bool,
    /// Hex-encoded raw attestation document, if available.
    pub attestation_hex: Option<String>,
    /// Human-readable status, e.g. why it is unavailable in local dev.
    pub status: String,
}

/// Fetches the raw attestation from the Oyster attestation server at `endpoint`.
pub async fn fetch_attestation(endpoint: &str) -> AttestationInfo {
    match try_fetch(endpoint).await {
        Ok(bytes) => AttestationInfo {
            available: true,
            attestation_hex: Some(hex::encode(bytes)),
            status: "attestation retrieved from Oyster enclave".to_string(),
        },
        Err(e) => AttestationInfo {
            available: false,
            attestation_hex: None,
            status: format!(
                "attestation unavailable (expected outside an Oyster enclave): {e}"
            ),
        },
    }
}

async fn try_fetch(endpoint: &str) -> Result<Vec<u8>, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let resp = client.get(endpoint).send().await?.error_for_status()?;
    Ok(resp.bytes().await?.to_vec())
}

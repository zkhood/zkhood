#!/usr/bin/env bash
# Starts a Helios light client sidecar for Ethereum Sepolia, exposing a locally-verified
# JSON-RPC endpoint at http://127.0.0.1:8545 for robinhood-bridge to use.
#
# Reads SEPOLIA_EXECUTION_RPC / SEPOLIA_CONSENSUS_RPC from a local .env if present.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="$SCRIPT_DIR/../.env"
if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

if [[ -z "${SEPOLIA_EXECUTION_RPC:-}" ]]; then
  echo "SEPOLIA_EXECUTION_RPC is not set. Copy .env.example to .env and set an" >&2
  echo "eth_getProof-capable Sepolia RPC (e.g. an Alchemy URL)." >&2
  exit 1
fi

: "${SEPOLIA_CONSENSUS_RPC:=http://unstable.sepolia.beacon-api.nimbus.team/}"

export PATH="$HOME/.helios-src/bin:$HOME/.helios/bin:$PATH"

exec helios ethereum \
  --network sepolia \
  --execution-rpc "$SEPOLIA_EXECUTION_RPC" \
  --consensus-rpc "$SEPOLIA_CONSENSUS_RPC" \
  --load-external-fallback

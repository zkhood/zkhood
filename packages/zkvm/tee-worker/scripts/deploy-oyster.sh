#!/usr/bin/env bash
# Deploys the ZKHood TEE worker to a Marlin Oyster CVM and verifies its remote attestation.
#
# Prereqs:
#   - oyster-cvm installed (https://docs.marlin.org/oyster/build-cvm/quickstart)
#   - a published image referenced in docker-compose.yml
#   - a wallet funded on Arbitrum One (~1 USDC + 0.001 ETH); pass its key via OYSTER_WALLET_KEY
#     (env only — never hardcode or paste secrets)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
DURATION_MINUTES="${DURATION_MINUTES:-15}"
ARCH="${ARCH:-amd64}"

if [[ -z "${OYSTER_WALLET_KEY:-}" ]]; then
  echo "Set OYSTER_WALLET_KEY to your Arbitrum One wallet private key (env only)." >&2
  exit 1
fi

if grep -q "REPLACE_ME" "$COMPOSE_FILE"; then
  echo "Edit docker-compose.yml: replace REPLACE_ME with your published image name first." >&2
  exit 1
fi

echo "Deploying ZKHood TEE worker to Oyster ($ARCH, ${DURATION_MINUTES}m)..."
oyster-cvm deploy \
  --wallet-private-key "$OYSTER_WALLET_KEY" \
  --duration-in-minutes "$DURATION_MINUTES" \
  --docker-compose "$COMPOSE_FILE" \
  --arch "$ARCH"

echo
echo "Note the enclave IP and image id printed above, then verify the attestation with:"
echo "  oyster-cvm verify --enclave-ip <ip> --image-id <image_id>"

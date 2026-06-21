#!/usr/bin/env bash
# ScoutChain — deploy all contracts to Stellar testnet or mainnet
# Usage: ./scripts/deploy.sh [testnet|mainnet]
set -euo pipefail

NETWORK="${1:-testnet}"
DEPLOYER="${DEPLOYER_SECRET:-}"

if [[ -z "$DEPLOYER" ]]; then
  echo "ERROR: Set DEPLOYER_SECRET env var to your Stellar secret key."
  exit 1
fi

WASM_DIR="target/wasm32-unknown-unknown/release"

if command -v sha256sum >/dev/null 2>&1; then
  hash_wasm() { sha256sum "$1" | awk '{print $1}'; }
else
  hash_wasm() { shasum -a 256 "$1" | awk '{print $1}'; }
fi

echo "==> Building contracts..."
cargo build --workspace --target wasm32-unknown-unknown --release

CONTRACTS=(registration verification progress scout_access)

declare -A CONTRACT_IDS
declare -A CONTRACT_WASM_HASHES

for name in "${CONTRACTS[@]}"; do
  wasm_name="scoutchain_${name}.wasm"
  optimized="${WASM_DIR}/scoutchain_${name}.optimized.wasm"

  echo "==> Optimizing $name..."
  stellar contract optimize --wasm "${WASM_DIR}/${wasm_name}" --wasm-out "$optimized"

  echo "==> Deploying $name to $NETWORK..."
  id=$(stellar contract deploy \
    --wasm "$optimized" \
    --source "$DEPLOYER" \
    --network "$NETWORK")

  CONTRACT_IDS[$name]="$id"
  CONTRACT_WASM_HASHES[$name]=$(hash_wasm "$optimized")
  echo "    $name => $id"
  echo "    $name wasm hash => ${CONTRACT_WASM_HASHES[$name]}"
done

# Write contract IDs and WASM hashes to .env.contracts
{
  echo "REGISTRATION_CONTRACT_ID=${CONTRACT_IDS[registration]}"
  echo "REGISTRATION_CONTRACT_WASM_HASH=${CONTRACT_WASM_HASHES[registration]}"
  echo "VERIFICATION_CONTRACT_ID=${CONTRACT_IDS[verification]}"
  echo "VERIFICATION_CONTRACT_WASM_HASH=${CONTRACT_WASM_HASHES[verification]}"
  echo "PROGRESS_CONTRACT_ID=${CONTRACT_IDS[progress]}"
  echo "PROGRESS_CONTRACT_WASM_HASH=${CONTRACT_WASM_HASHES[progress]}"
  echo "SCOUT_ACCESS_CONTRACT_ID=${CONTRACT_IDS[scout_access]}"
  echo "SCOUT_ACCESS_CONTRACT_WASM_HASH=${CONTRACT_WASM_HASHES[scout_access]}"
} > .env.contracts

echo ""
echo "==> All contracts deployed. IDs saved to .env.contracts"

#!/usr/bin/env bash
# ScoutChain — seed testnet with demo data.
# Run after initialize.sh to create test players, validators, and scouts.
#
# Idempotent: stellar keys generate is skipped if the named key already exists.
# Exits non-zero immediately if any step fails (set -euo pipefail).
set -euo pipefail

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() { echo "ERROR: $*" >&2; exit 1; }

require_nonempty() {
  local value="$1"
  local label="$2"
  [[ -n "$value" ]] || die "$label is empty. Re-run deploy.sh and initialize.sh first."
}

# ensure_key <name>
#   Creates a named Stellar key if it does not already exist.
#   Uses --no-fund so the key is just stored locally; Friendbot funds it later.
ensure_key() {
  local name="$1"
  if ! stellar keys show "$name" &>/dev/null; then
    stellar keys generate --no-fund "$name"
  fi
}

# ---------------------------------------------------------------------------
# Pre-flight: validate .env.contracts
# ---------------------------------------------------------------------------

[[ -f .env.contracts ]] || die ".env.contracts not found. Run ./scripts/deploy.sh and ./scripts/initialize.sh first."

# shellcheck source=/dev/null
source .env.contracts

require_nonempty "${REGISTRATION_CONTRACT_ID:-}"  "REGISTRATION_CONTRACT_ID"
require_nonempty "${VERIFICATION_CONTRACT_ID:-}"  "VERIFICATION_CONTRACT_ID"

NETWORK="testnet"
DEPLOYER="${DEPLOYER_SECRET:?Set DEPLOYER_SECRET in .env or environment}"
# ADMIN_ADDRESS is validated here even though not used directly in seed.sh —
# its presence confirms the .env is fully configured before continuing.
: "${ADMIN_ADDRESS:?Set ADMIN_ADDRESS in .env or environment}"

# ---------------------------------------------------------------------------
# Generate (or reuse) test keypairs
# ---------------------------------------------------------------------------

echo "==> Ensuring test keypairs exist..."
ensure_key player-test
ensure_key scout-test
ensure_key validator-test

PLAYER_ADDRESS=$(stellar keys address player-test)
SCOUT_ADDRESS=$(stellar keys address scout-test)
VALIDATOR_ADDRESS=$(stellar keys address validator-test)

echo "    Player:    $PLAYER_ADDRESS"
echo "    Scout:     $SCOUT_ADDRESS"
echo "    Validator: $VALIDATOR_ADDRESS"

# ---------------------------------------------------------------------------
# Fund via Friendbot (safe to call multiple times — already-funded accounts
# receive an HTTP 400 which we ignore)
# ---------------------------------------------------------------------------

echo "==> Funding test accounts via Friendbot..."

fund_account() {
  local addr="$1"
  curl -sf "https://friendbot.stellar.org?addr=$addr" > /dev/null 2>&1 \
    || echo "    (account $addr may already be funded — continuing)"
}

fund_account "$PLAYER_ADDRESS"
fund_account "$SCOUT_ADDRESS"
fund_account "$VALIDATOR_ADDRESS"

# ---------------------------------------------------------------------------
# Seed contract state
# ---------------------------------------------------------------------------

echo "==> Registering validator..."
stellar contract invoke \
  --id "$VERIFICATION_CONTRACT_ID" \
  --source "$DEPLOYER" \
  --network "$NETWORK" \
  -- register_validator \
  --wallet "$VALIDATOR_ADDRESS" \
  --credentials "UEFA B License — Test Validator" \
  || die "register_validator failed."

echo "==> Registering test player..."
stellar contract invoke \
  --id "$REGISTRATION_CONTRACT_ID" \
  --source player-test \
  --network "$NETWORK" \
  -- register_player \
  --wallet "$PLAYER_ADDRESS" \
  --vitals '{"age":19,"position":"Forward","region":"West Africa","nationality":"Ghana"}' \
  --ipfs_hashes '["QmTestHighlight1","QmTestPhoto1"]' \
  || die "register_player failed."

echo "==> Registering test scout..."
stellar contract invoke \
  --id "$REGISTRATION_CONTRACT_ID" \
  --source scout-test \
  --network "$NETWORK" \
  -- register_scout \
  --wallet "$SCOUT_ADDRESS" \
  --region "Europe" \
  || die "register_scout failed."

# ---------------------------------------------------------------------------
# Write .accounts file
# ---------------------------------------------------------------------------

{
  echo "PLAYER_ADDRESS=$PLAYER_ADDRESS"
  echo "SCOUT_ADDRESS=$SCOUT_ADDRESS"
  echo "VALIDATOR_ADDRESS=$VALIDATOR_ADDRESS"
} > testnet/.accounts

echo ""
echo "==> Seed complete."
echo "    Player address:    $PLAYER_ADDRESS"
echo "    Scout address:     $SCOUT_ADDRESS"
echo "    Validator address: $VALIDATOR_ADDRESS"
echo "    Saved to testnet/.accounts"

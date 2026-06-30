# ScoutChain Glossary

Domain-specific terms used throughout the contracts, documentation, and SDKs.
Each definition includes a role description and links to the relevant contract
functions in [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md).

---

## CID (Content Identifier)

A self-describing content hash produced by IPFS or Arweave. CIDs are stored
on-chain as strings inside player profiles and milestone evidence fields so
that off-chain video and photo assets can be retrieved and verified without
trusting a centralised server.

- IPFS CIDs start with `Qm…` (CIDv0) or `bafy…` (CIDv1).
- The `evidence_hash` parameter of `approve_milestone` and the `details_hash`
  parameter of `log_trial_offer` both accept CIDs.
- Relevant functions: `register_player`, `update_profile`, `approve_milestone`,
  `log_trial_offer` — see [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md).

---

## Contact Fee

A micro-payment in XLM (denominated in stroops) that a scout pays to unlock a
specific player's full contact details. Controlled by the `contact_fee_stroops`
field in [`FeeConfig`](#feeconfig).

- Relevant function: [`pay_to_contact`](CONTRACT_REFERENCE.md#pay_to_contactscout-address-player_id-u64---resultscoutaccesserror).

---

## FeeConfig

The primary configuration struct for the `scout_access` contract. Controls all
subscription and pay-to-contact fee rates. Set at `initialize` time and
adjustable via `update_fee_config`.

| Field | Type | Unit | Valid Range | Typical Value |
|---|---|---|---|---|
| `contact_fee_stroops` | `i128` | stroops (1 XLM = 10 000 000 stroops) | > 0 | `100000` (0.01 XLM) |
| `basic_sub_stroops` | `i128` | stroops | > 0 | `1000000` (0.1 XLM) |
| `pro_sub_stroops` | `i128` | stroops | > 0 | `3000000` (0.3 XLM) |
| `elite_sub_stroops` | `i128` | stroops | > 0 | `7000000` (0.7 XLM) |
| `sub_duration_secs` | `u64` | seconds | > 0 | `2592000` (30 days) |

All fields must be strictly greater than zero; `initialize` and
`update_fee_config` return `InvalidInput` otherwise.

- Relevant functions: `initialize`, `update_fee_config`, `get_fee_config` — see
  [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#scout_access).

---

## Milestone

A verified player achievement recorded on-chain by an authorised validator.
Each milestone stores a plain-text description, an IPFS/Arweave evidence CID,
the approving validator's address, and a ledger sequence number for
auditability.

Examples: "Scored 5 goals in Local Cup", "Top speed clocked at 32 km/h".

- Relevant functions: `approve_milestone`, `get_milestone`,
  `get_milestone_count` — see
  [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#verification).

---

## Player

A registered footballer with an on-chain identity. A player is identified by a
`player_id` (auto-incremented `u64`) and a Stellar wallet address. Players
start at `ProgressLevel` 0 (Unverified) and advance through up to four levels
as validators approve milestones and scouts log trial offers.

- Relevant functions: `register_player`, `get_player`, `filter_players` — see
  [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#registration).

---

## Progress Level

The four-tier trust ranking attached to every player profile. Levels advance
sequentially; skipping or reversing is blocked by the progress contract (admin
`reset_player_level` is the only exception).

| Level | Variant | Meaning |
|---|---|---|
| 0 | `Unverified` | Profile created, no verifications |
| 1 | `VerifiedIdentity` | Identity confirmed by a validator |
| 2 | `PerformanceMilestones` | Performance stats verified by a validator |
| 3 | `EliteTier` | Trial offer logged by an Elite-tier scout |

- Relevant functions: `advance_level`, `get_level`, `get_progress_history`,
  `reset_player_level` — see
  [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#progress).

---

## Scout

A talent-discovery professional registered on-chain with a Stellar wallet.
Scouts purchase a subscription tier (`Basic`, `Pro`, or `Elite`) to access the
filtered player pool, pay per-contact fees to unlock player details, and (Elite
only) log trial offers that advance a player to Level 3.

- Relevant functions: `register_scout`, `subscribe`, `pay_to_contact`,
  `log_trial_offer` — see
  [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#registration).

---

## Stroop

The smallest unit of XLM. 1 XLM = 10 000 000 stroops. All fee fields in
`FeeConfig` and all fee-related return values in the `scout_access` contract
are expressed in stroops (Rust type `i128`).

---

## Subscription Tier

The access level purchased by a scout. Determines which players are visible and
whether trial offers can be logged.

| Tier | Variant | Notes |
|---|---|---|
| Basic | `Basic` | Access to the filtered player pool |
| Pro | `Pro` | Higher trust signal; wider discovery |
| Elite | `Elite` | Required to call `log_trial_offer` |

Subscriptions expire after `sub_duration_secs` (default 30 days). Downgrades
while a subscription is active are blocked; upgrades charge the full new-tier
fee with no proration.

- Relevant functions: `subscribe`, `get_subscription` — see
  [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#scout_access).

---

## Trial Offer

An on-chain record that a scout has offered a player a trial or professional
opportunity. Logging a trial offer also advances the player to `EliteTier`
(Level 3) via a cross-contract call to the progress contract. Only scouts with
an active Elite subscription may log trial offers.

- Relevant functions: `log_trial_offer`, `get_trial_offer`, `get_trial_count`
  — see [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#scout_access).

---

## Validator

A trusted third party (local coach, academy director, or certified trainer)
registered by the platform admin. Only active validators may call
`approve_milestone`. A validator can be revoked by the admin; revoked validators
cannot approve further milestones until re-activated.

- Relevant functions: `register_validator`, `revoke_validator`,
  `get_validator_status`, `approve_milestone` — see
  [CONTRACT_REFERENCE.md](CONTRACT_REFERENCE.md#verification).

/// Integration test: scout_access ↔ progress cross-contract wiring.
///
/// Deploys ProgressContract and ScoutAccessContract in the same Env, wires
/// them together, and verifies that log_trial_offer actually advances the
/// player's level in the progress contract to EliteTier.
///
/// These tests would silently pass (incorrectly) with the old hand-rolled
/// mock client in scout_access — they require the real #[contractclient]
/// implementation that makes genuine cross-contract calls.
use scoutchain_progress::{ProgressContract, ProgressContractClient};
use scoutchain_scout_access::{
    FeeConfig, ScoutAccessContract, ScoutAccessContractClient, SubscriptionTier,
};
use scoutchain_shared_types::ProgressLevel;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, String,
};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn default_fees() -> FeeConfig {
    FeeConfig {
        contact_fee_stroops: 100_000,
        basic_sub_stroops: 1_000_000,
        pro_sub_stroops: 3_000_000,
        elite_sub_stroops: 7_000_000,
        sub_duration_secs: 30 * 24 * 60 * 60,
    }
}

struct Harness {
    env: Env,
    xlm: Address,
    progress: ProgressContractClient<'static>,
    scout_access: ScoutAccessContractClient<'static>,
}

fn setup() -> Harness {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let admin = Address::generate(&env);

    // Deploy and initialize the progress contract.
    let progress_id = env.register_contract(None, ProgressContract);
    let progress = ProgressContractClient::new(&env, &progress_id);
    progress.initialize(&admin);

    // Create the XLM token used by scout_access.
    let xlm = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    // Deploy and initialize the scout_access contract.
    let sa_id = env.register_contract(None, ScoutAccessContract);
    let scout_access = ScoutAccessContractClient::new(&env, &sa_id);
    scout_access.initialize(&admin, &xlm, &default_fees());

    // Wire scout_access → progress so log_trial_offer can call advance_level.
    scout_access.set_progress_contract(&progress_id);

    Harness {
        env,
        xlm,
        progress,
        scout_access,
    }
}

/// Advance a player by `levels` tiers directly in the progress contract.
/// Uses a fresh caller each test since progress.advance_level only requires
/// caller auth (no verification contract is configured here).
fn advance_player(h: &Harness, player_id: u64, levels: u32) {
    let caller = Address::generate(&h.env);
    for i in 1..=levels {
        h.progress.advance_level(&caller, &player_id, &i);
    }
}

/// Mint enough XLM for an Elite subscription and subscribe.
fn subscribe_elite(h: &Harness, scout: &Address) {
    StellarAssetClient::new(&h.env, &h.xlm).mint(scout, &10_000_000i128);
    h.scout_access.subscribe(scout, &SubscriptionTier::Elite);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Happy path: log_trial_offer advances a player from PerformanceMilestones
/// to EliteTier via a real cross-contract call to the progress contract.
#[test]
fn test_log_trial_offer_advances_player_to_elite_tier() {
    let h = setup();
    let player_id: u64 = 1;
    let scout = Address::generate(&h.env);

    // PerformanceMilestones is level 2 — two advance_level calls needed.
    advance_player(&h, player_id, 2);
    assert_eq!(
        h.progress.get_level(&player_id),
        ProgressLevel::PerformanceMilestones,
    );

    subscribe_elite(&h, &scout);

    let index = h.scout_access.log_trial_offer(
        &scout,
        &player_id,
        &String::from_str(&h.env, "QmTrialOfferHash1234567890"),
    );

    assert_eq!(index, 1);
    // The cross-contract call must have advanced the player to EliteTier.
    assert_eq!(h.progress.get_level(&player_id), ProgressLevel::EliteTier);
    assert_eq!(h.scout_access.get_trial_count(&player_id), 1);
}

/// Edge case: when the player is already at EliteTier, log_trial_offer must
/// silently ignore the AlreadyAtMaxLevel error from the progress contract
/// and still record the offer and return Ok.
#[test]
fn test_log_trial_offer_already_at_max_level_is_silent() {
    let h = setup();
    let player_id: u64 = 99;
    let scout = Address::generate(&h.env);

    // Advance all three tiers — player starts at EliteTier.
    advance_player(&h, player_id, 3);
    assert_eq!(h.progress.get_level(&player_id), ProgressLevel::EliteTier);

    subscribe_elite(&h, &scout);

    // First offer: advance_level returns AlreadyAtMaxLevel; must be ignored.
    let index = h.scout_access.log_trial_offer(
        &scout,
        &player_id,
        &String::from_str(&h.env, "QmTrialOfferHash1234567890"),
    );
    assert_eq!(index, 1);
    assert_eq!(h.progress.get_level(&player_id), ProgressLevel::EliteTier);

    // Second offer: same behavior — offer recorded, level unchanged.
    let index2 = h.scout_access.log_trial_offer(
        &scout,
        &player_id,
        &String::from_str(&h.env, "QmTrialOfferHash0987654321"),
    );
    assert_eq!(index2, 2);
    assert_eq!(h.scout_access.get_trial_count(&player_id), 2);
    assert_eq!(h.progress.get_level(&player_id), ProgressLevel::EliteTier);
}

/// Issue #425 — TrialCounter increments correctly across multiple log_trial_offer calls.
///
/// Two different Elite scouts each log one trial offer for the same player.
/// Using distinct scouts avoids the 24-hour per-(scout, player) cooldown so
/// both offers can be recorded in the same test environment without advancing
/// the ledger timestamp.
///
/// Acceptance criteria:
///   - First log_trial_offer returns trial index 1
///   - Second log_trial_offer (different scout) returns trial index 2
///   - get_trial_offer(player_id, 1) returns the first offer with correct scout
///   - get_trial_offer(player_id, 2) returns the second offer with correct scout
///   - get_trial_count returns 2 after both calls
#[test]
fn test_trial_counter_increments_across_two_scouts() {
    let h = setup();
    let player_id: u64 = 42;

    // Advance the player to PerformanceMilestones (level 2) so the first
    // log_trial_offer can advance them to EliteTier via the progress contract.
    advance_player(&h, player_id, 2);
    assert_eq!(
        h.progress.get_level(&player_id),
        ProgressLevel::PerformanceMilestones,
    );

    // Scout A — first trial offer
    let scout_a = Address::generate(&h.env);
    subscribe_elite(&h, &scout_a);

    let hash_a = String::from_str(&h.env, "QmTrialOfferScoutAHash1234567");
    let index_a = h.scout_access.log_trial_offer(&scout_a, &player_id, &hash_a);

    // First call must return index 1
    assert_eq!(index_a, 1, "first trial offer must be assigned index 1");
    // Player advanced to EliteTier by the cross-contract call
    assert_eq!(h.progress.get_level(&player_id), ProgressLevel::EliteTier);
    // Counter is now 1
    assert_eq!(h.scout_access.get_trial_count(&player_id), 1);

    // Scout B — second trial offer for the same player.
    // A different scout is used to avoid the 24-hour per-(scout, player) cooldown.
    let scout_b = Address::generate(&h.env);
    subscribe_elite(&h, &scout_b);

    let hash_b = String::from_str(&h.env, "QmTrialOfferScoutBHash9876543");
    let index_b = h.scout_access.log_trial_offer(&scout_b, &player_id, &hash_b);

    // Second call must return index 2
    assert_eq!(index_b, 2, "second trial offer must be assigned index 2");
    // Counter is now 2
    assert_eq!(h.scout_access.get_trial_count(&player_id), 2);

    // Retrieve the first offer and verify its fields
    let offer_1 = h.scout_access.get_trial_offer(&player_id, &1u32);
    assert_eq!(offer_1.player_id, player_id);
    assert_eq!(offer_1.scout, scout_a);
    assert_eq!(offer_1.details_hash, hash_a);

    // Retrieve the second offer and verify its fields
    let offer_2 = h.scout_access.get_trial_offer(&player_id, &2u32);
    assert_eq!(offer_2.player_id, player_id);
    assert_eq!(offer_2.scout, scout_b);
    assert_eq!(offer_2.details_hash, hash_b);
}

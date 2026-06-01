mod errors;
mod events;
mod types;

use errors::ScoutAccessError;
use types::{ContractHealth, DataKey, FeeConfig, Subscription, SubscriptionTier, TrialOffer};

use soroban_sdk::{contract, contractimpl, token, Address, Env, String};

// ~30 days at 5 s/ledger; extend when TTL drops below half that.
const TRIAL_TTL_THRESHOLD: u32 = 259_200;
const TRIAL_TTL_EXTEND_TO: u32 = 518_400;
const PERSISTENT_TTL_MIN: u32 = 2000;
const PERSISTENT_TTL_MAX: u32 = 10000;

// Persistent entries: extend when TTL < 1000 ledgers; extend to 2000.
const PERSISTENT_TTL_MIN: u32 = 1000;
const PERSISTENT_TTL_MAX: u32 = 2000;

#[cfg(target_family = "wasm")]
mod progress_contract {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/scoutchain_progress.wasm"
    );
}

#[cfg(not(target_family = "wasm"))]
mod progress_contract {
    use soroban_sdk::{contracterror, Address, Env, Val, Error as SorobanError};

    #[contracterror]
    #[derive(Copy, Clone, Debug, PartialEq)]
    #[repr(u32)]
    pub enum Error {
        AlreadyAtMaxLevel = 6,
    }

    pub struct Client<'a> {
        pub env: Env,
        pub contract_id: Address,
        #[allow(dead_code)]
        phantom: core::marker::PhantomData<&'a ()>,
    }

    impl<'a> Client<'a> {
        pub fn new(env: &Env, contract_id: &Address) -> Self {
            Self {
                env: env.clone(),
                contract_id: contract_id.clone(),
                phantom: core::marker::PhantomData,
            }
        }

        pub fn try_advance_level(
            &self,
            _caller: &Address,
            _player_id: &u64,
            _milestone_ref: &u32,
        ) -> Result<Result<Val, Val>, Result<Error, SorobanError>> {
            // Mock implementation for host/tests
            Ok(Ok(0u32.into()))
        }
    }
}

#[contract]
pub struct ScoutAccessContract;

mod progress_contract {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/scoutchain_progress.wasm"
    );
}

#[contractimpl]
impl ScoutAccessContract {
    // -------------------------------------------------------------------------
    // Admin
    // -------------------------------------------------------------------------

    pub fn initialize(
        env: Env,
        admin: Address,
        xlm_token: Address,
        fee_config: FeeConfig,
    ) -> Result<(), ScoutAccessError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(ScoutAccessError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::XlmToken, &xlm_token);
        env.storage().instance().set(&DataKey::FeeConfig, &fee_config);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::AccumulatedFees, &0i128);
        events::contract_initialized(&env, &admin);
        Ok(())
    }

    pub fn update_fee_config(env: Env, fee_config: FeeConfig) -> Result<(), ScoutAccessError> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::FeeConfig, &fee_config);
        Ok(())
    }

    pub fn withdraw_fees(env: Env, to: Address) -> Result<i128, ScoutAccessError> {
        Self::require_admin(&env)?;
        let fees: i128 = env
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0i128);
        if fees == 0 {
            return Err(ScoutAccessError::InsufficientFee);
        }
        let xlm = Self::xlm_token(&env);
        let contract_addr = env.current_contract_address();
        token::Client::new(&env, &xlm).transfer(&contract_addr, &to, &fees);
        env.storage().instance().set(&DataKey::AccumulatedFees, &0i128);
        events::fees_withdrawn(&env, &to, fees);
        Ok(fees)
    }

    pub fn pause_contract(env: Env) -> Result<(), ScoutAccessError> {
        Self::require_admin(&env)?;
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ScoutAccessError::NotInitialized)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        events::contract_paused(&env, &admin);
        Ok(())
    }

    pub fn unpause_contract(env: Env) -> Result<(), ScoutAccessError> {
        Self::require_admin(&env)?;
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ScoutAccessError::NotInitialized)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        events::contract_unpaused(&env, &admin);
        Ok(())
    }

    /// Register the progress contract address so log_trial_offer can
    /// atomically advance the player to Level 3 (admin only).
    pub fn set_progress_contract(env: Env, addr: Address) -> Result<(), ScoutAccessError> {
        Self::require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::ProgressContract, &addr);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Scout subscription
    // -------------------------------------------------------------------------

    /// Purchase a scout subscription. Scout must pre-approve the XLM transfer.
    pub fn subscribe(
        env: Env,
        scout: Address,
        tier: SubscriptionTier,
    ) -> Result<(), ScoutAccessError> {
        Self::require_not_paused(&env)?;
        Self::require_initialized(&env)?;
        scout.require_auth();

        let config = Self::fee_config(&env);
        let fee = match &tier {
            SubscriptionTier::Basic => config.basic_sub_stroops,
            SubscriptionTier::Pro => config.pro_sub_stroops,
            SubscriptionTier::Elite => config.elite_sub_stroops,
        };

        let xlm = Self::xlm_token(&env);
        let contract_addr = env.current_contract_address();
        token::Client::new(&env, &xlm).transfer(&scout, &contract_addr, &fee);
        Self::accumulate_fee(&env, fee)?;

        let now = env.ledger().timestamp();
        let sub = Subscription {
            scout: scout.clone(),
            tier: tier.clone(),
            expires_at: now
                .checked_add(config.sub_duration_secs)
                .ok_or(ScoutAccessError::Overflow)?,
            subscribed_at: now,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Subscription(scout.clone()), &sub);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Subscription(scout.clone()), PERSISTENT_TTL_MIN, PERSISTENT_TTL_MAX);

        events::scout_subscribed(&env, &scout, &tier);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Pay-to-contact
    // -------------------------------------------------------------------------

    /// Pay a micro-fee to unlock a player's contact details.
    /// Scout must have an active subscription.
    pub fn pay_to_contact(
        env: Env,
        scout: Address,
        player_id: u64,
    ) -> Result<(), ScoutAccessError> {
        Self::require_not_paused(&env)?;
        scout.require_auth();
        Self::require_active_subscription(&env, &scout)?;

        let contact_key = DataKey::ContactRecord(player_id, scout.clone());
        if env.storage().persistent().has(&contact_key) {
            return Err(ScoutAccessError::AlreadyContacted);
        }

        let config = Self::fee_config(&env);
        let xlm = Self::xlm_token(&env);
        let contract_addr = env.current_contract_address();
        token::Client::new(&env, &xlm).transfer(
            &scout,
            &contract_addr,
            &config.contact_fee_stroops,
        );
        Self::accumulate_fee(&env, config.contact_fee_stroops)?;

        env.storage().persistent().set(&contact_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&contact_key, PERSISTENT_TTL_MIN, PERSISTENT_TTL_MAX);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Subscription(scout.clone()), PERSISTENT_TTL_MIN, PERSISTENT_TTL_MAX);
        events::player_contacted(&env, player_id, &scout);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Trial offer
    // -------------------------------------------------------------------------

    /// Log a trial offer on-chain. Scout must have an Elite subscription.
    /// The backend should call progress.advance_level after this succeeds.
    pub fn log_trial_offer(
        env: Env,
        scout: Address,
        player_id: u64,
        details_hash: String,
    ) -> Result<u32, ScoutAccessError> {
        Self::require_not_paused(&env)?;
        scout.require_auth();

        let sub = Self::require_active_subscription(&env, &scout)?;
        if sub.tier != SubscriptionTier::Elite {
            return Err(ScoutAccessError::Unauthorized);
        }
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Subscription(scout.clone()), PERSISTENT_TTL_MIN, PERSISTENT_TTL_MAX);

        let counter_key = DataKey::TrialCounter(player_id);
        let index: u32 = env
            .storage()
            .persistent()
            .get(&counter_key)
            .unwrap_or(0u32);
        let next_index = index.checked_add(1).ok_or(ScoutAccessError::Overflow)?;

        let offer = TrialOffer {
            player_id,
            scout: scout.clone(),
            details_hash,
            logged_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::TrialOffer(player_id, next_index), &offer);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::TrialOffer(player_id, next_index), TRIAL_TTL_THRESHOLD, TRIAL_TTL_EXTEND_TO);
        env.storage().persistent().set(&counter_key, &next_index);
        env.storage()
            .persistent()
            .extend_ttl(&counter_key, TRIAL_TTL_THRESHOLD, TRIAL_TTL_EXTEND_TO);

        events::trial_offer_logged(&env, player_id, &scout);

        // Atomically advance the player to Level 3 if the progress contract
        // is configured. AlreadyAtMaxLevel is silently ignored; any other
        // failure is a hard error.
        if let Some(progress_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ProgressContract)
        {
            let progress_client = progress_contract::Client::new(&env, &progress_addr);
            match progress_client.try_advance_level(&scout, &player_id, &next_index) {
                Ok(_) => {}
                Err(Ok(progress_contract::Error::AlreadyAtMaxLevel)) => {}
                Err(_) => return Err(ScoutAccessError::ProgressCallFailed),
            }
        }

        Ok(next_index)
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    pub fn get_subscription(
        env: Env,
        scout: Address,
    ) -> Result<Subscription, ScoutAccessError> {
        let sub = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(scout.clone()))
            .ok_or(ScoutAccessError::ScoutNotSubscribed)?;
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Subscription(scout), PERSISTENT_TTL_MIN, PERSISTENT_TTL_MAX);
        Ok(sub)
    }

    pub fn get_fee_config(env: Env) -> FeeConfig {
        Self::fee_config(&env)
    }

    pub fn get_accumulated_fees(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0i128)
    }

    pub fn has_contacted(env: Env, scout: Address, player_id: u64) -> bool {
        let key = DataKey::ContactRecord(player_id, scout);
        let exists = env.storage().persistent().has(&key);
        if exists {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_TTL_MIN, PERSISTENT_TTL_MAX);
        }
        exists
    }

    pub fn get_trial_offer(
        env: Env,
        player_id: u64,
        index: u32,
    ) -> Result<TrialOffer, ScoutAccessError> {
        let offer = env
            .storage()
            .persistent()
            .get(&DataKey::TrialOffer(player_id, index))
            .ok_or(ScoutAccessError::TrialOfferNotFound)?;
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::TrialOffer(player_id, index), TRIAL_TTL_THRESHOLD, TRIAL_TTL_EXTEND_TO);
        Ok(offer)
    }

    pub fn get_trial_count(env: Env, player_id: u64) -> u32 {
        let count = env
            .storage()
            .persistent()
            .get(&DataKey::TrialCounter(player_id))
            .unwrap_or(0u32);
        if count > 0 {
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::TrialCounter(player_id), TRIAL_TTL_THRESHOLD, TRIAL_TTL_EXTEND_TO);
        }
        count
    }

    pub fn health(env: Env) -> ContractHealth {
        let initialized = env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Initialized)
            .unwrap_or(false);
        let paused = env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false);
        ContractHealth { initialized, paused }
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    fn require_admin(env: &Env) -> Result<(), ScoutAccessError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ScoutAccessError::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }

    fn require_initialized(env: &Env) -> Result<(), ScoutAccessError> {
        if !env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            return Err(ScoutAccessError::NotInitialized);
        }
        Ok(())
    }

    fn require_not_paused(env: &Env) -> Result<(), ScoutAccessError> {
        if env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(ScoutAccessError::ContractPaused);
        }
        Ok(())
    }

    fn require_active_subscription(
        env: &Env,
        scout: &Address,
    ) -> Result<Subscription, ScoutAccessError> {
        let sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(scout.clone()))
            .ok_or(ScoutAccessError::ScoutNotSubscribed)?;
        if env.ledger().timestamp() > sub.expires_at {
            return Err(ScoutAccessError::SubscriptionExpired);
        }
        Ok(sub)
    }

    fn fee_config(env: &Env) -> FeeConfig {
        env.storage()
            .instance()
            .get(&DataKey::FeeConfig)
            .expect("fee config not set")
    }

    fn xlm_token(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::XlmToken)
            .expect("xlm token not set")
    }

    fn accumulate_fee(env: &Env, amount: i128) -> Result<(), ScoutAccessError> {
        let current: i128 = env
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0i128);
        let new_total = current
            .checked_add(amount)
            .ok_or(ScoutAccessError::Overflow)?;
        env.storage()
            .instance()
            .set(&DataKey::AccumulatedFees, &new_total);
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger, MockAuth, MockAuthInvoke},
        token::{Client as TokenClient, StellarAssetClient},
        Env, IntoVal, String, Symbol,
    };

    /// Deploy a mock SAC token, mint `amount` to `to`, return the token contract address.
    fn create_token(env: &Env, admin: &Address) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        token_id.address()
    }

    fn mint_token(env: &Env, token: &Address, admin: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(env, token).mint(to, &amount);
    }

    fn default_fees() -> FeeConfig {
        FeeConfig {
            contact_fee_stroops: 100_000,
            basic_sub_stroops: 1_000_000,
            pro_sub_stroops: 3_000_000,
            elite_sub_stroops: 7_000_000,
            sub_duration_secs: 30 * 24 * 60 * 60,
        }
    }

    fn setup() -> (Env, Address, Address, Address, ScoutAccessContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let xlm = create_token(&env, &admin);
        let contract_id = env.register_contract(None, ScoutAccessContract);
        let client = ScoutAccessContractClient::new(&env, &contract_id);
        client.initialize(&admin, &xlm, &default_fees());
        (env, admin, xlm, contract_id, client)
    }

    #[test]
    fn test_initialize_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let xlm = create_token(&env, &admin);
        let contract_id = env.register_contract(None, ScoutAccessContract);
        let client = ScoutAccessContractClient::new(&env, &contract_id);

        client.initialize(&admin, &xlm, &default_fees());

        let events = env.events().all();
        let last_event = events.get(events.len() - 1);
        
        assert_eq!(
            last_event,
            (
                contract_id.clone(),
                (Symbol::new(&env, "contract_initialized"), admin.clone()).into_val(&env),
                admin.clone().into_val(&env)
            )
        );

        // Duplicate initialize should fail and NOT emit event
        let res = client.try_initialize(&admin, &xlm, &default_fees());
        assert_eq!(res, Err(Ok(ScoutAccessError::AlreadyInitialized)));
        
        let events_after = env.events().all();
        assert_eq!(events.len(), events_after.len());
    }

    #[test]
    fn test_initialize_and_health() {
        let (_, _, _, _, client) = setup();
        assert!(client.health());
    }

    #[test]
    fn test_subscribe_basic() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        // Fund scout with enough XLM
        mint_token(&env, &xlm, &admin, &scout, 10_000_000);

        client.subscribe(&scout, &SubscriptionTier::Basic);

        let sub = client.get_subscription(&scout);
        assert_eq!(sub.tier, SubscriptionTier::Basic);
        assert!(sub.expires_at > sub.subscribed_at);
        assert_eq!(client.get_accumulated_fees(), 1_000_000);
    }

    #[test]
    fn test_subscribe_elite_and_pay_to_contact() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        client.subscribe(&scout, &SubscriptionTier::Elite);
        client.pay_to_contact(&scout, &1u64);

        assert!(client.has_contacted(&scout, &1u64));
        // elite fee + contact fee
        assert_eq!(client.get_accumulated_fees(), 7_000_000 + 100_000);
    }

    #[test]
    #[should_panic]
    fn test_duplicate_contact_fails() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        client.subscribe(&scout, &SubscriptionTier::Elite);
        client.pay_to_contact(&scout, &1u64);
        // second contact with same player should panic
        client.pay_to_contact(&scout, &1u64);
    }

    #[test]
    fn test_log_trial_offer_elite() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        client.subscribe(&scout, &SubscriptionTier::Elite);
        let idx = client.log_trial_offer(
            &scout,
            &1u64,
            &String::from_str(&env, "QmTrialDetails"),
        );
        assert_eq!(idx, 1);
        assert_eq!(client.get_trial_count(&1u64), 1);

        let offer = client.get_trial_offer(&1u64, &1u32);
        assert_eq!(offer.player_id, 1);
        assert_eq!(offer.scout, scout);
    }

    #[test]
    fn test_trial_offer_ttl_extended_after_ledger_advance() {
        let (env, admin, xlm, contract_id, client) = setup();

        // Start at a known ledger sequence so TTL arithmetic is predictable.
        env.ledger().with_mut(|l| {
            l.sequence_number = 100_000;
            l.min_persistent_entry_ttl = 500;
            l.max_entry_ttl = 600_000;
        });

        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);
        client.subscribe(&scout, &SubscriptionTier::Elite);

        // log_trial_offer stores the entry and immediately calls extend_ttl
        // with TRIAL_TTL_EXTEND_TO (518_400 ledgers).
        client.log_trial_offer(&scout, &1u64, &String::from_str(&env, "QmTTLTest"));

        // Advance the ledger well past the default min_persistent_entry_ttl (500)
        // but within TRIAL_TTL_EXTEND_TO (518_400). The entry must still be live.
        env.ledger().with_mut(|l| {
            l.sequence_number = 100_000 + 1_000;
        });

        // Both the offer and the counter must still be accessible.
        let offer = client.get_trial_offer(&1u64, &1u32);
        assert_eq!(offer.player_id, 1);
        assert_eq!(client.get_trial_count(&1u64), 1);
    }

    #[test]
    #[should_panic]
    fn test_trial_offer_requires_elite() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        // Pro tier — not allowed to log trial offers
        client.subscribe(&scout, &SubscriptionTier::Pro);
        client.log_trial_offer(&scout, &1u64, &String::from_str(&env, "QmDetails"));
    }

    #[test]
    #[should_panic]
    fn test_trial_offer_rejected_for_basic_tier() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        // Basic tier — not allowed to log trial offers
        client.subscribe(&scout, &SubscriptionTier::Basic);
        client.log_trial_offer(&scout, &1u64, &String::from_str(&env, "QmDetails"));
    }

    #[test]
    #[should_panic]
    fn test_contact_without_subscription_fails() {
        let (env, _, _, _, client) = setup();
        let scout = Address::generate(&env);
        client.pay_to_contact(&scout, &1u64);
    }

    #[test]
    #[should_panic]
    fn test_subscription_expiry() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        client.subscribe(&scout, &SubscriptionTier::Pro);

        // Fast-forward past expiry (31 days)
        env.ledger().with_mut(|l| {
            l.timestamp += 31 * 24 * 60 * 60;
        });

        // Should panic with SubscriptionExpired
        client.pay_to_contact(&scout, &1u64);
    }

    #[test]
    fn test_pause_unpause_events() {
        let (env, admin, _, _, client) = setup();

        client.pause_contract();
        let events = env.events().all();
        assert_eq!(
            events,
            soroban_sdk::vec![
                &env,
                (
                    client.address.clone(),
                    (Symbol::new(&env, "contract_paused"),).into_val(&env),
                    admin.clone().into_val(&env)
                )
            ]
        );

        client.unpause_contract();
        let events = env.events().all();
        assert_eq!(
            events,
            soroban_sdk::vec![
                &env,
                (
                    client.address.clone(),
                    (Symbol::new(&env, "contract_unpaused"),).into_val(&env),
                    admin.clone().into_val(&env)
                )
            ]
        );
    }

    #[test]
    fn test_full_scout_workflow() {
        let (env, admin, xlm, _contract_id, client) = setup();
        let scout = Address::generate(&env);
        let player_id = 1u64;
        let details_hash = String::from_str(&env, "QmTrialDetails");

        // 1. Initialize with default fees (already done in setup)
        let fees = default_fees();

        // 2. Mint XLM to a scout
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        // 3. Subscribe the scout with Elite tier
        client.subscribe(&scout, &SubscriptionTier::Elite);

        // 4. Calls pay_to_contact(scout, player_id = 1)
        client.pay_to_contact(&scout, &player_id);

        // 5. Calls log_trial_offer(scout, player_id = 1, "QmTrialDetails")
        client.log_trial_offer(&scout, &player_id, &details_hash);

        // Assertions (8 total)
        // 1. Asserts has_contacted(scout, 1) == true.
        assert!(client.has_contacted(&scout, &player_id));

        // 2. Asserts get_trial_count(1) == 1.
        assert_eq!(client.get_trial_count(&player_id), 1);

        // 3. Asserts get_accumulated_fees() == elite_sub_fee + contact_fee.
        let expected_fees = fees.elite_sub_stroops + fees.contact_fee_stroops;
        assert_eq!(client.get_accumulated_fees(), expected_fees);

        // 4. Asserts the subscription tier is Elite.
        let sub = client.get_subscription(&scout);
        assert_eq!(sub.tier, SubscriptionTier::Elite);

        // 5. Asserts the trial offer was recorded by the correct scout.
        let offer = client.get_trial_offer(&player_id, &1u32);
        assert_eq!(offer.scout, scout);

        // 6. Asserts the trial offer points to the correct player.
        assert_eq!(offer.player_id, player_id);

        // 7. Asserts the trial offer contains the correct details hash.
        assert_eq!(offer.details_hash, details_hash);

        // 8. Asserts the subscription duration is active.
        assert!(sub.expires_at > sub.subscribed_at);
    }

    #[test]
    fn test_withdraw_fees_success() {
        let (env, admin, xlm, _contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 10_000_000);

        // Accumulate some fees
        client.subscribe(&scout, &SubscriptionTier::Basic);
        assert_eq!(client.get_accumulated_fees(), 1_000_000);

        let recipient = Address::generate(&env);
        let withdrawn = client.withdraw_fees(&recipient);
        assert_eq!(withdrawn, 1_000_000);
        assert_eq!(client.get_accumulated_fees(), 0);

        let token_client = TokenClient::new(&env, &xlm);
        assert_eq!(token_client.balance(&recipient), 1_000_000);
    }

    #[test]
    fn test_withdraw_fees_insufficient() {
        let (env, _admin, _xlm, _contract_id, client) = setup();
        let recipient = Address::generate(&env);
        // Should return InsufficientFee since fees are 0
        let result = client.try_withdraw_fees(&recipient);
        assert_eq!(result, Err(Ok(ScoutAccessError::InsufficientFee)));
    }

    #[test]
    fn test_fee_accumulation_overflow() {
        let (env, admin, xlm, contract_id, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 100_000_000);

        // Manually set AccumulatedFees to near MAX
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&DataKey::AccumulatedFees, &(i128::MAX - 1));
        });

        // Subscribing should trigger overflow in accumulate_fee
        // basic_sub_stroops is 1,000_000
        let result = client.try_subscribe(&scout, &SubscriptionTier::Basic);
        assert_eq!(result, Err(Ok(ScoutAccessError::Overflow)));
    }
}

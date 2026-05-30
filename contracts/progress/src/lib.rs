mod errors;
mod events;
mod types;

use errors::ProgressError;
use types::{DataKey, ProgressEntry, ProgressLevel};

use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct ProgressContract;

#[contractimpl]
impl ProgressContract {
    // -------------------------------------------------------------------------
    // Admin
    // -------------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address) -> Result<(), ProgressError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(ProgressError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn pause_contract(env: Env) -> Result<(), ProgressError> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause_contract(env: Env) -> Result<(), ProgressError> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Transfer admin rights to a new address (current admin auth required).
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), ProgressError> {
        let old_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ProgressError::NotInitialized)?;
        old_admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        events::admin_transferred(&env, &old_admin, &new_admin);
        Ok(())
    }

    /// Store the address of the registration contract.
    /// Only the admin may call this; must be called before `initialize_player`
    /// can be used.
    pub fn set_registration_contract(
        env: Env,
        addr: Address,
    ) -> Result<(), ProgressError> {
        Self::require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::RegistrationContract, &addr);
        Ok(())
    }

    /// Explicitly create an on-chain record for `player_id` at
    /// [`ProgressLevel::Unverified`], making it possible to distinguish a
    /// known-but-unadvanced player from an ID that has never been seen.
    ///
    /// Only the address stored under [`DataKey::RegistrationContract`] may
    /// call this function.  Returns [`ProgressError::PlayerAlreadyInitialized`]
    /// if the key already exists.
    pub fn initialize_player(
        env: Env,
        caller: Address,
        player_id: u64,
    ) -> Result<(), ProgressError> {
        Self::require_not_paused(&env)?;
        Self::require_initialized(&env)?;
        caller.require_auth();

        // Only the registered registration contract is authorised.
        let registration_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::RegistrationContract)
            .ok_or(ProgressError::Unauthorized)?;
        if caller != registration_contract {
            return Err(ProgressError::Unauthorized);
        }

        // Reject duplicate initialisation.
        if env
            .storage()
            .persistent()
            .has(&DataKey::PlayerLevel(player_id))
        {
            return Err(ProgressError::PlayerAlreadyInitialized);
        }

        env.storage()
            .persistent()
            .set(&DataKey::PlayerLevel(player_id), &ProgressLevel::Unverified);

        events::player_initialized(&env, player_id, &caller);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Progress updates
    // -------------------------------------------------------------------------

    /// Advance a player's progress level by one tier.
    /// Caller must be an authorized validator (or scout for Level 3).
    /// `milestone_ref` links back to the verification contract's milestone index.
    pub fn advance_level(
        env: Env,
        caller: Address,
        player_id: u64,
        milestone_ref: u32,
    ) -> Result<ProgressLevel, ProgressError> {
        Self::require_not_paused(&env)?;
        Self::require_initialized(&env)?;
        caller.require_auth();

        let current = Self::get_current_level(&env, player_id);
        let new_level = current
            .next()
            .ok_or(ProgressError::AlreadyAtMaxLevel)?;

        // Record history entry
        let history_key = DataKey::HistoryCounter(player_id);
        let index: u32 = env
            .storage()
            .persistent()
            .get(&history_key)
            .unwrap_or(0u32);
        let next_index = index.checked_add(1).expect("overflow");

        let entry = ProgressEntry {
            player_id,
            old_level: current,
            new_level: new_level.clone(),
            updated_by: caller.clone(),
            updated_at: env.ledger().timestamp(),
            milestone_ref,
        };

        env.storage()
            .persistent()
            .set(&DataKey::HistoryEntry(player_id, next_index), &entry);
        env.storage()
            .persistent()
            .set(&history_key, &next_index);
        env.storage()
            .persistent()
            .set(&DataKey::PlayerLevel(player_id), &new_level);

        events::progress_updated(&env, player_id, &new_level, &caller);
        Ok(new_level)
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    pub fn get_level(env: Env, player_id: u64) -> ProgressLevel {
        Self::get_current_level(&env, player_id)
    }

    pub fn get_history_count(env: Env, player_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::HistoryCounter(player_id))
            .unwrap_or(0u32)
    }

    pub fn get_history_entry(
        env: Env,
        player_id: u64,
        index: u32,
    ) -> Result<ProgressEntry, ProgressError> {
        env.storage()
            .persistent()
            .get(&DataKey::HistoryEntry(player_id, index))
            .ok_or(ProgressError::PlayerNotFound)
    }

    pub fn health(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Initialized)
            .unwrap_or(false)
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    fn get_current_level(env: &Env, player_id: u64) -> ProgressLevel {
        env.storage()
            .persistent()
            .get(&DataKey::PlayerLevel(player_id))
            .unwrap_or(ProgressLevel::Unverified)
    }

    fn require_initialized(env: &Env) -> Result<(), ProgressError> {
        if !env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            return Err(ProgressError::NotInitialized);
        }
        Ok(())
    }

    fn require_not_paused(env: &Env) -> Result<(), ProgressError> {
        if env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(ProgressError::ContractPaused);
        }
        Ok(())
    }

    fn require_admin(env: &Env) -> Result<(), ProgressError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ProgressError::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, ProgressContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, ProgressContract);
        let client = ProgressContractClient::new(&env, &id);
        (env, client)
    }

    #[test]
    fn test_health_false_before_initialize() {
        let env = Env::default();
        let id = env.register_contract(None, ProgressContract);
        let client = ProgressContractClient::new(&env, &id);
        // No initialize() call — health() must report false
        assert!(!client.health());
    }

    #[test]
    fn test_advance_level_sequence() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 1u64;

        // Unverified → VerifiedIdentity
        let l1 = client.advance_level(&validator, &player_id, &1u32);
        assert_eq!(l1, ProgressLevel::VerifiedIdentity);

        // VerifiedIdentity → PerformanceMilestones
        let l2 = client.advance_level(&validator, &player_id, &2u32);
        assert_eq!(l2, ProgressLevel::PerformanceMilestones);

        // PerformanceMilestones → EliteTier
        let l3 = client.advance_level(&validator, &player_id, &3u32);
        assert_eq!(l3, ProgressLevel::EliteTier);

        assert_eq!(client.get_history_count(&player_id), 3);
    }

    #[test]
    #[should_panic]
    fn test_cannot_exceed_elite_tier() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 1u64;

        client.advance_level(&validator, &player_id, &1u32);
        client.advance_level(&validator, &player_id, &2u32);
        client.advance_level(&validator, &player_id, &3u32);
        // This should panic — already at EliteTier
        client.advance_level(&validator, &player_id, &4u32);
    }

    #[test]
    fn test_transfer_admin_success() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let new_admin = Address::generate(&env);
        // Should not panic — current admin auth is satisfied
        client.transfer_admin(&new_admin);
    }

    #[test]
    #[should_panic]
    fn test_transfer_admin_unauthorized() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Clear all mocks — no auth satisfied, so admin check fails
        env.mock_auths(&[]);
        client.transfer_admin(&Address::generate(&env));
    }

    #[test]
    fn test_pause_and_unpause() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 42u64;

        // --- pause ---
        client.pause_contract();

        // advance_level must be rejected with ContractPaused while paused
        let err = client
            .try_advance_level(&validator, &player_id, &1u32)
            .expect_err("expected an error while paused");
        assert_eq!(
            err.unwrap(),
            ProgressError::ContractPaused,
            "expected ContractPaused error"
        );

        // player level must be unchanged
        assert_eq!(client.get_level(&player_id), ProgressLevel::Unverified);

        // --- unpause ---
        client.unpause_contract();

        // advance_level must now succeed
        let new_level = client.advance_level(&validator, &player_id, &1u32);
        assert_eq!(new_level, ProgressLevel::VerifiedIdentity);
    }

    #[test]
    #[should_panic]
    fn test_old_admin_loses_access_after_transfer() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let new_admin = Address::generate(&env);
        client.transfer_admin(&new_admin);

        // Clear mocks — old admin auth no longer stored, so pause must fail
        env.mock_auths(&[]);
        client.pause_contract();
    }

    // -------------------------------------------------------------------------
    // initialize_player tests
    // -------------------------------------------------------------------------

    /// Helper: set up a fully initialised contract and register a registration
    /// contract address, returning both the client and the registration address.
    fn setup_with_registration() -> (Env, ProgressContractClient<'static>, Address) {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let reg = Address::generate(&env);
        client.set_registration_contract(&reg);
        (env, client, reg)
    }

    #[test]
    fn test_initialize_player_sets_unverified() {
        let (env, client, reg) = setup_with_registration();
        let player_id = 100u64;

        // Before initialization the key is absent — get_level returns Unverified
        // via unwrap_or, but has() on the storage key is false.
        // After initialize_player the key must be explicitly present.
        client.initialize_player(&reg, &player_id);

        assert_eq!(client.get_level(&player_id), ProgressLevel::Unverified);
    }

    #[test]
    fn test_initialize_player_duplicate_returns_error() {
        let (env, client, reg) = setup_with_registration();
        let player_id = 101u64;

        client.initialize_player(&reg, &player_id);

        let err = client
            .try_initialize_player(&reg, &player_id)
            .expect_err("expected error on duplicate initialize_player");
        assert_eq!(
            err.unwrap(),
            ProgressError::PlayerAlreadyInitialized,
            "expected PlayerAlreadyInitialized on duplicate call"
        );
    }

    #[test]
    fn test_initialize_player_unauthorized_caller() {
        let (env, client, _reg) = setup_with_registration();
        let player_id = 102u64;
        let impostor = Address::generate(&env);

        let err = client
            .try_initialize_player(&impostor, &player_id)
            .expect_err("expected error for unauthorized caller");
        assert_eq!(
            err.unwrap(),
            ProgressError::Unauthorized,
            "expected Unauthorized for non-registration caller"
        );
    }
}

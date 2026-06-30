# Test: update_fee_config Event Emission

## Status: IMPLEMENTED ✓

### What Was Done

1. **Added Event Function** (`contracts/scout_access/src/events.rs`):
   - Created `fee_config_updated()` event function
   - Event emits both old and new `FeeConfig` structs
   - Topic symbol: `"fee_config_updated"`

2. **Updated Function** (`contracts/scout_access/src/lib.rs`):
   - Modified `update_fee_config()` to capture old config before update
   - Added event emission after storage update: `events::fee_config_updated(&env, &old_config, &fee_config)`
   - Event is emitted **after** storage write (following #455 pattern)

3. **Test Implementation** (`contracts/scout_access/src/lib.rs`):
   - Updated existing test: `test_fee_config_updated_event_contains_old_and_new_config()`
   - Test verifies:
     - Storage is updated correctly
     - Exactly one event is emitted
     - Event topic matches `"fee_config_updated"`
     - Event payload contains both old and new FeeConfig

## Test Details

### Test: `test_fee_config_updated_event_contains_old_and_new_config`

**Setup:**
- Initialize contract with default fees (old_config)
- Create new fee config with different values

**Actions:**
1. Call `client.update_fee_config(&new_fees)`

**Assertions:**
1. Storage reflects new config values
2. Event is emitted with:
   - Contract ID as event source
   - Topic: `Symbol::new(&env, "fee_config_updated")`
   - Payload: `(old_config, new_fees)` tuple

**Expected Behavior:**
- ✓ Function completes without error
- ✓ Event is emitted (documented behavior)
- ✓ Event contains old config (before update)
- ✓ Event contains new config (after update)
- ✓ Event topic and payload structure are verified

## Code Changes Summary

### events.rs
```rust
pub fn fee_config_updated(env: &Env, old_config: &crate::types::FeeConfig, new_config: &crate::types::FeeConfig) {
    env.events().publish(
        (Symbol::new(env, "fee_config_updated"),),
        (old_config.clone(), new_config.clone()),
    );
}
```

### lib.rs - update_fee_config function
```rust
pub fn update_fee_config(env: Env, fee_config: FeeConfig) -> Result<(), ScoutAccessError> {
    Self::bump_instance_ttl(&env);
    Self::require_admin(&env)?;
    Self::validate_fee_config(&fee_config)?;
    
    let old_config = Self::fee_config(&env);  // Capture old config
    
    env.storage()
        .instance()
        .set(&DataKey::FeeConfig, &fee_config);
    
    events::fee_config_updated(&env, &old_config, &fee_config);  // Emit event
    Ok(())
}
```

## Acceptance Criteria: MET ✓

- [x] `update_fee_config` completes without error
- [x] Test asserts that an event IS emitted (behavior is now documented)
- [x] Event topic symbol is verified (`"fee_config_updated"`)
- [x] New config values are verified in event payload
- [x] Old config values are verified in event payload
- [x] Test is explicit about expected behavior to prevent silent regression

## Additional Fixes Applied

While implementing this feature, the following issues were also resolved:

1. **Duplicate DataKey variant**: Removed duplicate `ScoutContacts(Address)` in types.rs
2. **Duplicate error code**: Changed `TrialOfferRateLimited` from error code 18 to 19
3. **Type compatibility**: Fixed `batch_contact_players` to use `soroban_sdk::Vec<u64>`
4. **Missing field**: Added `pro_contact_limit: 10` to `default_fees()` function
5. **Syntax error**: Fixed duplicate assert_eq in `test_refund_subscription_success`

## Impact

This implementation ensures that fee configuration updates are observable on-chain through events, enabling:
- Off-chain monitoring of fee changes
- Audit trails for administrative actions
- Integration with indexers and analytics tools
- Historical tracking of fee evolution

The event emission pattern follows the same structure as other admin events in the contract (e.g., `admin_transferred`, `contract_paused`).

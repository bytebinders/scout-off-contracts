use soroban_sdk::{Address, Env, Symbol};
use crate::types::SubscriptionTier;

pub fn contract_initialized(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "contract_initialized"), admin.clone()),
        admin.clone(),
    );
}

pub fn scout_subscribed(env: &Env, scout: &Address, tier: &SubscriptionTier) {
    env.events().publish(
        (Symbol::new(env, "scout_subscribed"), scout.clone()),
        tier.clone(),
    );
}

pub fn player_contacted(env: &Env, player_id: u64, scout: &Address, fee_paid: i128) {
    env.events().publish(
        (Symbol::new(env, "player_contacted"), scout.clone()),
        (player_id, fee_paid),
    );
}

pub fn trial_offer_logged(env: &Env, player_id: u64, scout: &Address) {
    env.events().publish(
        (Symbol::new(env, "trial_offer_logged"), scout.clone()),
        player_id,
    );
}

pub fn fees_withdrawn(env: &Env, to: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "fees_withdrawn"), to.clone()),
        amount,
    );
}

pub fn contract_paused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "contract_paused"),),
        admin.clone(),
    );
}

pub fn contract_unpaused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "contract_unpaused"),),
        admin.clone(),
    );
}

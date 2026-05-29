use soroban_sdk::{contracttype, Address};

/// Mirrors the four-tier level in registration — kept in sync manually
/// (or via a shared crate in a future refactor).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProgressLevel {
    Unverified,
    VerifiedIdentity,
    PerformanceMilestones,
    EliteTier,
}

impl ProgressLevel {
    /// Returns the next valid [`ProgressLevel`] in the progression sequence,
    /// or `None` if the player is already at [`ProgressLevel::EliteTier`].
    ///
    /// This is the single source of truth for valid level transitions.
    /// All contract logic that advances a player's level must go through
    /// this method to ensure consistency.
    ///
    /// # Examples
    ///
    /// ```
    /// # use scoutchain_progress::types::ProgressLevel;
    /// assert_eq!(ProgressLevel::Unverified.next(), Some(ProgressLevel::VerifiedIdentity));
    /// assert_eq!(ProgressLevel::VerifiedIdentity.next(), Some(ProgressLevel::PerformanceMilestones));
    /// assert_eq!(ProgressLevel::PerformanceMilestones.next(), Some(ProgressLevel::EliteTier));
    /// assert_eq!(ProgressLevel::EliteTier.next(), None);
    /// ```
    pub fn next(&self) -> Option<ProgressLevel> {
        match self {
            ProgressLevel::Unverified => Some(ProgressLevel::VerifiedIdentity),
            ProgressLevel::VerifiedIdentity => Some(ProgressLevel::PerformanceMilestones),
            ProgressLevel::PerformanceMilestones => Some(ProgressLevel::EliteTier),
            ProgressLevel::EliteTier => None,
        }
    }
}

/// A single entry in the immutable progress history
#[contracttype]
#[derive(Clone, Debug)]
pub struct ProgressEntry {
    pub player_id: u64,
    pub old_level: ProgressLevel,
    pub new_level: ProgressLevel,
    /// Wallet that triggered the update (validator or scout)
    pub updated_by: Address,
    pub updated_at: u64,
    /// Milestone index from the verification contract that triggered this
    pub milestone_ref: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Paused,
    /// player_id → current ProgressLevel
    PlayerLevel(u64),
    /// history counter per player
    HistoryCounter(u64),
    /// (player_id, history_index) → ProgressEntry
    HistoryEntry(u64, u32),
    /// address of the verification contract (for cross-contract auth checks)
    VerificationContract,
}

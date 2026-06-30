pub use scoutchain_shared_types::ContractHealth;
use soroban_sdk::{contracttype, Address, String, Vec};

/// Richer validator status — distinguishes unregistered from revoked.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ValidatorStatus {
    NotRegistered,
    Active,
    Revoked,
}

/// A single verified milestone record
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub player_id: u64,
    pub validator: Address,
    pub description: String,
    /// IPFS/Arweave CID of supporting evidence (video clip, stat sheet, etc.)
    pub evidence_hash: String,
    pub approved_at: u64,
    /// Stellar ledger sequence at time of approval for tamper-proof auditability
    pub ledger_sequence: u32,
}

/// Validator entry in the trusted registry
#[contracttype]
#[derive(Clone, Debug)]
pub struct Validator {
    pub wallet: Address,
    /// Human-readable credential label (e.g. "UEFA B License", "Academy Director")
    pub credentials: String,
    pub registered_at: u64,
    pub active: bool,
}

/// Entry in the global milestone index for on-chain auditability.
#[contracttype]
#[derive(Clone, Debug)]
pub struct GlobalMilestoneEntry {
    pub player_id: u64,
    pub milestone_index: u32,
}

/// Paginated response for global milestone index queries.
#[contracttype]
#[derive(Clone, Debug)]
pub struct GlobalMilestoneIndexPage {
    pub entries: Vec<GlobalMilestoneEntry>,
    pub total: u32,
}

/// A player-initiated dispute for a milestone.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneDispute {
    pub player_id: u64,
    pub milestone_index: u32,
    pub reason: String,
    pub disputed_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Paused,
    ProgressContract,
    ProgressContractSet,
    Validator(Address),
    MilestoneCounter(u64),
    Milestone(u64, u32),
    ValidatorMilestoneCount(Address),
    ValidatorPlayerMilestoneCount(Address, u64),
    ValidatorVector,
    TotalMilestoneCount,
    GlobalMilestoneIndex,
    /// Persistent index: validator wallet → Vec<u64> of distinct player_ids
    /// for which that validator has approved at least one milestone.
    /// Updated on every `approve_milestone` call (duplicates are skipped).
    ValidatorPlayers(Address),
    MilestoneDispute(u64, u32),
}

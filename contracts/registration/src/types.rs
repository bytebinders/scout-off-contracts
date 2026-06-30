use soroban_sdk::{contracttype, Address, String, Vec};

pub use scoutchain_shared_types::{ContractHealth, ProgressLevel};

/// Basic player vitals stored on-chain
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlayerVitals {
    pub age: u32,
    pub position: String,
    pub region: String,
    pub nationality: String,
}

/// Internal on-chain player profile (no level — progress contract is the source of truth)
#[contracttype]
#[derive(Clone, Debug)]
pub struct StoredPlayerProfile {
    pub player_id: u64,
    pub wallet: Address,
    pub vitals: PlayerVitals,
    /// IPFS/Arweave CIDs for highlight reels and photos
    pub ipfs_hashes: Vec<String>,
    pub registered_at: u64,
    pub updated_at: u64,
}

/// Full on-chain player profile returned to callers.
/// `level` is derived from the progress contract at read time — it is NOT
/// persisted here.  `progress::get_level` is the single source of truth.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlayerProfile {
    pub player_id: u64,
    pub wallet: Address,
    pub vitals: PlayerVitals,
    /// IPFS/Arweave CIDs for highlight reels and photos
    pub ipfs_hashes: Vec<String>,
    pub level: ProgressLevel,
    pub registered_at: u64,
    pub updated_at: u64,
}

/// Lightweight player view for scout discovery (no IPFS hashes or wallet).
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlayerSummary {
    pub player_id: u64,
    pub vitals: PlayerVitals,
    pub level: ProgressLevel,
    pub updated_at: u64,
}

/// Paginated response from filter_players.
/// `next_cursor` is `0` when there are no more results.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FilterResult {
    pub profiles: Vec<PlayerProfile>,
    /// Pass this value as `cursor` in the next call to continue pagination.
    /// A value of `0` means there are no further results.
    pub next_cursor: u64,
}

/// Scout profile stored on-chain
#[contracttype]
#[derive(Clone, Debug)]
pub struct ScoutProfile {
    pub scout_id: u64,
    pub wallet: Address,
    pub region: String,
    pub verified: bool,
    pub registered_at: u64,
}

/// Storage keys for contract state
#[contracttype]
pub enum DataKey {
    /// Admin wallet address authorized to manage validators and fees
    Admin,
    /// Boolean flag indicating if contract has been initialized
    Initialized,
    /// Boolean flag indicating if contract is paused (circuit breaker)
    Paused,
    /// Counter for generating unique player IDs
    PlayerCounter,
    /// Counter for generating unique scout IDs
    ScoutCounter,
    /// Full player profile stored by player_id
    Player(u64),
    /// Index mapping player wallet address to player_id for fast lookup
    PlayerByWallet(Address),
    /// Full scout profile stored by scout_id
    Scout(u64),
    /// Index mapping scout wallet address to scout_id for fast lookup
    ScoutByWallet(Address),
    /// Index of all player IDs for efficient filtering and iteration
    PlayerIndex,
    /// Address of the progress contract allowed to call set_player_level
    ProgressContract,
    /// Composite index: (ProgressLevel, region) → Vec<u64> of player IDs.
    /// Used by `filter_players` for combined level+region queries so only
    /// matching players are loaded, avoiding a full scan of `PlayerIndex`.
    PlayersByLevelRegion(ProgressLevel, String),
    /// Per-level sub-index: ProgressLevel → Vec<u64> of player IDs.
    /// Primary lookup path for level-filtered queries without a region constraint.
    /// Falls back to `PlayerIndex` only when no level filter is specified.
    PlayersByLevel(ProgressLevel),
}

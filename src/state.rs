use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    /// Admin address (governance module - can pause/resume and withdraw)
    pub admin: String,
    /// Recipient address (receives GNK when tranches are released)
    pub recipient: String,
    /// Native token denomination
    pub native_denom: String,
    /// Whether contract is paused
    pub is_paused: bool,
    /// Block time (unix seconds) when contract was instantiated
    pub start_time: u64,
}

#[cw_serde]
pub struct Tranche {
    /// Tranche index (0–3)
    pub id: u32,
    /// GNK amount in ngonka to release (9 decimals)
    pub gnk_amount: Uint128,
    /// Unix timestamp after which this tranche can be released
    pub unlock_time: u64,
    /// Whether GNK has been sent to the recipient
    pub released: bool,
}

/// Contract configuration
pub const CONFIG: Item<Config> = Item::new("config");

/// Map from tranche_id (u32) -> Tranche
pub const TRANCHES: Map<u32, Tranche> = Map::new("tranches");

/// Number of tranches in the vesting schedule
pub const NUM_TRANCHES: u32 = 4;

/// GNK amounts per tranche in ngonka (9 decimals: 1 GNK = 1_000_000_000 ngonka)
pub const TRANCHE_GNK_AMOUNTS: [u128; 4] = [
    500_000_000_000_000, // Tranche 0: 500,000 GNK  — at signing
    150_000_000_000_000, // Tranche 1: 150,000 GNK  — +3 months
    150_000_000_000_000, // Tranche 2: 150,000 GNK  — +6 months
    170_000_000_000_000, // Tranche 3: 170,000 GNK  — +9 months
];
// Total: 970,000 GNK = 970_000_000_000_000 ngonka

/// Unlock offsets in seconds from contract instantiation
pub const TRANCHE_UNLOCK_OFFSETS: [u64; 4] = [
    0,                // Tranche 0: immediate
    90 * 24 * 3600,   // Tranche 1: 90 days  (~3 months)
    180 * 24 * 3600,  // Tranche 2: 180 days (~6 months)
    270 * 24 * 3600,  // Tranche 3: 270 days (~9 months)
];

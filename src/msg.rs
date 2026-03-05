use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    /// Admin address (governance module - can pause/resume and withdraw)
    pub admin: String,
    /// Recipient address (receives GNK when tranches are released)
    pub recipient: String,
    /// Native token denomination (e.g. "ngonka")
    pub native_denom: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Release GNK for a tranche whose unlock_time has passed
    ReleaseTranche { tranche_id: u32 },
    /// Admin: Pause the contract
    Pause {},
    /// Admin: Resume the contract
    Resume {},
    /// Admin: Update recipient address
    UpdateRecipient { recipient: String },
    /// Admin: Withdraw native tokens from contract
    WithdrawNativeTokens { amount: Uint128, recipient: String },
    /// Admin: Emergency withdraw all funds
    EmergencyWithdraw { recipient: String },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get contract configuration
    #[returns(ConfigResponse)]
    Config {},
    /// Get a single tranche by id
    #[returns(TrancheResponse)]
    Tranche { tranche_id: u32 },
    /// Get all 4 tranches
    #[returns(AllTranchesResponse)]
    AllTranches {},
    /// Get contract's native token balance
    #[returns(NativeBalanceResponse)]
    NativeBalance {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub recipient: String,
    pub native_denom: String,
    pub is_paused: bool,
    pub start_time: u64,
}

#[cw_serde]
pub struct TrancheResponse {
    pub id: u32,
    pub gnk_amount: Uint128,
    pub unlock_time: u64,
    pub released: bool,
}

#[cw_serde]
pub struct AllTranchesResponse {
    pub tranches: Vec<TrancheResponse>,
}

#[cw_serde]
pub struct NativeBalanceResponse {
    pub balance: Coin,
}

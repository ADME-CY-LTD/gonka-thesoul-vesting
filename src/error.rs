use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Contract is paused")]
    ContractPaused {},

    #[error("Zero amount not allowed")]
    ZeroAmount {},

    #[error("Insufficient contract balance: {available}, needed: {needed}")]
    InsufficientBalance { available: u128, needed: u128 },

    #[error("Tranche {id} not found (valid: 0-3)")]
    TrancheNotFound { id: u32 },

    #[error("Tranche {id} already released")]
    TrancheAlreadyReleased { id: u32 },

    #[error("Tranche {id} is locked until timestamp {unlock_at}")]
    TrancheNotUnlocked { id: u32, unlock_at: u64 },
}

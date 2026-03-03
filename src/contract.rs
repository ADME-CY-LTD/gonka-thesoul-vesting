use cosmwasm_std::{
    entry_point, to_json_binary, to_json_vec, BankMsg, Binary, Coin, Deps, DepsMut,
    Env, MessageInfo, Response, StdError, StdResult, Uint128, Uint256, QueryRequest, GrpcQuery,
    ContractResult, SystemResult,
};
use prost::Message;
use cw2::{get_contract_version, set_contract_version};

use crate::error::ContractError;
use crate::msg::{
    AllTranchesResponse, ConfigResponse, ExecuteMsg, InstantiateMsg,
    NativeBalanceResponse, QueryMsg, TrancheResponse,
};
use crate::state::{
    Config, Tranche, CONFIG, TRANCHES, NUM_TRANCHES, TRANCHE_GNK_AMOUNTS, TRANCHE_UNLOCK_OFFSETS,
};

#[derive(Clone, PartialEq, Message)]
pub struct QueryTotalSupplyRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct QueryTotalSupplyResponse {
    #[prost(message, repeated, tag = "1")]
    pub supply: ::prost::alloc::vec::Vec<CoinProto>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CoinProto {
    #[prost(string, tag = "1")]
    pub denom: String,
    #[prost(string, tag = "2")]
    pub amount: String,
}

const CONTRACT_NAME: &str = "gonka-thesoul-vesting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn get_native_denom(deps: Deps) -> Result<String, ContractError> {
    let request = QueryTotalSupplyRequest {};
    match query_proto::<QueryTotalSupplyRequest, QueryTotalSupplyResponse>(
        deps,
        "/cosmos.bank.v1beta1.Query/TotalSupply",
        &request,
    ) {
        Ok(response) => {
            if let Some(coin) = response.supply.first() {
                if !coin.denom.is_empty() {
                    return Ok(coin.denom.clone());
                }
            }
            Ok("ngonka".to_string())
        }
        Err(_) => Ok("ngonka".to_string()),
    }
}

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
        .map_err(|e| ContractError::Std(StdError::msg(e.to_string())))?;

    let admin = deps.api.addr_validate(&msg.admin)?.to_string();
    let recipient = deps.api.addr_validate(&msg.recipient)?.to_string();

    let native_denom = get_native_denom(deps.as_ref())?;
    let start_time = env.block.time.seconds();

    let config = Config {
        admin: admin.clone(),
        recipient: recipient.clone(),
        native_denom: native_denom.clone(),
        is_paused: false,
        start_time,
    };
    CONFIG.save(deps.storage, &config)?;

    // Create the 4 tranches
    for i in 0..(NUM_TRANCHES as usize) {
        let tranche = Tranche {
            id: i as u32,
            gnk_amount: Uint128::from(TRANCHE_GNK_AMOUNTS[i]),
            unlock_time: start_time + TRANCHE_UNLOCK_OFFSETS[i],
            released: false,
        };
        TRANCHES.save(deps.storage, i as u32, &tranche)?;
    }

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("admin", admin)
        .add_attribute("recipient", recipient)
        .add_attribute("native_denom", native_denom)
        .add_attribute("start_time", start_time.to_string())
        .add_attribute("tranches", NUM_TRANCHES.to_string()))
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReleaseTranche { tranche_id } => release_tranche(deps, env, tranche_id),
        ExecuteMsg::Pause {} => pause_contract(deps, info),
        ExecuteMsg::Resume {} => resume_contract(deps, info),
        ExecuteMsg::UpdateRecipient { recipient } => update_recipient(deps, info, recipient),
        ExecuteMsg::WithdrawNativeTokens { amount, recipient } => withdraw_native_tokens(deps, info, amount, recipient),
        ExecuteMsg::EmergencyWithdraw { recipient } => emergency_withdraw(deps, env, info, recipient),
    }
}

/// Release GNK for a tranche whose unlock_time has passed.
/// Can be called by anyone — GNK always goes to config.recipient.
fn release_tranche(
    deps: DepsMut,
    env: Env,
    tranche_id: u32,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if config.is_paused {
        return Err(ContractError::ContractPaused {});
    }

    let mut tranche = TRANCHES
        .load(deps.storage, tranche_id)
        .map_err(|_| ContractError::TrancheNotFound { id: tranche_id })?;

    if tranche.released {
        return Err(ContractError::TrancheAlreadyReleased { id: tranche_id });
    }

    let now = env.block.time.seconds();
    if now < tranche.unlock_time {
        return Err(ContractError::TrancheNotUnlocked {
            id: tranche_id,
            unlock_at: tranche.unlock_time,
        });
    }

    // Check contract balance (Coin.amount is Uint256, convert for comparison)
    let contract_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), &config.native_denom)?;
    let needed: Uint256 = tranche.gnk_amount.into();

    if contract_balance.amount < needed {
        return Err(ContractError::InsufficientBalance {
            available: Uint128::try_from(contract_balance.amount).unwrap_or(Uint128::MAX).u128(),
            needed: tranche.gnk_amount.u128(),
        });
    }

    // Mark released
    tranche.released = true;
    TRANCHES.save(deps.storage, tranche_id, &tranche)?;

    // Send GNK to recipient
    let send_msg = BankMsg::Send {
        to_address: config.recipient.clone(),
        amount: vec![Coin {
            denom: config.native_denom.clone(),
            amount: tranche.gnk_amount.into(),
        }],
    };

    Ok(Response::new()
        .add_message(send_msg)
        .add_attribute("method", "release_tranche")
        .add_attribute("tranche_id", tranche_id.to_string())
        .add_attribute("gnk_amount", tranche.gnk_amount)
        .add_attribute("recipient", config.recipient))
}

fn pause_contract(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender.as_str() != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    config.is_paused = true;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("method", "pause"))
}

fn resume_contract(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender.as_str() != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    config.is_paused = false;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("method", "resume"))
}

fn update_recipient(deps: DepsMut, info: MessageInfo, recipient: String) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender.as_str() != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    let validated_recipient = deps.api.addr_validate(&recipient)?.to_string();
    config.recipient = validated_recipient.clone();
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("method", "update_recipient")
        .add_attribute("recipient", validated_recipient))
}

fn withdraw_native_tokens(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
    recipient: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender.as_str() != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    let recipient_addr = deps.api.addr_validate(&recipient)?;
    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }
    let send_msg = BankMsg::Send {
        to_address: recipient_addr.to_string(),
        amount: vec![Coin {
            denom: config.native_denom,
            amount: amount.into(),
        }],
    };
    Ok(Response::new()
        .add_message(send_msg)
        .add_attribute("method", "withdraw")
        .add_attribute("amount", amount)
        .add_attribute("recipient", recipient))
}

fn emergency_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender.as_str() != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    let recipient_addr = deps.api.addr_validate(&recipient)?;
    let balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), &config.native_denom)?;

    if balance.amount.is_zero() {
        return Ok(Response::new()
            .add_attribute("method", "emergency_withdraw")
            .add_attribute("message", "no_funds"));
    }

    let send_msg = BankMsg::Send {
        to_address: recipient_addr.to_string(),
        amount: vec![balance.clone()],
    };
    Ok(Response::new()
        .add_message(send_msg)
        .add_attribute("method", "emergency_withdraw")
        .add_attribute("amount", balance.amount)
        .add_attribute("recipient", recipient))
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Tranche { tranche_id } => to_json_binary(&query_tranche(deps, tranche_id)?),
        QueryMsg::AllTranches {} => to_json_binary(&query_all_tranches(deps)?),
        QueryMsg::NativeBalance {} => to_json_binary(&query_native_balance(deps, env)?),
    }
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Binary) -> Result<Response, ContractError> {
    let old = get_contract_version(deps.storage)
        .map_err(|e| ContractError::Std(StdError::msg(e.to_string())))?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
        .map_err(|e| ContractError::Std(StdError::msg(e.to_string())))?;
    Ok(Response::new()
        .add_attribute("action", "migrate")
        .add_attribute("from_version", old.version)
        .add_attribute("to_version", CONTRACT_VERSION))
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        admin: config.admin,
        recipient: config.recipient,
        native_denom: config.native_denom,
        is_paused: config.is_paused,
        start_time: config.start_time,
    })
}

fn query_tranche(deps: Deps, tranche_id: u32) -> StdResult<TrancheResponse> {
    let t = TRANCHES.load(deps.storage, tranche_id)?;
    Ok(TrancheResponse {
        id: t.id,
        gnk_amount: t.gnk_amount,
        unlock_time: t.unlock_time,
        released: t.released,
    })
}

fn query_all_tranches(deps: Deps) -> StdResult<AllTranchesResponse> {
    let mut tranches = Vec::with_capacity(NUM_TRANCHES as usize);
    for i in 0..NUM_TRANCHES {
        let t = TRANCHES.load(deps.storage, i)?;
        tranches.push(TrancheResponse {
            id: t.id,
            gnk_amount: t.gnk_amount,
            unlock_time: t.unlock_time,
            released: t.released,
        });
    }
    Ok(AllTranchesResponse { tranches })
}

fn query_native_balance(deps: Deps, env: Env) -> StdResult<NativeBalanceResponse> {
    let config = CONFIG.load(deps.storage)?;
    let balance = deps
        .querier
        .query_balance(&env.contract.address, &config.native_denom)?;
    Ok(NativeBalanceResponse { balance })
}

fn query_grpc(deps: Deps, path: &str, data: Binary) -> StdResult<Binary> {
    let request = QueryRequest::Grpc(GrpcQuery {
        path: path.to_string(),
        data,
    });
    query_raw(deps, &request)
}

fn query_raw(deps: Deps, request: &QueryRequest<GrpcQuery>) -> StdResult<Binary> {
    let raw = to_json_vec(request).map_err(|e| StdError::msg(format!("Serializing: {e}")))?;
    match deps.querier.raw_query(&raw) {
        SystemResult::Err(e) => Err(StdError::msg(format!("System error: {e}"))),
        SystemResult::Ok(ContractResult::Err(e)) => Err(StdError::msg(format!("Contract error: {e}"))),
        SystemResult::Ok(ContractResult::Ok(value)) => Ok(value),
    }
}

fn query_proto<TRequest, TResponse>(deps: Deps, path: &str, request: &TRequest) -> StdResult<TResponse>
where
    TRequest: prost::Message,
    TResponse: prost::Message + Default,
{
    let mut buf = Vec::new();
    request.encode(&mut buf).map_err(|e| StdError::msg(format!("Encode: {}", e)))?;
    let bytes = query_grpc(deps, path, Binary::from(buf))?;
    TResponse::decode(bytes.as_slice()).map_err(|e| StdError::msg(format!("Decode: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi};
    use cosmwasm_std::{from_json, Addr, MessageInfo};

    fn mock_instantiate_msg(api: &MockApi) -> InstantiateMsg {
        InstantiateMsg {
            admin: api.addr_make("admin").to_string(),
            recipient: api.addr_make("recipient").to_string(),
        }
    }

    #[test]
    fn proper_instantiation() {
        let deps = mock_dependencies();
        let api = MockApi::default();
        let recipient_addr = api.addr_make("recipient").to_string();

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };

        let res = instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();
        assert!(res.attributes.iter().any(|a| a.key == "recipient" && a.value == recipient_addr));
        assert!(res.attributes.iter().any(|a| a.key == "tranches" && a.value == "4"));

        let all: AllTranchesResponse =
            from_json(&query(deps.as_ref(), env, QueryMsg::AllTranches {}).unwrap()).unwrap();

        assert_eq!(all.tranches.len(), 4);
        assert_eq!(all.tranches[0].gnk_amount, Uint128::from(500_000_000_000_000u128));
        assert_eq!(all.tranches[1].gnk_amount, Uint128::from(150_000_000_000_000u128));
        assert_eq!(all.tranches[2].gnk_amount, Uint128::from(150_000_000_000_000u128));
        assert_eq!(all.tranches[3].gnk_amount, Uint128::from(170_000_000_000_000u128));
        assert!(!all.tranches[0].released);
    }

    #[test]
    fn test_pause_resume() {
        let deps = mock_dependencies();
        let api = MockApi::default();
        let admin_addr = api.addr_make("admin");

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        let info = MessageInfo {
            sender: admin_addr.clone(),
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info.clone(), ExecuteMsg::Pause {}).unwrap();

        let config: ConfigResponse =
            from_json(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()).unwrap();
        assert!(config.is_paused);

        execute(deps.as_mut(), env.clone(), info, ExecuteMsg::Resume {}).unwrap();
        let config: ConfigResponse =
            from_json(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
        assert!(!config.is_paused);
    }

    #[test]
    fn test_update_recipient() {
        let deps = mock_dependencies();
        let api = MockApi::default();
        let admin_addr = api.addr_make("admin");
        let new_recipient = api.addr_make("newrecipient").to_string();

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        let info = MessageInfo {
            sender: admin_addr,
            funds: vec![],
        };
        execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::UpdateRecipient { recipient: new_recipient.clone() },
        )
        .unwrap();

        let config: ConfigResponse =
            from_json(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
        assert_eq!(config.recipient, new_recipient);
    }

    #[test]
    fn test_release_tranche_0() {
        let deps = mock_dependencies();
        let api = MockApi::default();

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        // Fund the contract
        deps.querier.bank.update_balance(
            env.contract.address.to_string(),
            vec![Coin {
                denom: "ngonka".to_string(),
                amount: Uint128::from(970_000_000_000_000u128).into(),
            }],
        );

        // Anyone can release tranche 0 (immediate)
        let info = MessageInfo {
            sender: Addr::unchecked("anyone"),
            funds: vec![],
        };
        let res = execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ReleaseTranche { tranche_id: 0 },
        )
        .unwrap();

        assert!(res.messages.len() == 1);
        assert!(res.attributes.iter().any(|a| a.key == "gnk_amount" && a.value == "500000000000000"));

        // Verify tranche is marked released
        let t: TrancheResponse =
            from_json(&query(deps.as_ref(), env, QueryMsg::Tranche { tranche_id: 0 }).unwrap())
                .unwrap();
        assert!(t.released);
    }

    #[test]
    fn test_cannot_release_locked_tranche() {
        let deps = mock_dependencies();
        let api = MockApi::default();

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        deps.querier.bank.update_balance(
            env.contract.address.to_string(),
            vec![Coin {
                denom: "ngonka".to_string(),
                amount: Uint128::from(970_000_000_000_000u128).into(),
            }],
        );

        let info = MessageInfo {
            sender: Addr::unchecked("anyone"),
            funds: vec![],
        };
        let err = execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ReleaseTranche { tranche_id: 1 },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::TrancheNotUnlocked { id: 1, .. }));
    }

    #[test]
    fn test_release_tranche_after_unlock() {
        let deps = mock_dependencies();
        let api = MockApi::default();

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        deps.querier.bank.update_balance(
            env.contract.address.to_string(),
            vec![Coin {
                denom: "ngonka".to_string(),
                amount: Uint128::from(970_000_000_000_000u128).into(),
            }],
        );

        // Advance time by 91 days
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(91 * 24 * 3600);

        let info = MessageInfo {
            sender: Addr::unchecked("anyone"),
            funds: vec![],
        };
        let res = execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ReleaseTranche { tranche_id: 1 },
        )
        .unwrap();

        assert!(res.attributes.iter().any(|a| a.key == "gnk_amount" && a.value == "150000000000000"));
    }

    #[test]
    fn test_cannot_release_twice() {
        let deps = mock_dependencies();
        let api = MockApi::default();

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        deps.querier.bank.update_balance(
            env.contract.address.to_string(),
            vec![Coin {
                denom: "ngonka".to_string(),
                amount: Uint128::from(970_000_000_000_000u128).into(),
            }],
        );

        let info = MessageInfo {
            sender: Addr::unchecked("anyone"),
            funds: vec![],
        };

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ReleaseTranche { tranche_id: 0 },
        )
        .unwrap();

        let err = execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ReleaseTranche { tranche_id: 0 },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::TrancheAlreadyReleased { id: 0 }));
    }

    #[test]
    fn test_pause_blocks_release() {
        let deps = mock_dependencies();
        let api = MockApi::default();
        let admin_addr = api.addr_make("admin");

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        deps.querier.bank.update_balance(
            env.contract.address.to_string(),
            vec![Coin {
                denom: "ngonka".to_string(),
                amount: Uint128::from(970_000_000_000_000u128).into(),
            }],
        );

        // Admin pauses
        let info = MessageInfo {
            sender: admin_addr,
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info, ExecuteMsg::Pause {}).unwrap();

        // Release should fail
        let info = MessageInfo {
            sender: Addr::unchecked("anyone"),
            funds: vec![],
        };
        let err = execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ReleaseTranche { tranche_id: 0 },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::ContractPaused {}));
    }

    #[test]
    fn test_unauthorized_update() {
        let deps = mock_dependencies();
        let api = MockApi::default();
        let attacker = api.addr_make("attacker");
        let hacker = api.addr_make("hacker").to_string();

        let mut deps = deps;
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        instantiate(deps.as_mut(), env.clone(), info, mock_instantiate_msg(&api)).unwrap();

        let info = MessageInfo {
            sender: attacker,
            funds: vec![],
        };
        let err = execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::UpdateRecipient { recipient: hacker },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }
}

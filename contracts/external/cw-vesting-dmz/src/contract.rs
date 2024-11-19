use core::panic;
use std::io::BufRead;
use std::result::Result;

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, InstantiateMsg, QueryManagedDenomResponse, QueryMsg, QueryPendingClaimResponse,
    QueryPendingClaimsResponse, MigrateMsg,
};
use crate::state::{
    add_balance, add_claimed, assert_admin, get_admin, get_balance, get_balances, get_claimed,
    get_current_balance, get_managed_balance, get_managed_denom, get_max_balance_account,
    get_total_claimed, get_weights, reduce_balance, reduce_managed_balance, set_admin,
    set_managed_balance, set_managed_denom, set_weights, sum_balances, validate_admin,
    validate_weights,
};
use crate::util::split_number_with_weights;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128,
};
use cw2::set_contract_version;

const CONTRACT_NAME: &str = "crates.io:cw-vesting-dmz";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn migrate(
    deps: DepsMut,
    env: Env,
    msg: MigrateMsg
) -> Result<Response, ContractError> {

    match msg.weights {
        Some(weights) => {
            let mut store = deps.storage;
            let total_claimed = get_total_claimed(store)?;
            if !total_claimed.is_zero() {
                return Err(ContractError::Std(StdError::generic_err("Cannot migrate to new weights with executed claims")));
            }
            let managed_bal = get_managed_balance(store)?;
            if !managed_bal.is_zero() {
                return Err(ContractError::Std(StdError::generic_err("Cannot migrate to new weights with managed balance")));
            }
            set_weights(store, deps.api, weights)?;
        },
        None => {}
    }
    
    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    set_managed_denom(deps.storage, msg.managed_denom)?;
    set_managed_balance(deps.storage, Uint128::zero())?;
    set_weights(deps.storage, deps.api, msg.weights)?;
    validate_admin(deps.api, msg.admin.clone())?;
    match msg.admin {
        Some(admin) => set_admin(deps.storage, deps.api, Some(admin))?,
        None => set_admin(deps.storage, deps.api, Some(info.sender.into_string()))?,
    }

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = info.sender.clone().into_string();
    match msg {
        ExecuteMsg::UpdateClaims {} => execute_update_claims(deps, env, info),
        ExecuteMsg::Claim {} => execute_withdraw(deps, env, info, sender),
        ExecuteMsg::SetAdmin { admin } => execute_set_admin(deps, info, admin),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Admin {} => to_json_binary(&get_admin(deps.storage)?),
        QueryMsg::PendingClaim { address } => query_claim(deps, address),
        QueryMsg::PendingClaims {} => query_claims(deps),
        QueryMsg::Claimed { address } => query_claimed(deps, address),
        QueryMsg::TotalClaimed {} => Ok(to_json_binary(&get_total_claimed(deps.storage)?)?),
        QueryMsg::Denom {} => query_denom(deps),
    }
}

pub fn execute_update_claims(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // 1st) Check admin privileges
    assert_admin(deps.storage, info.sender.into_string())?;

    // 2nd) get the current balance and the managed balance
    let balance = get_current_balance(deps.storage, deps.querier, env)?;
    let managed_balance = get_managed_balance(deps.storage)?;

    // 3rd) set managed balance to the actual balance
    set_managed_balance(deps.storage, balance)?;

    // 4th) calculate the difference between the two balances
    // the checked sub errors if the managed balance is greater
    // than the actual balance -> which should never happen
    let diff_balance = match balance.checked_sub(managed_balance) {
        Ok(diff) => diff,
        Err(_) => {
            return Err(ContractError::Std(StdError::generic_err(
                "Managed balance is greater than the actual balance",
            )))
        }
    };

    // 5th from the difference calculate the shares for each address
    // and add them to the claimbable balances
    let weights = get_weights(deps.storage)?;
    let shares = split_number_with_weights(diff_balance, weights)?;
    // -> increase all balances with the difference
    for (address, share) in shares {
        add_balance(deps.storage, deps.api, address, share)?;
    }

    // 6th) we need to correct rounding errors - if the sum of the shares is
    // less than the difference then we need to add the difference to the address
    // with the highest weight correct the rounding error by accounting it to the
    // address with the highest balance so that the impact of the roundig error
    // is minimized
    let sum_of_balances = sum_balances(deps.storage)?;
    let actual_balance = balance.clone();
    let max_balance_acc = get_max_balance_account(deps.storage)?;
    if actual_balance.gt(&sum_of_balances) {
        let diff = actual_balance.checked_sub(sum_of_balances).unwrap();
        add_balance(deps.storage, deps.api, max_balance_acc, diff)?;
    } else if actual_balance.lt(&sum_of_balances) {
        let diff = sum_of_balances.checked_sub(actual_balance).unwrap();
        reduce_balance(deps.storage, deps.api, max_balance_acc, diff)?;
    }

    Ok(Response::new())
}

pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    // 1st decrease the managed balance by the balance of the address
    let withdraw_amount = get_balance(deps.storage, address.clone())?;
    if withdraw_amount.is_zero() {
        return Err(ContractError::Std(StdError::generic_err(
            "No balance to withdraw",
        )));
    }
    reduce_managed_balance(deps.storage, withdraw_amount)?;

    // 2nd decrease the balance of the address to zero
    reduce_balance(deps.storage, deps.api, address.clone(), withdraw_amount)?;

    // 3rd increase the claimed amount of the address by the balance of the address
    add_claimed(deps.storage, deps.api, address.clone(), withdraw_amount)?;

    // 4th emit message to send the withdrawn amount to the address
    let recipient = deps.api.addr_validate(&address)?;
    let denom = get_managed_denom(deps.storage)?;
    let transfer_msg = denom.get_transfer_to_message(&recipient, withdraw_amount)?;
    Ok(Response::new().add_message(transfer_msg))
}

pub fn execute_set_admin(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    // 1st) Check admin privileges
    assert_admin(deps.storage, info.sender.into_string())?;

    // 2nd) set the new admin
    set_admin(deps.storage, deps.api, Some(address))?;

    Ok(Response::new())
}

pub fn query_claims(deps: Deps) -> StdResult<Binary> {
    let balances = get_balances(deps.storage)?;
    let formatted_balances = balances
        .iter()
        .map(|item| QueryPendingClaimResponse {
            address: item.0.clone(),
            amount: item.1,
        })
        .collect();
    let total = sum_balances(deps.storage)?;
    let resp = QueryPendingClaimsResponse {
        claims: formatted_balances,
        total,
    };
    Ok(to_json_binary(&resp)?)
}

pub fn query_claim(deps: Deps, address: String) -> StdResult<Binary> {
    let balance = get_balance(deps.storage, address.clone())?;
    let resp = QueryPendingClaimResponse {
        address: address,
        amount: balance,
    };
    Ok(to_json_binary(&resp)?)
}

pub fn query_claimed(deps: Deps, address: String) -> StdResult<Binary> {
    let claimed_amount = get_claimed(deps.storage, address.clone())?;
    let resp = QueryPendingClaimResponse {
        address: address,
        amount: claimed_amount,
    };
    Ok(to_json_binary(&resp)?)
}

pub fn query_denom(deps: Deps) -> StdResult<Binary> {
    let denom = get_managed_denom(deps.storage)?;
    let amount = get_managed_balance(deps.storage)?;
    let resp = QueryManagedDenomResponse {
        managed_denom: denom,
        amount,
    };
    Ok(to_json_binary(&resp)?)
}

#[cfg(test)]
mod test {

    use std::borrow::Borrow;

    use crate::error::ContractError;
    use crate::msg::InstantiateMsg;
    use crate::state::{get_admin, get_managed_balance, get_weights, set_claimed};
    use crate::test_util::{
        get_mocked_balance, mock_contract, set_mocked_cw20_balance, set_mocked_native_balance,
        wasm_query_handler,
    };
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info, MockApi, MockQuerier},
        Addr, BankMsg, Coin, CosmosMsg, Decimal, Env, MemoryStorage, OwnedDeps, Response, Uint128,
    };

    use super::instantiate;

    #[test]
    fn instantiate_works_with_native() {
        let msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
                ("addr0002".to_string(), Decimal::percent(30)),
                ("addr0003".to_string(), Decimal::percent(40)),
            ],
        };
        match mock_contract(msg) {
            Ok(_) => {}
            Err(e) => panic!("Should not have failed"),
        }
    }

    #[test]
    fn instantiate_works_with_cw20() {
        let msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Cw20(Addr::unchecked("token")),
            weights: vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
                ("addr0002".to_string(), Decimal::percent(30)),
                ("addr0003".to_string(), Decimal::percent(40)),
            ],
        };
        match mock_contract(msg) {
            Ok(_) => {}
            Err(e) => panic!("Should not have failed"),
        }
    }

    #[test]
    fn instantiate_rejects_with_unmatched_weights() {
        let msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
            ],
        };
        match mock_contract(msg) {
            Ok(_) => panic!("Should have failed"),
            Err(e) => assert_eq!(
                ContractError::Std(cosmwasm_std::StdError::GenericErr {
                    msg: "weights must sum up to 1".into()
                }),
                e
            ),
        }
    }

    #[test]
    fn execute_update_claims_works() {
        // mock the contract
        let init_msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
                ("addr0002".to_string(), Decimal::percent(30)),
                ("addr0003".to_string(), Decimal::percent(40)),
            ],
        };
        let (mut deps, env) = mock_contract(init_msg).unwrap();

        // execute the update claims cannot be executed from a non-admin
        let info = mock_info("non-admin", &[]);
        let res = super::execute_update_claims(deps.as_mut(), env.clone(), info).unwrap_err();
        assert_eq!(
            ContractError::Std(cosmwasm_std::StdError::GenericErr {
                msg: "unauthorized".into()
            }),
            res
        );

        // execute the update claims from admin
        let info = mock_info("admin", &[]);
        let res = super::execute_update_claims(deps.as_mut(), env.clone(), info).unwrap();
        assert_eq!(0, res.messages.len());

        //Check the balances
        let managed_balance = super::get_managed_balance(deps.as_ref().storage).unwrap();
        assert_eq!(get_mocked_balance("contract".to_string()), managed_balance);
        let balance = super::get_balance(deps.as_ref().storage, "addr0000".to_string()).unwrap();
        assert_eq!(Uint128::from(44_400_000u32), balance);
        let balance = super::get_balance(deps.as_ref().storage, "addr0001".to_string()).unwrap();
        assert_eq!(Uint128::from(88_800_000u32), balance);
        let balance = super::get_balance(deps.as_ref().storage, "addr0002".to_string()).unwrap();
        assert_eq!(Uint128::from(133_200_000u32), balance);
        let balance = super::get_balance(deps.as_ref().storage, "addr0003".to_string()).unwrap();
        assert_eq!(Uint128::from(177_600_000u32), balance);
    }

    #[test]
    fn update_claims_with_non_dividing_weights() {
        // mock the contract
        // the weights do not divide the balance evenly and more so
        // the result of the division is rounded up for both addresses
        // and therefore the sum of the balances would be greater than
        // the actual balance if the rounding error is not accounted for
        // correctly
        let init_msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: vec![
                ("addr0000".to_string(), Decimal::from_ratio(1u32, 512u32)),
                ("addr0001".to_string(), Decimal::from_ratio(511u32, 512u32)),
            ],
        };
        let (mut deps, env) = mock_contract(init_msg).unwrap();

        // execute the update claims from admin
        let info = mock_info("admin", &[]);
        let res = super::execute_update_claims(deps.as_mut(), env.clone(), info).unwrap();
        assert_eq!(0, res.messages.len());

        // Check the total managed balance is not messed up through the rounding error
        let managed_balance = super::get_managed_balance(deps.as_ref().storage).unwrap();
        assert_eq!(get_mocked_balance("contract".to_string()), managed_balance);

        // balance 1 should be rounded up as expected
        let balance = super::get_balance(deps.as_ref().storage, "addr0000".to_string()).unwrap();
        assert_eq!(Uint128::from(867188u32), balance);

        // balance 2 is internally rounded up but the rounding error is accounted to it afterwards
        let balance = super::get_balance(deps.as_ref().storage, "addr0001".to_string()).unwrap();
        assert_eq!(Uint128::from(443132812u32), balance);
    }

    #[test]
    fn execute_withdraw_works() {
        // mock the contract
        let init_msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
                ("addr0002".to_string(), Decimal::percent(30)),
                ("addr0003".to_string(), Decimal::percent(40)),
            ],
        };
        let (mut deps, env) = mock_contract(init_msg).unwrap();

        // execute the update claims from admin
        let info = mock_info("admin", &[]);
        let res = super::execute_update_claims(deps.as_mut(), env.clone(), info).unwrap();
        assert_eq!(0, res.messages.len());

        // execute the withdraw from addr0000
        let info = mock_info("addr0000", &[]);
        let res = super::execute_withdraw(deps.as_mut(), env.clone(), info, "addr0000".to_string())
            .unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Bank(BankMsg::Send {
                to_address: "addr0000".to_string(),
                amount: vec![Coin::new(44_400_000u128, "uusd")],
            })
        );

        // check the balances
        // -> addr0000 should have 0 balance
        // -> addr0001 should have 88_800_000
        // -> addr0002 should have 133_200_000
        // -> addr0003 should have 177_600_000
        // -> managed balance should be 177_600_000
        let balance = super::get_balance(deps.as_ref().storage, "addr0000".to_string()).unwrap();
        assert_eq!(Uint128::zero(), balance);
        let balance = super::get_balance(deps.as_ref().storage, "addr0001".to_string()).unwrap();
        assert_eq!(Uint128::from(88_800_000u32), balance);
        let balance = super::get_balance(deps.as_ref().storage, "addr0002".to_string()).unwrap();
        assert_eq!(Uint128::from(133_200_000u32), balance);
        let balance = super::get_balance(deps.as_ref().storage, "addr0003".to_string()).unwrap();
        assert_eq!(Uint128::from(177_600_000u32), balance);
        let managed_balance = super::get_managed_balance(deps.as_ref().storage).unwrap();
        assert_eq!(Uint128::from(399_600_000u32), managed_balance);

        // check failed withdraw on zero balance
        let info = mock_info("addr0000", &[]);
        let res = super::execute_withdraw(deps.as_mut(), env.clone(), info, "addr0000".to_string()).unwrap_err();
        assert_eq!(res, ContractError::Std(cosmwasm_std::StdError::GenericErr {
            msg: "No balance to withdraw".into()
        }));
    }

    #[test]
    fn set_admin() {
        // mock the contract
        let init_msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
                ("addr0002".to_string(), Decimal::percent(30)),
                ("addr0003".to_string(), Decimal::percent(40)),
            ],
        };
        let (mut deps, env) = mock_contract(init_msg).unwrap();
        let info = mock_info("admin", &[]);

        // set the new admin
        let res = super::execute_set_admin(deps.as_mut(), info, String::from("new_admin")).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(
            get_admin(deps.as_ref().storage).unwrap().unwrap(),
            String::from("new_admin")
        );

        // set the new admin is not possible from a non-admin
        let info = mock_info("non-admin", &[]);
        let res =
            super::execute_set_admin(deps.as_mut(), info, String::from("new_admin")).unwrap_err();
        assert_eq!(
            res,
            ContractError::Std(cosmwasm_std::StdError::GenericErr {
                msg: "unauthorized".into()
            })
        );
    }

    #[test]
    fn test_set_new_weights_on_migration() {

        // mock the contract
        let old_weights = vec![
            ("addr0000".to_string(), Decimal::percent(10)),
            ("addr0001".to_string(), Decimal::percent(20)),
            ("addr0002".to_string(), Decimal::percent(30)),
            ("addr0003".to_string(), Decimal::percent(40)),
        ];
        let init_msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: old_weights.clone(),
        };
        let (mut deps, env) = mock_contract(init_msg).unwrap();

        // set the new weights
        let new_weights = vec![
            ("addr0000".to_string(), Decimal::percent(20)),
            ("addr0001".to_string(), Decimal::percent(30)),
            ("addr0002".to_string(), Decimal::percent(40)),
            ("addr0003".to_string(), Decimal::percent(10)),
        ];

        let msg = super::MigrateMsg {
            weights: Some(new_weights.clone()),
        };

        // this should work
        let res = super::migrate(deps.as_mut(), env.clone(), msg.clone()).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(get_weights(deps.as_ref().storage).unwrap(), new_weights);


    }

    #[test]
    fn test_reject_new_weights_on_migration_if_contract_active() {

        // mock the contract
        let old_weights = vec![
            ("addr0000".to_string(), Decimal::percent(10)),
            ("addr0001".to_string(), Decimal::percent(20)),
            ("addr0002".to_string(), Decimal::percent(30)),
            ("addr0003".to_string(), Decimal::percent(40)),
        ];
        let init_msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
            weights: old_weights.clone(),
        };
        let (mut deps, env) = mock_contract(init_msg).unwrap();
        let info = mock_info("admin", &[]);

        // new weights
        let new_weights = vec![
            ("addr0000".to_string(), Decimal::percent(20)),
            ("addr0001".to_string(), Decimal::percent(30)),
            ("addr0002".to_string(), Decimal::percent(40)),
            ("addr0003".to_string(), Decimal::percent(10)),
        ];
        let msg = super::MigrateMsg {
            weights: Some(new_weights.clone()),
        };

        // execute the update claims from admin
        let res = super::execute_update_claims(deps.as_mut(), env.clone(), info).unwrap();

        // this should NOT work as we have active managed balance
        let res = super::migrate(deps.as_mut(), env.clone(), msg.clone()).unwrap_err();
        assert_eq!(res, ContractError::Std(cosmwasm_std::StdError::GenericErr {msg: "Cannot migrate to new weights with managed balance".into()}));
        assert_eq!(get_weights(deps.as_mut().storage).unwrap(), old_weights);

    }

    #[test]
    fn test_reject_new_weights_on_migration_when_claims_executed() {
            
            let old_weights = vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
                ("addr0002".to_string(), Decimal::percent(30)),
                ("addr0003".to_string(), Decimal::percent(40)),
            ];
            // mock the contract
            let init_msg = InstantiateMsg {
                admin: None,
                managed_denom: cw_denom::CheckedDenom::Native("uusd".to_string()),
                weights: old_weights.clone(),
            };
            let (mut deps, env) = mock_contract(init_msg).unwrap();
            let info = mock_info("admin", &[]);
    
            // new weights
            let new_weights = vec![
                ("addr0000".to_string(), Decimal::percent(20)),
                ("addr0001".to_string(), Decimal::percent(30)),
                ("addr0002".to_string(), Decimal::percent(40)),
                ("addr0003".to_string(), Decimal::percent(10)),
            ];
            let msg = super::MigrateMsg {
                weights: Some(new_weights.clone()),
            };
    
            // execute the update claims from admin
            let res = super::execute_update_claims(deps.as_mut(), env.clone(), info).unwrap();

            // let all accounts withdraw to make the claims executed
            let info = mock_info("addr0000", &[]);
            super::execute_withdraw(deps.as_mut(), env.clone(), info, "addr0000".to_string()).unwrap();
            let info = mock_info("addr0001", &[]);
            super::execute_withdraw(deps.as_mut(), env.clone(), info, "addr0001".to_string()).unwrap();
            let info = mock_info("addr0002", &[]);
            super::execute_withdraw(deps.as_mut(), env.clone(), info, "addr0002".to_string()).unwrap();
            let info = mock_info("addr0003", &[]);
            super::execute_withdraw(deps.as_mut(), env.clone(), info, "addr0003".to_string()).unwrap();

            // assert managed balance is zero now
            assert_eq!(get_managed_balance(deps.as_mut().storage).unwrap(), Uint128::zero());
    
            // this should NOT work - managed balance is zero but claims have been executed
            let res = super::migrate(deps.as_mut(), env.clone(), msg.clone()).unwrap_err();
            assert_eq!(res, ContractError::Std(cosmwasm_std::StdError::GenericErr {msg: "Cannot migrate to new weights with executed claims".into()}));
            assert_eq!(get_weights(deps.as_mut().storage).unwrap(), old_weights);
    }
}

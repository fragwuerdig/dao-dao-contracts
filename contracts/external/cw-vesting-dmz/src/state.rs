
use cosmwasm_std::{
    Api, Decimal, DecimalRangeExceeded, Deps, DepsMut, Env, MessageInfo, Order, QuerierWrapper, StdError, StdResult, Storage, Uint128
};
use cw_denom::CheckedDenom;
use cw_storage_plus::{Item, Map};

use crate::util::round_dec_closest;

// --------------------------
//
// ADMIN
//
// --------------------------
pub const ADMIN: Item<String> = Item::new("admin");

pub fn validate_admin(api: &dyn Api, address: Option<String>) -> StdResult<()> {
    match address {
        Some(address) => {
            api.addr_validate(&address)?;
        }
        None => {}
    }
    Ok(())
}

pub fn set_admin(store: &mut dyn Storage, api: &dyn Api, address: Option<String>) -> StdResult<()> {
    match address {
        Some(address) => {
            api.addr_validate(&address)?;
            ADMIN.save(store, &address)?;
        }
        None => {
            ADMIN.save(store, &"".to_string())?;
        }
    }
    Ok(())
}

pub fn get_admin(store: &dyn Storage) -> StdResult<Option<String>> {
    Ok(ADMIN.may_load(store)?)
}

pub fn is_admin(store: &dyn Storage, address: String) -> StdResult<bool> {
    let admin = ADMIN.may_load(store)?;
    match admin {
        Some(admin) => Ok(admin == address),
        None => Ok(false),
    }
}

pub fn assert_admin(store: &dyn Storage, address: String) -> StdResult<()> {
    if !is_admin(store, address.clone())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    Ok(())
}

// --------------------------
//
// MANAGED DENOM
//
// --------------------------
pub const MANAGED_DENOM: Item<CheckedDenom> = Item::new("managed_denom");

pub fn set_managed_denom(store: &mut dyn Storage, denom: CheckedDenom) -> StdResult<()> {
    MANAGED_DENOM.save(store, &denom)?;
    Ok(())
}

pub fn get_managed_denom(store: &dyn Storage) -> StdResult<CheckedDenom> {
    Ok(MANAGED_DENOM.load(store)?)
}

pub fn get_current_balance(store: &dyn Storage, querier: QuerierWrapper, env: Env) -> StdResult<Uint128> {
    let denom = get_managed_denom(store)?;
    match denom {
        CheckedDenom::Native(denom) => {
            let balance = querier.query_balance(&env.contract.address, denom)?;
            Ok(balance.amount)
        }
        CheckedDenom::Cw20(addr) => {
            let query_msg = cw20::Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            };
            let balance: cw20::BalanceResponse = querier.query_wasm_smart(addr, &query_msg)?;
            Ok(balance.balance)
        }
    }
}

// --------------------------
//
// MANAGED BALANCE
// Is the total amount of tokens managed by this contract
// which is different from the actual balance of the contract
//
// --------------------------
pub const MANAGED_BALANCE: Item<Uint128> = Item::new("managed_balance");

pub fn set_managed_balance(store: &mut dyn Storage, amount: Uint128) -> StdResult<()> {
    MANAGED_BALANCE.save(store, &amount)?;
    Ok(())
}

pub fn get_managed_balance(store: &dyn Storage) -> StdResult<Uint128> {
    Ok(MANAGED_BALANCE.load(store)?)
}

pub fn reduce_managed_balance(store: &mut dyn Storage, amount: Uint128) -> StdResult<()> {
    let managed_balance = match MANAGED_BALANCE.may_load(store)? {
        Some(managed_balance) => managed_balance.checked_sub(amount)?,
        None => return Err(StdError::generic_err("managed balance not found")),
    };
    MANAGED_BALANCE.save(store, &managed_balance)?;
    Ok(())
}

// --------------------------
//
// BALANCES
// Map addresses to eligible withdrawal amounts
//
// --------------------------
pub const BALANCES: Map<String, Uint128> = Map::new("balances");

pub fn set_balance(store: &mut dyn Storage, api: &dyn Api, address: String, amount: Uint128) -> StdResult<()> {
    api.addr_validate(&address)?;
    BALANCES.save(store, address, &amount)?;
    Ok(())
}

pub fn set_balances(store: &mut dyn Storage, api: &dyn Api, balances: Vec<(String, Uint128)>) -> StdResult<()> {
    for (address, amount) in balances {
        set_balance(store, api, address, amount)?;
    }
    Ok(())
}

pub fn add_balance(store: &mut dyn Storage, api: &dyn Api, address: String, amount: Uint128) -> StdResult<()> {
    api.addr_validate(&address)?;
    let balance = match BALANCES.may_load(store, address.clone())? {
        Some(balance) => balance.checked_add(amount)?,
        None => amount,
    };
    BALANCES.save(store, address, &balance)?;
    Ok(())
}

pub fn reduce_balance(store: &mut dyn Storage, api: &dyn Api, address: String, amount: Uint128) -> StdResult<()> {
    api.addr_validate(&address)?;
    let balance = match BALANCES.may_load(store, address.clone())? {
        Some(balance) => balance.checked_sub(amount)?,
        None => return Err(StdError::generic_err("balance not found")),
    };
    BALANCES.save(store, address, &balance)?;
    Ok(())
}

pub fn get_max_balance_account(store: &dyn Storage) -> StdResult<String> {
    let mut max_balance = Uint128::zero();
    let mut max_address = String::new();

    BALANCES
        .range(store, None, None, Order::Descending)
        .for_each(|item| {
            if let Ok((key, balance)) = item {
                if balance > max_balance {
                    max_balance = balance;
                    max_address = key;
                }
            }
        });

    Ok(max_address)
}

pub fn get_balance(store: &dyn Storage, address: String) -> StdResult<Uint128> {
    Ok(BALANCES.load(store, address)?)
}

pub fn sum_balances(store: &dyn Storage) -> StdResult<Uint128> {
    let sum: Uint128 = BALANCES
        .range(store, None, None, Order::Ascending)
        .filter_map(|item| {
            if let Ok((_, balance)) = item {
                Some(balance)
            } else {
                None
            }
        })
        .sum();

    Ok(sum)
}

pub fn get_balances(store: &dyn Storage) -> StdResult<Vec<(String, Uint128)>> {
    let res: Vec<(String, Uint128)> = BALANCES
        .range(store, None, None, Order::Ascending)
        .filter_map(|item| {
            if let Ok((key, balance)) = item {
                Some((key, balance))
            } else {
                None
            }
        })
        .collect();

    Ok(res)
}

// --------------------------
//
// CLAIMED
// Holds the total amount of tokens that have been withdrawn by each address
//
// --------------------------
pub const CLAIMED: Map<String, Uint128> = Map::new("withdrawn");

pub fn set_claimed(store: &mut dyn Storage, api: &dyn Api, address: String, amount: Uint128) -> StdResult<()> {
    api.addr_validate(&address)?;
    CLAIMED.save(store, address, &amount)?;
    Ok(())
}

pub fn get_claimed(store: &dyn Storage, address: String) -> StdResult<Uint128> {
    Ok(CLAIMED.load(store, address)?)
}

pub fn add_claimed(store: &mut dyn Storage, api: &dyn Api, address: String, amount: Uint128) -> StdResult<()> {
    api.addr_validate(&address)?;
    let claimed = match CLAIMED.may_load(store, address.clone())? {
        Some(claimed) => claimed.checked_add(amount)?,
        None => amount,
    };
    CLAIMED.save(store, address, &claimed)?;
    Ok(())
}

pub fn get_total_claimed(store: &dyn Storage) -> StdResult<Uint128> {
    let sum = CLAIMED
        .range(store, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, s| {
            let item = s?.1;
            let result = match acc.checked_add(item) {
                Ok(result) => Ok(result),
                Err(_) => Err(StdError::GenericErr {
                    msg: "overflow error".to_string(),
                }),
            };
            result
        })?;
    Ok(sum)
}

// --------------------------
//
// WEIGHTS
// Map addresses to eligible weights (must sum up to 1)
//
// --------------------------
pub const WEIGHTS: Map<String, Decimal> = Map::new("weights");

pub fn set_weights(store: &mut dyn Storage, api: &dyn Api, weights: Vec<(String, Decimal)>) -> StdResult<()> {
    validate_weights(weights.clone())?;
    for (address, weight) in weights {
        api.addr_validate(&address)?;
        WEIGHTS.save(store, address, &weight)?;
    }
    Ok(())
}

pub fn get_weights(store: &dyn Storage) -> StdResult<Vec<(String, Decimal)>> {
    let mut res: Vec<(String, Decimal)> = vec![];
    let res = WEIGHTS
        .range(store, None, None, Order::Ascending)
        .filter_map(|item| {
            if let Ok((key, weight)) = item {
                Some((key, weight))
            } else {
                None
            }
        })
        .collect();
    Ok(res)
}

pub fn get_weight(store: &dyn Storage, address: String) -> StdResult<Decimal> {
    Ok(WEIGHTS.load(store, address)?)
}

pub fn validate_weights(weights: Vec<(String, Decimal)>) -> StdResult<()> {
    let sum: Decimal = weights.iter().map(|(_, w)| w).sum();
    if sum != Decimal::one() {
        return Err(StdError::generic_err("weights must sum up to 1"));
    }
    Ok(())
}

#[cfg(test)]
mod test {

    use super::{get_admin, sum_balances};
    use crate::msg::InstantiateMsg;
    use crate::test_util::mock_contract;
    use crate::test_util::{get_mocked_balance, wasm_query_handler};
    use cosmwasm_schema::Api;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::StdError::Overflow;
    use cosmwasm_std::{Addr, Coin, Decimal, Querier, StdError, Uint128};
    use cw_denom::CheckedDenom;
    use std::borrow::Borrow;
    use std::env;

    use super::{get_current_balance, set_balances, set_managed_denom};
    use cosmwasm_std::{
        OverflowError,
        OverflowOperation::{Add, Sub},
    };

    #[test]
    fn assert_admin_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let store = deps.storage;
        let api = deps.api;
        let admin = "addr0000".to_string();
        super::set_admin(store, api, Some(admin.clone())).unwrap();

        // must succeed
        super::assert_admin(store, admin.clone()).unwrap();

        // must fail
        let other = "addr0001".to_string();
        super::assert_admin(store, other.clone()).unwrap_err();
    }

    #[test]
    fn set_admin_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let mut store = deps.storage;
        let api = deps.api;
        let admin = "addr0000".to_string();

        super::set_admin(store, api, Some(admin.clone())).unwrap();
        assert_eq!(get_admin(store).unwrap().unwrap(), admin);

        super::set_admin(store, api, None).unwrap();
        assert_eq!(
            get_admin(store).unwrap().unwrap(),
            String::from("")
        );
    }

    #[test]
    fn get_balance_works() {
        // mock the querier
        let mut owned_deps = mock_dependencies();
        owned_deps.querier.update_wasm(|r| wasm_query_handler(r));
        owned_deps.querier.update_balance(
            "contract".to_string(),
            vec![Coin::new(
                get_mocked_balance("contract".to_string()).into(),
                "uusd",
            )],
        );
        let mut deps = owned_deps.as_mut();
        let api = deps.api;
        let querier = deps.querier;
        let store = deps.storage;
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");

        // native balance works
        let native_denom = CheckedDenom::Native("uusd".to_string());
        set_managed_denom(store, native_denom).unwrap();

        let balance = get_current_balance(store, querier, env.clone()).unwrap();
        assert_eq!(balance, get_mocked_balance(String::from("contract")));

        // cw20 balance works as well
        let cw20_denom = CheckedDenom::Cw20(Addr::unchecked("booh"));
        set_managed_denom(store, cw20_denom).unwrap();

        let balance = get_current_balance(store, querier, env.clone()).unwrap();
        assert_eq!(balance, get_mocked_balance(String::from("contract")));
    }

    #[test]
    fn sum_balances_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let api = deps.api;
        let mut store = deps.storage;
        let balances = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        set_balances(store, api, balances).unwrap();
        let sum = sum_balances(store).unwrap();
        assert_eq!(sum, Uint128::new(600_000_001));
    }

    #[test]
    fn get_max_balance_account_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let mut store = deps.storage;
        let api = deps.api;
        let balances = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0003".to_string(), Uint128::new(300_000_001)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        set_balances(store, api, balances).unwrap();
        let max_address = super::get_max_balance_account(store).unwrap();

        // the last address has the highest balance
        // in case of equal balance sort by alphabetical
        // order
        assert_eq!(max_address, "addr0003");
    }

    #[test]
    fn get_total_claimed_works() {
        let mut owned_deps = mock_dependencies();
        let deps = owned_deps.as_mut();
        let mut store = deps.storage;
        let api = deps.api;
        let claimed = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        for (address, amount) in claimed {
            super::set_claimed(store, api, address, amount).unwrap();
        }
        let total_claimed = super::get_total_claimed(store).unwrap();
        assert_eq!(total_claimed, Uint128::new(600_000_001));
    }

    #[test]
    fn set_managed_denom_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let native_denom = CheckedDenom::Native("uusd".to_string());
        let cw20_denom = CheckedDenom::Cw20(Addr::unchecked("booh"));

        super::set_managed_denom(store, native_denom.clone()).unwrap();
        let denom = super::get_managed_denom(store).unwrap();
        assert_eq!(denom, native_denom);

        super::set_managed_denom(store, cw20_denom.clone()).unwrap();
        let denom = super::get_managed_denom(store).unwrap();
        assert_eq!(denom, cw20_denom);
    }

    #[test]
    fn get_current_balance_works() {
        // native balance works
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
        let mocked = mock_contract(msg).unwrap();
        let deps = mocked.0.as_ref();
        let api = deps.api;
        let store = deps.storage;
        let querier = deps.querier;
        let balance = get_current_balance(store, querier, mocked.1).unwrap();
        assert_eq!(get_mocked_balance("contract".to_string()), balance);

        // cw20 balance works as well
        let msg = InstantiateMsg {
            admin: None,
            managed_denom: cw_denom::CheckedDenom::Cw20(Addr::unchecked("booh")),
            weights: vec![
                ("addr0000".to_string(), Decimal::percent(10)),
                ("addr0001".to_string(), Decimal::percent(20)),
                ("addr0002".to_string(), Decimal::percent(30)),
                ("addr0003".to_string(), Decimal::percent(40)),
            ],
        };
        let mocked = mock_contract(msg).unwrap();
        let deps = mocked.0.as_ref();
        let store = deps.storage;
        let querier = deps.querier;
        let balance = get_current_balance(store, querier, mocked.1).unwrap();
        assert_eq!(get_mocked_balance("contract".to_string()), balance);
    }

    #[test]
    fn set_managed_balance_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let amount = Uint128::new(100_000_000);
        super::set_managed_balance(store, amount).unwrap();
        let managed_balance = super::get_managed_balance(store).unwrap();
        assert_eq!(managed_balance, amount);
    }

    #[test]
    fn reduce_managed_balance() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let amount = Uint128::new(100_000_000);
        super::set_managed_balance(store, amount).unwrap();
        super::reduce_managed_balance(store, Uint128::new(10_000_000)).unwrap();
        let managed_balance = super::get_managed_balance(store).unwrap();
        assert_eq!(managed_balance, Uint128::new(90_000_000));
    }

    #[test]
    fn set_balance_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let api = deps.api;
        let mut store = deps.storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_balance(store, api, address.clone(), amount).unwrap();
        let balance = super::get_balance(store, address.clone()).unwrap();
        assert_eq!(balance, amount);
    }

    #[test]
    fn reduce_balance_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let api = deps.api;
        let mut store = deps.storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_balance(store, api, address.clone(), amount).unwrap();

        // reduce balance works
        super::reduce_balance(store, api, address.clone(), Uint128::new(10_000_000)).unwrap();
        let balance = super::get_balance(store, address.clone()).unwrap();
        assert_eq!(balance, Uint128::new(90_000_000));

        // reduce balance fails on overflow
        let err =
            super::reduce_balance(store, api, address.clone(), Uint128::new(110_000_000)).unwrap_err();
        assert_eq!(
            err,
            Overflow {
                source: OverflowError {
                    operation: Sub,
                    operand1: String::from("90000000"),
                    operand2: String::from("110000000")
                }
            }
        );

        // reduce fails on nonexistent balance
        let err = super::reduce_balance(store, api, "addr0001".to_string(), Uint128::new(10_000_000))
            .unwrap_err();
        assert_eq!(err, StdError::generic_err("balance not found"));
    }

    #[test]
    fn add_balance_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let mut store = deps.storage;
        let api = deps.api;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_balance(store, api, address.clone(), amount).unwrap();

        // add balance works
        super::add_balance(store, api, address.clone(), Uint128::new(10_000_000)).unwrap();
        let balance = super::get_balance(store, address.clone()).unwrap();
        assert_eq!(balance, Uint128::new(110_000_000));

        // add balance fails on overflow
        let err = super::add_balance(store, api, address.clone(), Uint128::MAX).unwrap_err();
        assert_eq!(
            err,
            Overflow {
                source: OverflowError {
                    operation: Add,
                    operand1: String::from("110000000"),
                    operand2: Uint128::MAX.to_string()
                }
            }
        );

        // add balance works on nonexistent balance
        super::add_balance(store, api, "addr0001".to_string(), Uint128::new(10_000_000)).unwrap();
        let balance = super::get_balance(store, "addr0001".to_string()).unwrap();
        assert_eq!(balance, Uint128::new(10_000_000));
    }

    #[test]
    fn set_claimed_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let mut store = deps.storage;
        let api = deps.api;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);

        // set claim works
        super::set_claimed(store, api, address.clone(), amount).unwrap();
        let claimed = super::get_claimed(store, address.clone()).unwrap();
        assert_eq!(claimed, amount);
    }

    #[test]
    fn add_claim_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let api = deps.api;
        let mut store = deps.storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_claimed(store, api, address.clone(), amount).unwrap();

        // add claim works
        super::add_claimed(store, api, address.clone(), Uint128::new(10_000_000)).unwrap();
        let claimed = super::get_claimed(store, address.clone()).unwrap();
        assert_eq!(claimed, Uint128::new(110_000_000));

        // add claim works on nonexistent claim
        super::add_claimed(store, api, "addr0001".to_string(), Uint128::new(10_000_000)).unwrap();
        let claimed = super::get_claimed(store, "addr0001".to_string()).unwrap();
        assert_eq!(claimed, Uint128::new(10_000_000));

        // add claim fails on overflow
        let err = super::add_claimed(store, api, address.clone(), Uint128::MAX).unwrap_err();
        assert_eq!(
            err,
            Overflow {
                source: OverflowError {
                    operation: Add,
                    operand1: String::from("110000000"),
                    operand2: Uint128::MAX.to_string()
                }
            }
        );
    }

    #[test]
    fn sum_claimed_works() {
        let mut owned_deps = mock_dependencies();
        let mut deps = owned_deps.as_mut();
        let api = deps.api;
        let mut store = deps.storage;
        let claimed = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        for (address, amount) in claimed {
            super::set_claimed(store, api, address, amount).unwrap();
        }
        let total_claimed = super::get_total_claimed(store).unwrap();
        assert_eq!(total_claimed, Uint128::new(600_000_001));
    }

    #[test]
    fn set_weights_works() {
        let mut owned_depsdeps = mock_dependencies();
        let mut deps = owned_depsdeps.as_mut();
        let api = deps.api;
        let mut store = deps.storage;
        let weights = vec![
            ("addr0000".to_string(), Decimal::percent(10)),
            ("addr0001".to_string(), Decimal::percent(20)),
            ("addr0002".to_string(), Decimal::percent(30)),
            ("addr0003".to_string(), Decimal::percent(40)),
        ];
        super::set_weights(store, api, weights.clone()).unwrap();

        let weight = super::get_weight(store, "addr0000".to_string()).unwrap();
        assert_eq!(weight, Decimal::percent(10));
        let weight = super::get_weight(store, "addr0001".to_string()).unwrap();
        assert_eq!(weight, Decimal::percent(20));
        let weight = super::get_weight(store, "addr0002".to_string()).unwrap();
        assert_eq!(weight, Decimal::percent(30));
        let weight = super::get_weight(store, "addr0003".to_string()).unwrap();
        assert_eq!(weight, Decimal::percent(40));
    }

    #[test]
    fn validate_weights_works() {
        let weights = vec![
            ("addr0000".to_string(), Decimal::percent(10)),
            ("addr0001".to_string(), Decimal::percent(20)),
            ("addr0002".to_string(), Decimal::percent(30)),
            ("addr0003".to_string(), Decimal::percent(40)),
        ];
        super::validate_weights(weights.clone()).unwrap();

        let weights = vec![
            ("addr0000".to_string(), Decimal::percent(10)),
            ("addr0001".to_string(), Decimal::percent(20)),
            ("addr0002".to_string(), Decimal::percent(30)),
            ("addr0003".to_string(), Decimal::percent(50)),
        ];
        let err = super::validate_weights(weights.clone()).unwrap_err();
        assert_eq!(err, StdError::generic_err("weights must sum up to 1"));
    }
}

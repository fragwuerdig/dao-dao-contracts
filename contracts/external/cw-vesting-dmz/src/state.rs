use cosmwasm_std::{
    Decimal, DecimalRangeExceeded, Deps, DepsMut, Env, Order, StdError, StdResult, Storage, Uint128,
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

pub fn validate_admin(deps: Deps, address: Option<String>) -> StdResult<()> {
    match address {
        Some(address) => {
            deps.api.addr_validate(&address)?;
        }
        None => {}
    }
    Ok(())
}

pub fn set_admin(deps: DepsMut, address: Option<String>) -> StdResult<()> {
    match address {
        Some(address) => {
            ADMIN.save(deps.storage, &address)?;
        }
        None => {
            ADMIN.save(deps.storage, &"".to_string())?;
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

pub fn get_current_balance(deps: Deps, env: Env) -> StdResult<Uint128> {
    let denom = get_managed_denom(deps.storage)?;
    match denom {
        CheckedDenom::Native(denom) => {
            let balance = deps.querier.query_balance(&env.contract.address, denom)?;
            Ok(balance.amount)
        }
        CheckedDenom::Cw20(addr) => {
            let query_msg = cw20::Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            };
            let balance: cw20::BalanceResponse = deps.querier.query_wasm_smart(addr, &query_msg)?;
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

pub fn set_balance(store: &mut dyn Storage, address: String, amount: Uint128) -> StdResult<()> {
    BALANCES.save(store, address, &amount)?;
    Ok(())
}

pub fn set_balances(store: &mut dyn Storage, balances: Vec<(String, Uint128)>) -> StdResult<()> {
    for (address, amount) in balances {
        set_balance(store, address, amount)?;
    }
    Ok(())
}

pub fn add_balance(store: &mut dyn Storage, address: String, amount: Uint128) -> StdResult<()> {
    let balance = match BALANCES.may_load(store, address.clone())? {
        Some(balance) => balance.checked_add(amount)?,
        None => amount,
    };
    BALANCES.save(store, address, &balance)?;
    Ok(())
}

pub fn reduce_balance(store: &mut dyn Storage, address: String, amount: Uint128) -> StdResult<()> {
    let balance = match BALANCES.may_load(store, address.clone())? {
        Some(balance) => balance.checked_sub(amount)?,
        None => return Err(StdError::generic_err("balance not found")),
    };
    BALANCES.save(store, address, &balance)?;
    Ok(())
}

pub fn get_max_balance_account(store: &dyn Storage) -> StdResult<String> {
    let keys = BALANCES.keys(store, None, None, Order::Descending);
    let mut max_balance = Uint128::zero();
    let mut max_address = "".to_string();
    for key in keys {
        let unwrapped_key = match key {
            Ok(key) => key,
            Err(_) => return Err(StdError::generic_err("key not found")),
        };
        let balance = BALANCES.load(store, unwrapped_key.clone())?;
        if balance > max_balance {
            max_balance = balance;
            max_address = unwrapped_key;
        }
    }
    Ok(max_address)
}

pub fn get_balance(store: &dyn Storage, address: String) -> StdResult<Uint128> {
    Ok(BALANCES.load(store, address)?)
}

pub fn sum_balances(store: &dyn Storage) -> StdResult<Uint128> {
    let keys = BALANCES.keys(store, None, None, Order::Ascending);
    let mut sum = Uint128::zero();
    for key in keys {
        let unwrapped_key = match key {
            Ok(key) => key,
            Err(_) => return Err(StdError::generic_err("key not found")),
        };
        let balance = BALANCES.load(store, unwrapped_key.clone())?;
        sum += balance;
    }
    Ok(sum)
}

pub fn get_balances(store: &dyn Storage) -> StdResult<Vec<(String, Uint128)>> {
    let mut res: Vec<(String, Uint128)> = vec![];
    let keys = BALANCES.keys(store, None, None, Order::Ascending);
    for key in keys {
        let unwrapped_key = match key {
            Ok(key) => key,
            Err(_) => return Err(StdError::generic_err("key not found")),
        };
        let balance = BALANCES.load(store, unwrapped_key.clone())?;
        res.push((unwrapped_key, balance));
    }
    Ok(res)
}

// --------------------------
//
// CLAIMED
// Holds the total amount of tokens that have been withdrawn by each address
//
// --------------------------
pub const CLAIMED: Map<String, Uint128> = Map::new("withdrawn");

pub fn set_claimed(store: &mut dyn Storage, address: String, amount: Uint128) -> StdResult<()> {
    CLAIMED.save(store, address, &amount)?;
    Ok(())
}

pub fn get_claimed(store: &dyn Storage, address: String) -> StdResult<Uint128> {
    Ok(CLAIMED.load(store, address)?)
}

pub fn add_claimed(store: &mut dyn Storage, address: String, amount: Uint128) -> StdResult<()> {
    let claimed = match CLAIMED.may_load(store, address.clone())? {
        Some(claimed) => claimed.checked_add(amount)?,
        None => amount,
    };
    CLAIMED.save(store, address, &claimed)?;
    Ok(())
}

pub fn get_total_claimed(store: &dyn Storage) -> StdResult<Uint128> {
    let keys = CLAIMED.keys(store, None, None, Order::Ascending);
    let mut sum = Uint128::zero();
    for key in keys {
        let unwrapped_key = match key {
            Ok(key) => key,
            Err(_) => return Err(StdError::generic_err("key not found")),
        };
        let claimed = CLAIMED.load(store, unwrapped_key.clone())?;
        sum += claimed;
    }
    Ok(sum)
}

// --------------------------
//
// WEIGHTS
// Map addresses to eligible weights (must sum up to 1)
//
// --------------------------
pub const WEIGHTS: Map<String, Decimal> = Map::new("weights");

pub fn set_weights(store: &mut dyn Storage, weights: Vec<(String, Decimal)>) -> StdResult<()> {
    for (address, weight) in weights {
        WEIGHTS.save(store, address, &weight)?;
    }
    Ok(())
}

pub fn get_weights(store: &dyn Storage) -> StdResult<Vec<(String, Decimal)>> {
    let mut res: Vec<(String, Decimal)> = vec![];
    let keys = WEIGHTS.keys(store, None, None, Order::Descending);
    for key in keys {
        let unwrapped_key = match key {
            Ok(key) => key,
            Err(_) => return Err(StdError::generic_err("key not found")),
        };
        let weight = WEIGHTS.load(store, unwrapped_key.clone())?;
        res.push((unwrapped_key, weight));
    }
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
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::StdError::Overflow;
    use cosmwasm_std::{Addr, Coin, Decimal, StdError, Uint128};
    use cw_denom::CheckedDenom;
    use std::env;

    use super::{get_current_balance, set_balances, set_managed_denom};
    use cosmwasm_std::{
        OverflowError,
        OverflowOperation::{Add, Sub},
    };

    #[test]
    fn assert_admin_works() {
        let mut deps = mock_dependencies();
        let admin = "addr0000".to_string();
        super::set_admin(deps.as_mut(), Some(admin.clone())).unwrap();
        let store = deps.as_mut().storage;

        // must succeed
        super::assert_admin(store, admin.clone()).unwrap();

        // must fail
        let other = "addr0001".to_string();
        super::assert_admin(store, other.clone()).unwrap_err();
    }

    #[test]
    fn set_admin_works() {
        let mut deps = mock_dependencies();
        let admin = "addr0000".to_string();

        super::set_admin(deps.as_mut(), Some(admin.clone())).unwrap();
        assert_eq!(get_admin(deps.as_mut().storage).unwrap().unwrap(), admin);

        super::set_admin(deps.as_mut(), None).unwrap();
        assert_eq!(
            get_admin(deps.as_mut().storage).unwrap().unwrap(),
            String::from("")
        );
    }

    #[test]
    fn get_balance_works() {
        // mock the querier
        let mut deps = mock_dependencies();
        deps.querier.update_wasm(|r| wasm_query_handler(r));
        deps.querier.update_balance(
            "contract".to_string(),
            vec![Coin::new(
                get_mocked_balance("contract".to_string()).into(),
                "uusd",
            )],
        );
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");

        // native balance works
        let native_denom = CheckedDenom::Native("uusd".to_string());
        set_managed_denom(deps.as_mut().storage, native_denom).unwrap();

        let balance = get_current_balance(deps.as_ref(), env.clone()).unwrap();
        assert_eq!(balance, get_mocked_balance(String::from("contract")));

        // cw20 balance works as well
        let cw20_denom = CheckedDenom::Cw20(Addr::unchecked("booh"));
        set_managed_denom(deps.as_mut().storage, cw20_denom).unwrap();

        let balance = get_current_balance(deps.as_ref(), env.clone()).unwrap();
        assert_eq!(balance, get_mocked_balance(String::from("contract")));
    }

    #[test]
    fn sum_balances_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let balances = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        set_balances(store, balances).unwrap();
        let sum = sum_balances(store).unwrap();
        assert_eq!(sum, Uint128::new(600_000_001));
    }

    #[test]
    fn get_max_balance_account_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let balances = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0003".to_string(), Uint128::new(300_000_001)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        set_balances(store, balances).unwrap();
        let max_address = super::get_max_balance_account(store).unwrap();

        // the last address has the highest balance
        // in case of equal balance sort by alphabetical
        // order
        assert_eq!(max_address, "addr0003");
    }

    #[test]
    fn get_total_claimed_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let claimed = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        for (address, amount) in claimed {
            super::set_claimed(store, address, amount).unwrap();
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
        let balance = get_current_balance(mocked.0.as_ref(), mocked.1).unwrap();
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
        let balance = get_current_balance(mocked.0.as_ref(), mocked.1).unwrap();
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
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_balance(store, address.clone(), amount).unwrap();
        let balance = super::get_balance(store, address.clone()).unwrap();
        assert_eq!(balance, amount);
    }

    #[test]
    fn reduce_balance_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_balance(store, address.clone(), amount).unwrap();

        // reduce balance works
        super::reduce_balance(store, address.clone(), Uint128::new(10_000_000)).unwrap();
        let balance = super::get_balance(store, address.clone()).unwrap();
        assert_eq!(balance, Uint128::new(90_000_000));

        // reduce balance fails on overflow
        let err =
            super::reduce_balance(store, address.clone(), Uint128::new(110_000_000)).unwrap_err();
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
        let err = super::reduce_balance(store, "addr0001".to_string(), Uint128::new(10_000_000))
            .unwrap_err();
        assert_eq!(err, StdError::generic_err("balance not found"));
    }

    #[test]
    fn add_balance_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_balance(store, address.clone(), amount).unwrap();

        // add balance works
        super::add_balance(store, address.clone(), Uint128::new(10_000_000)).unwrap();
        let balance = super::get_balance(store, address.clone()).unwrap();
        assert_eq!(balance, Uint128::new(110_000_000));

        // add balance fails on overflow
        let err = super::add_balance(store, address.clone(), Uint128::MAX).unwrap_err();
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
        super::add_balance(store, "addr0001".to_string(), Uint128::new(10_000_000)).unwrap();
        let balance = super::get_balance(store, "addr0001".to_string()).unwrap();
        assert_eq!(balance, Uint128::new(10_000_000));
    }

    #[test]
    fn set_claimed_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);

        // set claim works
        super::set_claimed(store, address.clone(), amount).unwrap();
        let claimed = super::get_claimed(store, address.clone()).unwrap();
        assert_eq!(claimed, amount);
    }

    #[test]
    fn add_claim_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let address = "addr0000".to_string();
        let amount = Uint128::new(100_000_000);
        super::set_claimed(store, address.clone(), amount).unwrap();

        // add claim works
        super::add_claimed(store, address.clone(), Uint128::new(10_000_000)).unwrap();
        let claimed = super::get_claimed(store, address.clone()).unwrap();
        assert_eq!(claimed, Uint128::new(110_000_000));

        // add claim works on nonexistent claim
        super::add_claimed(store, "addr0001".to_string(), Uint128::new(10_000_000)).unwrap();
        let claimed = super::get_claimed(store, "addr0001".to_string()).unwrap();
        assert_eq!(claimed, Uint128::new(10_000_000));

        // add claim fails on overflow
        let err = super::add_claimed(store, address.clone(), Uint128::MAX).unwrap_err();
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
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let claimed = vec![
            ("addr0000".to_string(), Uint128::new(100_000_000)),
            ("addr0001".to_string(), Uint128::new(200_000_000)),
            ("addr0002".to_string(), Uint128::new(300_000_001)),
        ];
        for (address, amount) in claimed {
            super::set_claimed(store, address, amount).unwrap();
        }
        let total_claimed = super::get_total_claimed(store).unwrap();
        assert_eq!(total_claimed, Uint128::new(600_000_001));
    }

    #[test]
    fn set_weights_works() {
        let mut deps = mock_dependencies();
        let mut store = deps.as_mut().storage;
        let weights = vec![
            ("addr0000".to_string(), Decimal::percent(10)),
            ("addr0001".to_string(), Decimal::percent(20)),
            ("addr0002".to_string(), Decimal::percent(30)),
            ("addr0003".to_string(), Decimal::percent(40)),
        ];
        super::set_weights(store, weights.clone()).unwrap();
        
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

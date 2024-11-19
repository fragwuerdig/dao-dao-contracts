use crate::contract::instantiate;
use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use cosmwasm_std::Env;
use cosmwasm_std::{
    from_json, to_json_binary, Addr, BankQuery, ContractResult, DepsMut, Empty, MemoryStorage,
    OwnedDeps, QuerierResult, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg};

#[cfg(test)]
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};

#[cfg(test)]
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};

#[cfg(test)]
const MOCK_BALANCES: [(&str, Uint128); 4] = [
    ("addr0000", Uint128::new(100_000_000)),
    ("addr0001", Uint128::new(200_000_000)),
    ("addr0002", Uint128::new(300_000_000)),
    ("contract", Uint128::new(444_000_000)),
];

#[cfg(test)]
pub fn get_mocked_balance(addr: String) -> Uint128 {
    MOCK_BALANCES
        .iter()
        .find(|(a, _)| a == &addr)
        .unwrap_or(&(&addr, Uint128::zero()))
        .1
}

#[cfg(test)]
pub fn wasm_query_handler(request: &WasmQuery) -> QuerierResult {
    match request {
        WasmQuery::Smart { contract_addr, msg } => {
            let cw20_msg: Cw20QueryMsg = from_json(msg).unwrap();
            match cw20_msg {
                Cw20QueryMsg::Balance { address } => {
                    let addr = Addr::unchecked(address.as_str());
                    let resp = BalanceResponse {
                        balance: get_mocked_balance(addr.to_string()),
                    };
                    return QuerierResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()));
                }
                _ => panic!("Unsupported wasm cw20 query type in testing env"),
            };
        }
        _ => panic!("Unsupported wasm query type in testing env"),
    }
}

#[cfg(test)]
pub fn set_mocked_cw20_balance(deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier, Empty>) {
    deps.querier.update_wasm(|r| wasm_query_handler(r));
}

#[cfg(test)]
pub fn set_mocked_native_balance(deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier, Empty>) {
    use cosmwasm_std::Coin;

    MOCK_BALANCES.iter().for_each(|(addr, balance)| {
        deps.querier
            .update_balance(addr.to_string(), vec![Coin::new(balance.u128(), "uusd")]);
    });
}

#[cfg(test)]
pub fn mock_contract(
    msg: InstantiateMsg,
) -> Result<(OwnedDeps<MemoryStorage, MockApi, MockQuerier>, Env), ContractError> {
    let mut deps = mock_dependencies();
    let mut env = mock_env();
    env.contract.address = Addr::unchecked("contract");
    let info = mock_info("admin", &[]);
    set_mocked_cw20_balance(&mut deps);
    set_mocked_native_balance(&mut deps);
    instantiate(deps.as_mut(), env.clone(), info, msg)?;
    return Ok((deps, env));
}

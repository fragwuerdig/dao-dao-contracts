use cosmwasm_std::{Decimal, StdError, StdResult, Uint128};

pub fn round_dec_closest(n: Decimal) -> StdResult<Uint128> {
    let added = match n.checked_add(Decimal::percent(50)) {
        Ok(added) => added,
        Err(_) => return Err(StdError::generic_err("overflow")),
    };
    Ok(added.floor().to_uint_floor())
}

pub fn split_number_with_weights(
    amount: Uint128,
    weights: Vec<(String, Decimal)>,
) -> StdResult<Vec<(String, Uint128)>> {
    let dec_amount = match Decimal::from_atomics(amount, 0) {
        Ok(dec) => dec,
        Err(_) => return Err(StdError::generic_err("amount is too large")),
    };
    weights
        .iter()
        .map(|(address, weight)| {
            let share = match weight.checked_mul(dec_amount) {
                Ok(share) => share,
                Err(_) => return Err(StdError::generic_err("amount is too large")),
            };
            let rounded = match round_dec_closest(share) {
                Ok(rounded) => rounded,
                Err(_) => return Err(StdError::generic_err("rounding error")),
            };
            return Ok((address.clone(), rounded));
        })
        .collect()
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::error::ContractError;
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info},
        Addr, Coin, Decimal, Response,
    };

    #[test]
    fn test_round_dec_closest() {
        let n = Decimal::percent(50);
        let rounded = round_dec_closest(n).unwrap();
        assert_eq!(rounded, Uint128::new(1));
    }

    #[test]
    fn test_split_number_with_weights() {
        let amount = Uint128::new(100);
        let weights = vec![
            (String::from("addr1"), Decimal::percent(50)),
            (String::from("addr2"), Decimal::percent(50)),
        ];
        let shares = split_number_with_weights(amount, weights).unwrap();
    }
}

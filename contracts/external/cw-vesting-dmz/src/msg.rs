use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal, Uint128};
use cw_denom::CheckedDenom;

#[cw_serde]
pub enum ExecuteMsg {
    // Set Admin (admin only)
    SetAdmin { admin: String },

    // Unlock Tokens (admin only)
    UpdateClaims {},

    // Withdraw unlocked tokens (any user)
    Claim {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Option<String>)]
    Admin {},

    #[returns(QueryPendingClaimResponse)]
    PendingClaim { address: String },

    #[returns(QueryPendingClaimsResponse)]
    PendingClaims {},

    #[returns(Uint128)]
    Claimed { address: String },

    #[returns(Uint128)]
    TotalClaimed {},

    #[returns(QueryManagedDenomResponse)]
    Denom {},

    #[returns(QueryWeightsResponse)]
    Weights {},
}

#[cw_serde]
pub struct QueryPendingClaimResponse {
    pub address: String,
    pub amount: Uint128,
}

#[cw_serde]
pub struct QueryPendingClaimsResponse {
    pub claims: Vec<QueryPendingClaimResponse>,
    pub total: Uint128,
}

#[cw_serde]
pub struct QueryManagedDenomResponse {
    pub managed_denom: CheckedDenom,
    pub amount: Uint128,
}

#[cw_serde]
pub struct QueryWeightsResponse {
    pub weights: Vec<(String, Decimal)>,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub managed_denom: CheckedDenom,
    pub weights: Vec<(String, Decimal)>,
    pub admin: Option<String>,
}

#[cw_serde]
pub struct MigrateMsg {
    // if set - migrate to new weights if nothing
    // has been claimed yet
    pub weights: Option<Vec<(String, Decimal)>>,
}
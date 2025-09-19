use crate::schema;
use bigdecimal::BigDecimal;

use chrono::NaiveDateTime;

use diesel::prelude::{AsChangeset, Insertable, Queryable};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::lending_markets)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Market {
    pub id: String,
    #[diesel(column_name = marketcontractid)]
    pub market_contract_id: String,
    pub collateral_token: String,
    pub loan_token: String,
    pub oracle: String,
    pub irm: String,
    #[schema(value_type = String)]
    pub ltv: BigDecimal,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

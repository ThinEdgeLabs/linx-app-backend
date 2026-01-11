use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Queryable, Selectable, Debug, Clone, Serialize, Deserialize, ToSchema)]
#[diesel(table_name = crate::schema::linx_transactions)]
pub struct LinxTransaction {
    pub id: i64,
    pub tx_id: String,
    pub user_address: String,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::linx_transactions)]
pub struct NewLinxTransaction {
    pub tx_id: String,
    pub user_address: String,
}

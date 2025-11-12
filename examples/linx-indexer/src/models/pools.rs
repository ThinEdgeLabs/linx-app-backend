use diesel::prelude::{Insertable, Queryable};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema;

#[derive(Debug, Clone, Serialize, ToSchema, Queryable)]
#[diesel(table_name = schema::pools)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Pool {
    pub id: i64,
    pub address: String,
    pub token_a: String,
    pub token_b: String,
    pub factory_address: String,
}

#[derive(Insertable, Debug, Clone, PartialEq, Deserialize)]
#[diesel(table_name = schema::pools)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewPoolDto {
    pub token_a: String,
    pub token_b: String,
    pub factory_address: String,
    pub address: String,
}

impl From<NewPoolDto> for Pool {
    fn from(dto: NewPoolDto) -> Self {
        Pool {
            id: 0,
            token_a: dto.token_a,
            token_b: dto.token_b,
            factory_address: dto.factory_address,
            address: dto.address,
        }
    }
}

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use bento_core::DbPool;
use diesel_async::RunQueryDsl;

use crate::{
    models::{NewPoolDto, Pool},
    schema,
};

pub struct PoolRepository {
    db_pool: Arc<DbPool>,
}

impl PoolRepository {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    pub async fn insert_pools(&self, pools: &[NewPoolDto]) -> Result<()> {
        if pools.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;
        diesel::insert_into(schema::pools::table)
            .values(pools)
            .on_conflict(schema::pools::address)
            .do_nothing()
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn get_pools(&self) -> Result<HashMap<String, Pool>> {
        let mut conn = self.db_pool.get().await?;

        let pools: Vec<Pool> = schema::pools::table.load(&mut conn).await?;
        Ok(pools.into_iter().map(|pool| (pool.address.clone(), pool)).collect())
    }
}

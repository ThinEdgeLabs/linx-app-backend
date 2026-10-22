use crate::models::{LinxTransaction, NewLinxTransaction};
use crate::schema::linx_transactions;
use anyhow::Result;
use bento_types::DbPool;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::sync::Arc;

pub struct LinxTransactionsRepository {
    db_pool: Arc<DbPool>,
}

impl LinxTransactionsRepository {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    /// Insert a new Linx transaction record
    pub async fn insert_linx_transaction(&self, new_transaction: NewLinxTransaction) -> Result<LinxTransaction> {
        let mut conn = self.db_pool.get().await?;

        let transaction = diesel::insert_into(linx_transactions::table)
            .values(&new_transaction)
            .returning(LinxTransaction::as_returning())
            .get_result(&mut conn)
            .await?;

        Ok(transaction)
    }

    /// Check if a transaction ID exists in linx_transactions
    pub async fn is_linx_transaction(&self, tx_id: &str) -> Result<bool> {
        let mut conn = self.db_pool.get().await?;

        let count: i64 =
            linx_transactions::table.filter(linx_transactions::tx_id.eq(tx_id)).count().get_result(&mut conn).await?;

        Ok(count > 0)
    }
}

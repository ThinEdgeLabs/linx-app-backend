use crate::models::{AccountTransaction, NewAccountTransaction, SwapDetails, SwapTransaction};
use anyhow::Result;
use async_trait::async_trait;
use bento_types::DbPool;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
#[cfg(test)]
use mockall::automock;
use std::sync::Arc;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait AccountTransactionRepositoryTrait {
    async fn get_linx_swaps_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<SwapTransaction>>;
}

pub struct AccountTransactionRepository {
    db_pool: Arc<DbPool>,
}

impl AccountTransactionRepository {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    /// Generic insert method for any transaction type
    /// Uses ON CONFLICT DO NOTHING for idempotency
    pub async fn insert_transactions(
        &self,
        transactions: &[NewAccountTransaction],
    ) -> Result<()> {
        if transactions.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        use crate::schema::account_transactions;

        for tx in transactions {
            let result = diesel::insert_into(account_transactions::table)
                .values(tx)
                .on_conflict(account_transactions::tx_key)
                .do_nothing()
                .execute(&mut conn)
                .await;

            if let Err(e) = result {
                tracing::debug!("Failed to insert transaction for tx_id {}: {}", tx.tx_id, e);
            }
        }

        Ok(())
    }

    /// Get all transactions for an address using cursor-based pagination
    /// Pass `None` for cursor to get the first page
    /// For subsequent pages, pass the timestamp of the last item from the previous page
    pub async fn get_account_transactions(
        &self,
        address: &str,
        limit: i64,
        cursor: Option<NaiveDateTime>,
    ) -> Result<Vec<AccountTransaction>> {
        let mut conn = self.db_pool.get().await?;

        use crate::schema::account_transactions;

        let mut query = account_transactions::table
            .filter(account_transactions::address.eq(address))
            .order_by(account_transactions::timestamp.desc())
            .limit(limit)
            .into_boxed();

        if let Some(cursor_timestamp) = cursor {
            query = query.filter(account_transactions::timestamp.lt(cursor_timestamp));
        }

        let txs = query.load(&mut conn).await?;

        Ok(txs)
    }

    /// Get transactions by type in a time period
    pub async fn get_transactions_by_type_in_period(
        &self,
        tx_type: &str,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<AccountTransaction>> {
        let mut conn = self.db_pool.get().await?;

        use crate::schema::account_transactions;

        let txs = account_transactions::table
            .filter(account_transactions::tx_type.eq(tx_type))
            .filter(account_transactions::timestamp.ge(start_time))
            .filter(account_transactions::timestamp.lt(end_time))
            .load(&mut conn)
            .await?;

        Ok(txs)
    }

    /// Get Linx swaps (swaps submitted through the Linx app) in a time period
    /// Only swaps that have a corresponding entry in linx_transactions table are returned
    pub async fn get_linx_swaps_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<SwapTransaction>> {
        let mut conn = self.db_pool.get().await?;

        use crate::schema::{account_transactions, linx_transactions};

        // Join account_transactions with linx_transactions to filter only Linx swaps
        let account_txs: Vec<AccountTransaction> = account_transactions::table
            .inner_join(linx_transactions::table.on(account_transactions::tx_id.eq(linx_transactions::tx_id)))
            .filter(account_transactions::tx_type.eq("swap"))
            .filter(account_transactions::timestamp.ge(start_time))
            .filter(account_transactions::timestamp.lt(end_time))
            .select(AccountTransaction::as_select())
            .load(&mut conn)
            .await?;

        // Deserialize JSONB details into SwapTransaction
        let mut swap_transactions = Vec::new();
        for account_tx in account_txs {
            // Deserialize the JSONB details field into SwapDetails
            if let Ok(swap_details) = serde_json::from_value::<SwapDetails>(account_tx.details.clone()) {
                swap_transactions.push(SwapTransaction {
                    address: account_tx.address.clone(),
                    tx_id: account_tx.tx_id.clone(),
                    token_in: swap_details.token_in,
                    token_out: swap_details.token_out,
                    amount_in: swap_details.amount_in,
                    amount_out: swap_details.amount_out,
                    pool_address: swap_details.pool_address,
                    hop_count: swap_details.hop_count,
                });
            } else {
                tracing::warn!("Failed to deserialize swap details for tx_id: {}", account_tx.tx_id);
            }
        }

        Ok(swap_transactions)
    }
}

#[async_trait]
impl AccountTransactionRepositoryTrait for AccountTransactionRepository {
    async fn get_linx_swaps_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<SwapTransaction>> {
        self.get_linx_swaps_in_period(start_time, end_time).await
    }
}

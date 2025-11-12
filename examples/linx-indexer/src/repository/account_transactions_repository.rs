use crate::models::{
    AccountTransaction, NewContractCallTransactionDto, NewSwapTransactionDto,
    NewTransferTransactionDto, SwapDetails, SwapTransactionDto,
};
use crate::models::{AccountTransactionDetails, TransferDetails, TransferTransactionDto};
use crate::schema::swaps;
use anyhow::Result;
use async_trait::async_trait;
use bento_types::DbPool;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl};
#[cfg(test)]
use mockall::automock;
use std::sync::Arc;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait AccountTransactionRepositoryTrait {
    async fn get_swaps_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<SwapTransactionDto>>;
}

pub struct AccountTransactionRepository {
    db_pool: Arc<DbPool>,
}

impl AccountTransactionRepository {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    pub async fn insert_transfers(&self, dtos: &Vec<NewTransferTransactionDto>) -> Result<()> {
        if dtos.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        for dto in dtos {
            let mut account_tx = dto.account_transaction.clone();
            account_tx.tx_type = "transfer".to_string();
            let transfer = dto.transfer.clone();

            let result = conn
                .transaction::<_, diesel::result::Error, _>(|conn| {
                    async move {
                        use crate::schema::{account_transactions, transfers};

                        // Insert account transaction
                        let inserted_account_tx: AccountTransaction =
                            diesel::insert_into(account_transactions::table)
                                .values(&account_tx)
                                .returning(AccountTransaction::as_returning())
                                .get_result(conn)
                                .await?;

                        // Insert transfer - this will fail if duplicate exists
                        diesel::insert_into(transfers::table)
                            .values((
                                transfers::account_transaction_id.eq(inserted_account_tx.id),
                                transfers::token_id.eq(&transfer.token_id),
                                transfers::from_address.eq(&transfer.from_address),
                                transfers::to_address.eq(&transfer.to_address),
                                transfers::amount.eq(&transfer.amount),
                                transfers::tx_id.eq(&inserted_account_tx.tx_id),
                            ))
                            .execute(conn)
                            .await?;

                        Ok(())
                    }
                    .scope_boxed()
                })
                .await;

            if let Err(e) = result {
                tracing::debug!(
                    "Failed to insert transfer for tx_id {}: {}",
                    dto.account_transaction.tx_id,
                    e
                );
            }
        }

        Ok(())
    }

    pub async fn insert_contract_calls(
        &self,
        dtos: &Vec<NewContractCallTransactionDto>,
    ) -> Result<()> {
        if dtos.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        for dto in dtos {
            let mut account_tx = dto.account_transaction.clone();
            account_tx.tx_type = "contract_call".to_string();
            let contract_call = dto.contract_call.clone();

            let result = conn
                .transaction::<_, diesel::result::Error, _>(|conn| {
                    async move {
                        use crate::schema::{account_transactions, contract_calls};

                        // Insert account transaction
                        let inserted_account_tx: AccountTransaction =
                            diesel::insert_into(account_transactions::table)
                                .values(&account_tx)
                                .returning(AccountTransaction::as_returning())
                                .get_result(conn)
                                .await?;

                        // Insert contract call - this will fail if duplicate exists
                        diesel::insert_into(contract_calls::table)
                            .values((
                                contract_calls::account_transaction_id.eq(inserted_account_tx.id),
                                contract_calls::contract_address
                                    .eq(&contract_call.contract_address),
                                contract_calls::tx_id.eq(&inserted_account_tx.tx_id),
                            ))
                            .execute(conn)
                            .await?;

                        Ok(())
                    }
                    .scope_boxed()
                })
                .await;

            if let Err(e) = result {
                tracing::debug!(
                    "Failed to insert contract call for tx_id {}: {}",
                    dto.account_transaction.tx_id,
                    e
                );
            }
        }

        Ok(())
    }

    pub async fn insert_swaps(&self, dtos: &Vec<NewSwapTransactionDto>) -> Result<()> {
        if dtos.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        for dto in dtos {
            let mut account_tx = dto.account_transaction.clone();
            account_tx.tx_type = "swap".to_string();
            let swap = dto.swap.clone();

            let result = conn
                .transaction::<_, diesel::result::Error, _>(|conn| {
                    async move {
                        use crate::schema::account_transactions;

                        // Insert account transaction
                        let inserted_account_tx: AccountTransaction =
                            diesel::insert_into(account_transactions::table)
                                .values(&account_tx)
                                .returning(AccountTransaction::as_returning())
                                .get_result(conn)
                                .await?;

                        // Insert swap - this will fail if duplicate exists
                        diesel::insert_into(swaps::table)
                            .values((
                                swaps::account_transaction_id.eq(inserted_account_tx.id),
                                swaps::token_in.eq(&swap.token_in),
                                swaps::token_out.eq(&swap.token_out),
                                swaps::amount_in.eq(&swap.amount_in),
                                swaps::amount_out.eq(&swap.amount_out),
                                swaps::pool_address.eq(&swap.pool_address),
                                swaps::tx_id.eq(&swap.tx_id),
                            ))
                            .execute(conn)
                            .await?;

                        Ok(())
                    }
                    .scope_boxed()
                })
                .await;

            if let Err(e) = result {
                tracing::debug!(
                    "Failed to insert swap for tx_id {}: {}",
                    dto.account_transaction.tx_id,
                    e
                );
            }
        }

        Ok(())
    }

    pub async fn get_account_transactions(
        &self,
        address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AccountTransactionDetails>> {
        let mut conn = self.db_pool.get().await?;

        use crate::schema::{account_transactions, transfers};

        let account_txs: Vec<AccountTransaction> = account_transactions::table
            .filter(account_transactions::address.eq(address))
            .order_by(account_transactions::timestamp.desc())
            .limit(limit)
            .offset(offset)
            .load(&mut conn)
            .await?;

        if account_txs.is_empty() {
            return Ok(Vec::new());
        }

        let tx_ids: Vec<i64> = account_txs.iter().map(|tx| tx.id).collect();

        let transfers_map: std::collections::HashMap<i64, TransferDetails> = transfers::table
            .filter(transfers::account_transaction_id.eq_any(&tx_ids))
            .load::<(i64, i64, String, String, String, bigdecimal::BigDecimal, String)>(&mut conn)
            .await?
            .into_iter()
            .map(|(id, account_tx_id, token_id, from_addr, to_addr, amount, _tx_id)| {
                (
                    account_tx_id,
                    TransferDetails {
                        id,
                        token_id,
                        from_address: from_addr,
                        to_address: to_addr,
                        amount,
                    },
                )
            })
            .collect();

        let swaps_map: std::collections::HashMap<i64, SwapDetails> = swaps::table
            .filter(swaps::account_transaction_id.eq_any(&tx_ids))
            .load::<(
                i64,
                i64,
                String,
                String,
                bigdecimal::BigDecimal,
                bigdecimal::BigDecimal,
                String,
                String,
            )>(&mut conn)
            .await?
            .into_iter()
            .map(
                |(
                    id,
                    account_tx_id,
                    token_in,
                    token_out,
                    amount_in,
                    amount_out,
                    pool_address,
                    tx_id,
                )| {
                    (
                        account_tx_id,
                        SwapDetails {
                            id,
                            token_in,
                            token_out,
                            amount_in,
                            amount_out,
                            pool_address,
                            tx_id,
                        },
                    )
                },
            )
            .collect();
        let mut transaction_details = Vec::new();
        for account_tx in account_txs {
            match account_tx.tx_type.as_str() {
                "transfer" => {
                    if let Some(transfer) = transfers_map.get(&account_tx.id) {
                        transaction_details.push(AccountTransactionDetails::Transfer(
                            TransferTransactionDto {
                                account_transaction: account_tx,
                                transfer: transfer.clone(),
                            },
                        ));
                    }
                }
                "swap" => {
                    if let Some(swap) = swaps_map.get(&account_tx.id) {
                        transaction_details.push(AccountTransactionDetails::Swap(
                            SwapTransactionDto {
                                account_transaction: account_tx,
                                swap: swap.clone(),
                            },
                        ));
                    }
                }
                _ => continue,
            }
        }

        Ok(transaction_details)
    }

    pub async fn get_swaps_in_period(
        &self,
        start_time: chrono::NaiveDateTime,
        end_time: chrono::NaiveDateTime,
    ) -> Result<Vec<SwapTransactionDto>> {
        let mut conn = self.db_pool.get().await?;

        use crate::schema::{account_transactions, swaps};

        // Get account transactions in period that are swaps
        let swap_account_txs: Vec<AccountTransaction> = account_transactions::table
            .filter(account_transactions::tx_type.eq("swap"))
            .filter(account_transactions::timestamp.ge(start_time))
            .filter(account_transactions::timestamp.lt(end_time))
            .load(&mut conn)
            .await?;

        if swap_account_txs.is_empty() {
            return Ok(Vec::new());
        }

        let tx_ids: Vec<i64> = swap_account_txs.iter().map(|tx| tx.id).collect();

        // Get swap details for these transactions
        let swaps_map: std::collections::HashMap<i64, SwapDetails> = swaps::table
            .filter(swaps::account_transaction_id.eq_any(&tx_ids))
            .load::<(
                i64,
                i64,
                String,
                String,
                bigdecimal::BigDecimal,
                bigdecimal::BigDecimal,
                String,
                String,
            )>(&mut conn)
            .await?
            .into_iter()
            .map(
                |(
                    id,
                    account_tx_id,
                    token_in,
                    token_out,
                    amount_in,
                    amount_out,
                    pool_address,
                    tx_id,
                )| {
                    (
                        account_tx_id,
                        SwapDetails {
                            id,
                            token_in,
                            token_out,
                            amount_in,
                            amount_out,
                            pool_address,
                            tx_id,
                        },
                    )
                },
            )
            .collect();

        let mut result = Vec::new();
        for account_tx in swap_account_txs {
            if let Some(swap) = swaps_map.get(&account_tx.id) {
                result.push(SwapTransactionDto {
                    account_transaction: account_tx,
                    swap: swap.clone(),
                });
            }
        }

        Ok(result)
    }
}

#[async_trait]
impl AccountTransactionRepositoryTrait for AccountTransactionRepository {
    async fn get_swaps_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<SwapTransactionDto>> {
        self.get_swaps_in_period(start_time, end_time).await
    }
}

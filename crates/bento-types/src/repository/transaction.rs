use std::sync::Arc;

use diesel::insert_into;

use crate::{models::transaction::TransactionModel, DbPool};
use anyhow::Result;
use diesel_async::RunQueryDsl;

use diesel::prelude::*;

/// Insert txs into the database
pub async fn insert_txs_to_db(db: Arc<DbPool>, txs: Vec<TransactionModel>) -> Result<()> {
    let mut conn = db.get().await?;
    insert_into(crate::schema::transactions::table)
        .values(&txs)
        .on_conflict(crate::schema::transactions::tx_hash)
        .do_nothing()
        .execute(&mut conn)
        .await?;

    tracing::info!("Inserted {} txs", txs.len());

    Ok(())
}

/// List transactions with pagination
pub async fn get_txs(db: Arc<DbPool>, limit: i64, offset: i64) -> Result<Vec<TransactionModel>> {
    use crate::schema::transactions::dsl::*;

    let mut conn = db.get().await?;

    let tx_models: Vec<TransactionModel> = transactions
        .limit(limit)
        .offset(offset)
        .select(TransactionModel::as_select())
        .load(&mut conn)
        .await?;

    Ok(tx_models)
}

/// Get transaction by hash
pub async fn get_tx_by_hash(
    db: Arc<DbPool>,
    tx_hash_value: &str,
) -> Result<Option<TransactionModel>> {
    use crate::schema::transactions::dsl::*;

    let mut conn = db.get().await?;
    let tx = transactions
        .filter(tx_hash.eq(tx_hash_value))
        .select(TransactionModel::as_select())
        .first(&mut conn)
        .await
        .ok();
    Ok(tx)
}

/// Get transaction by block
pub async fn get_txs_by_block(
    db: Arc<DbPool>,
    block_hash_value: &str,
) -> Result<Vec<TransactionModel>> {
    use crate::schema::transactions::dsl::*;

    // Check if block exists
    let block_exists = crate::repository::block::exists_block(db.clone(), block_hash_value).await?;
    if !block_exists {
        return Err(anyhow::anyhow!("Block not found"));
    }
    let mut conn = db.get().await?;
    let txs = transactions
        .filter(block_hash.eq(block_hash_value))
        .select(TransactionModel::as_select())
        .load(&mut conn)
        .await?;

    Ok(txs)
}

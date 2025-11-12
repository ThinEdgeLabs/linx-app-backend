pub mod block;
pub mod event;
pub mod transaction;
use std::sync::Arc;

pub use block::*;
pub use event::*;
pub use transaction::*;

use crate::{
    errors::api::RepositoryError,
    models::{block::BlockModel, event::EventModel, transaction::TransactionModel},
    BlockHash, DbPool,
};
use anyhow::Ok;
use anyhow::Result;
use diesel::insert_into;
use diesel::prelude::*;
use diesel::ExpressionMethods;
use diesel_async::{scoped_futures::ScopedFutureExt, AsyncConnection, RunQueryDsl};

/// Insert block and events into the database.
pub async fn insert_block_and_events(
    db: Arc<DbPool>,
    block: BlockModel,
    events: Vec<EventModel>,
) -> Result<()> {
    let mut conn = db.get().await?;
    conn.transaction(|conn| {
        async move {
            insert_into(crate::schema::blocks::table).values(&block).execute(conn).await?;
            insert_into(crate::schema::events::table).values(&events).execute(conn).await?;
            diesel::result::QueryResult::Ok(())
        }
        .scope_boxed()
    })
    .await?;
    Ok(())
}

/// Update main chain status of block and transactions related to a block hash.
/// Reference: https://github.com/alephium/explorer-backend/blob/09cf587672bcc4cf1d02e927e88b2e71df08b2e1/app/src/main/scala/org/alephium/explorer/persistence/dao/BlockDao.scala#L121
pub async fn update_main_chain(
    db: Arc<DbPool>,
    block_hash: BlockHash,
    chain_from: i64,
    chain_to: i64,
    group_num: Option<i64>,
) -> Result<BlockHash> {
    let mut current_hash = block_hash;

    loop {
        let block = get_block_by_hash(db.clone(), &current_hash).await?;

        match block {
            Some(block) => {
                if !block.main_chain {
                    assert_eq!(block.chain_from, chain_from);
                    assert_eq!(block.chain_to, chain_to);
                }

                let block_hashes = fetch_block_hashes_at_height_filter_one(
                    db.clone(),
                    block.chain_from,
                    block.chain_to,
                    block.height,
                    &block.hash,
                )
                .await?;

                // Update any old main chain blocks to not be main chain
                update_main_chain_status(db.clone(), block_hashes, false).await?;

                // Update the given block to be main chain
                update_main_chain_status(db.clone(), vec![current_hash.clone()], true).await?;

                current_hash = block.parent(group_num).unwrap();
                continue;
            }
            None => break Ok(current_hash),
        }
    }
}

/// Update main chain status of block and transactions related to a list of block hashes.
pub async fn update_main_chain_status(
    db: Arc<DbPool>,
    block_hashes: Vec<String>,
    main_chain: bool,
) -> Result<()> {
    let mut conn = db.get().await?;
    if block_hashes.is_empty() {
        return Ok(());
    }
    for block_hash in block_hashes {
        conn.transaction(|conn| {
            async move {
                diesel::update(
                    crate::schema::blocks::table
                        .filter(crate::schema::blocks::hash.eq(block_hash.clone())),
                )
                .set(crate::schema::blocks::main_chain.eq(main_chain))
                .execute(conn)
                .await?;
                diesel::update(
                    crate::schema::transactions::table
                        .filter(crate::schema::transactions::block_hash.eq(block_hash)),
                )
                .set(crate::schema::transactions::main_chain.eq(main_chain))
                .execute(conn)
                .await?;
                diesel::result::QueryResult::Ok(())
            }
            .scope_boxed()
        })
        .await?;
    }
    Ok(())
}

pub async fn get_block_transactions(
    db: Arc<DbPool>,
    block_hash_value: BlockHash,
) -> Result<Vec<TransactionModel>> {
    let block_model = get_block_by_hash(db.clone(), &block_hash_value).await?;

    if let Some(block_model) = block_model {
        let mut conn = db.get().await?;
        let transaction_models = TransactionModel::belonging_to(&block_model)
            .select(TransactionModel::as_select())
            .load(&mut conn)
            .await?;
        Ok(transaction_models)
    } else {
        Err(RepositoryError::BlockNotFound(block_hash_value).into())
    }
}

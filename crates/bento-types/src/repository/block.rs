use std::sync::Arc;

use chrono::NaiveDateTime;
use diesel::{insert_into, query_dsl::methods::FilterDsl, SelectableHelper};

use crate::Order;
use crate::{models::block::BlockModel, DbPool};
use anyhow::Result;
use diesel::ExpressionMethods;

use diesel::query_dsl::methods::OrderDsl;
use diesel::query_dsl::methods::{LimitDsl, OffsetDsl, SelectDsl};
use diesel_async::RunQueryDsl;

/// Insert blocks into the database
#[allow(clippy::get_first)]
pub async fn insert_blocks_to_db(db: Arc<DbPool>, block_models: Vec<BlockModel>) -> Result<()> {
    if block_models.is_empty() {
        return Ok(());
    }
    let mut conn = db.get().await?;
    insert_into(crate::schema::blocks::table)
        .values(&block_models)
        .on_conflict(crate::schema::blocks::hash)
        .do_nothing()
        .execute(&mut conn)
        .await?;
    tracing::info!(
        "Inserted {} blocks from timestamp {} to timestamp {}",
        block_models.len(),
        block_models.get(0).unwrap().timestamp,
        block_models.last().unwrap().timestamp,
    );
    Ok(())
}

/// Get Block by hash
pub async fn get_block_by_hash(db: Arc<DbPool>, block_hash: &str) -> Result<Option<BlockModel>> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    let block = blocks
        .filter(hash.eq(block_hash))
        .select(BlockModel::as_select())
        .first(&mut conn)
        .await
        .ok();
    Ok(block)
}

/** Fetch bloch-hashes belonging to the input chain-index at a height, ignoring/filtering-out one
 * block-hash.
 *
 * @param fromGroup
 *   `chain_from` of the blocks
 * @param toGroup
 *   `chain_to` of the blocks
 * @param height
 *   `height` of the blocks
 * @param hashToIgnore
 *   the `block-hash` to ignore or filter-out.
 */
pub async fn fetch_block_hashes_at_height_filter_one(
    db: Arc<DbPool>,
    from_group: i64,
    to_group: i64,
    height_value: i64,
    hash_to_ignore: &str,
) -> Result<Vec<String>> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    let block_hashes = blocks
        .filter(chain_from.eq(from_group))
        .filter(chain_to.eq(to_group))
        .filter(height.eq(height_value))
        .filter(hash.ne(hash_to_ignore))
        .select(hash)
        .load(&mut conn)
        .await?;

    Ok(block_hashes)
}

/// Get blocks, order by height
pub async fn get_blocks(
    db: Arc<DbPool>,
    limit: i64,
    offset: i64,
    order: Option<Order>,
) -> Result<Vec<BlockModel>> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    match order {
        Some(Order::Desc) => {
            let block_models = blocks
                .limit(limit)
                .offset(offset)
                .select(BlockModel::as_select())
                .order(height.desc())
                .load(&mut conn)
                .await?;
            Ok(block_models)
        }
        _ => {
            let block_models = blocks
                .limit(limit)
                .offset(offset)
                .select(BlockModel::as_select())
                .order(height.asc())
                .load(&mut conn)
                .await?;
            Ok(block_models)
        }
    }
}

pub async fn get_block_by_height(db: Arc<DbPool>, height_value: i64) -> Result<Option<BlockModel>> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    let block_model = blocks
        .filter(height.eq(height_value))
        .select(BlockModel::as_select())
        .first(&mut conn)
        .await
        .ok();

    Ok(block_model)
}

pub async fn exists_block(db: Arc<DbPool>, block_hash_value: &str) -> Result<bool> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    let block_exists = blocks
        .filter(hash.eq(block_hash_value))
        .select(BlockModel::as_select())
        .first(&mut conn)
        .await
        .is_ok();

    Ok(block_exists)
}

pub async fn get_max_block_timestamp(db: &Arc<DbPool>) -> Result<Option<i64>> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    let max_timestamp: Option<NaiveDateTime> =
        blocks.select(diesel::dsl::max(timestamp)).first(&mut conn).await?;
    let milliseconds =
        max_timestamp.map(|ts: NaiveDateTime| ts.and_utc().timestamp_millis());
    Ok(milliseconds)
}

pub async fn get_blocks_at_height(db: &Arc<DbPool>, height_value: i64) -> Result<Vec<BlockModel>> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    let block_models = blocks
        .filter(height.eq(height_value))
        .select(BlockModel::as_select())
        .load(&mut conn)
        .await?;
    Ok(block_models)
}

pub async fn get_latest_block(
    db: &Arc<DbPool>,
    from_group: i64,
    to_group: i64,
) -> Result<Option<BlockModel>> {
    use crate::schema::blocks::dsl::*;

    let mut conn = db.get().await?;
    let block_model = blocks
        .filter(chain_from.eq(from_group))
        .filter(chain_to.eq(to_group))
        .order(height.desc())
        .limit(1)
        .select(BlockModel::as_select())
        .first(&mut conn)
        .await
        .ok();

    Ok(block_model)
}

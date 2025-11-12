use axum::extract::{Query, State};
use axum::Json;
use bento_types::repository::{
    get_block_by_hash, get_block_by_height, get_block_transactions, get_blocks,
};

use crate::error::AppError;
use crate::handler::dto::{BlockByHeightQuery, TransactionDto};
use crate::handler::dto::{BlockDto, BlocksQuery};
use crate::AppState;
use axum::response::IntoResponse;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::dto::BlockByHashQuery;
pub struct BlockApiModule;

impl BlockApiModule {
    pub fn register() -> OpenApiRouter<crate::AppState> {
        OpenApiRouter::new()
            .routes(routes!(get_blocks_handler))
            .routes(routes!(get_block_by_hash_handler))
            .routes(routes!(get_block_by_height_handler))
            .routes(routes!(get_block_transactions_handler))
    }
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Blocks",
    params(BlocksQuery),
    responses(
        (status = 200, description = "List of blocks retrieved successfully", body = Vec<BlockDto>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_blocks_handler(
    Query(query): Query<BlocksQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db;
    let pagination = query.pagination;
    let block_models =
        get_blocks(db, pagination.get_limit(), pagination.get_offset(), Some(query.order)).await?;
    Ok(Json(block_models))
}

#[utoipa::path(
    get,
    path = "/hash",
    tag = "Blocks",
    params(BlockByHashQuery),
    responses(
        (status = 200, description = "Block retrieved successfully", body = BlockDto),
        (status = 404, description = "Block not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_block_by_hash_handler(
    Query(query): Query<BlockByHashQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db;
    let block_model = get_block_by_hash(db, &query.hash).await?;
    if block_model.is_none() {
        return Err(AppError::NotFound("Block not found".to_string()));
    }
    Ok(Json(block_model))
}

#[utoipa::path(
    get,
    path = "/height",
    tag = "Blocks",
    params(BlockByHeightQuery),
    responses(
        (status = 200, description = "Block retrieved successfully", body = BlockDto),
        (status = 404, description = "Block not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_block_by_height_handler(
    Query(query): Query<BlockByHeightQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db;
    //TODO: Pass the from_group and to_group parameters to the repository function
    let block_model = get_block_by_height(db, query.height).await?;

    if block_model.is_none() {
        return Err(AppError::NotFound("Block not found".to_string()));
    }

    Ok(Json(block_model))
}

#[utoipa::path(
    get,
    path = "/transactions",
    tag = "Blocks",
    params(BlockByHashQuery),
    responses(
        (status = 200, description = "Block transactions retrieved successfully", body = Vec<TransactionDto>),
        (status = 404, description = "Block not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_block_transactions_handler(
    Query(query): Query<BlockByHashQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db;
    let transaction_models = get_block_transactions(db, query.hash).await?;

    Ok(Json(transaction_models))
}

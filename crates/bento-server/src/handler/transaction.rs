use axum::extract::{Query, State};
use axum::Json;
use bento_types::repository::{get_tx_by_hash, get_txs, get_txs_by_block};

use crate::error::AppError;
use crate::handler::dto::{TransactionBlockQuery, TransactionDto, TransactionHashQuery, TransactionsQuery};
use crate::AppState;
use crate::Pagination;
use axum::response::IntoResponse;
use utoipa_axum::{router::OpenApiRouter, routes};

pub struct TransactionApiModule;

impl TransactionApiModule {
    pub fn register() -> OpenApiRouter<crate::AppState> {
        OpenApiRouter::new()
            .routes(routes!(get_txs_handler))
            .routes(routes!(get_tx_by_hash_handler))
            .routes(routes!(get_tx_by_block_handler))
    }
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Transactions",
    params(TransactionsQuery),
    responses(
        (status = 200, description = "List of transactions retrieved successfully", body = Vec<TransactionDto>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_txs_handler(
    pagination: Query<Pagination>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db;
    let tx_models = get_txs(db, pagination.get_limit(), pagination.get_offset()).await?;
    Ok(Json(tx_models))
}

#[utoipa::path(
    get,
    path = "/hash",
    tag = "Transactions",
    params(TransactionHashQuery),
    responses(
        (status = 200, description = "Transaction retrieved successfully", body = TransactionDto),
        (status = 404, description = "Transaction not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_tx_by_hash_handler(
    Query(query): Query<TransactionHashQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db;
    let hash = query.hash.clone();
    let tx_model = get_tx_by_hash(db, &hash).await?;

    if tx_model.is_none() {
        return Err(AppError::NotFound(format!("Transaction with tx id {hash} not found").to_string()));
    }
    Ok(Json(tx_model))
}

#[utoipa::path(
    get,
    path = "/block",
    tag = "Transactions",
    params(TransactionBlockQuery),
    responses(
        (status = 200, description = "Transaction retrieved successfully", body = TransactionDto),
        (status = 404, description = "Transaction not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_tx_by_block_handler(
    Query(query): Query<TransactionBlockQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db;
    let block_hash = query.block_hash.clone();
    let tx_models = get_txs_by_block(db, &block_hash).await?;
    Ok(Json(tx_models))
}

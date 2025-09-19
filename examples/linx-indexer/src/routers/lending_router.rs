use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use bento_server::{AppState, error::AppError};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{models::Market, repository::LendingRepository};

pub struct LendingRouter;

impl LendingRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new().routes(routes!(get_markets_handler))
    }
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct MarketsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
}

fn default_limit() -> i64 {
    20
}

fn default_page() -> i64 {
    1
}

#[utoipa::path(
    get,
    path = "/lending/markets",
    tag = "Markets",
    params(MarketsQuery),
    responses(
        (status = 200, description = "List of markets retrieved successfully", body = Vec<Market>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_markets_handler(
    Query(query): Query<MarketsQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let markets = lending_repo.get_markets(query.page, query.limit).await?;

    Ok(Json(markets))
}

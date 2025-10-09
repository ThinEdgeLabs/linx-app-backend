use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
};
use bento_server::{AppState, error::AppError};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;

use crate::{
    models::{LendingEvent, Market, Position},
    repository::LendingRepository,
};

pub struct LendingRouter;

impl LendingRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new()
            .route("/lending/markets", get(get_markets))
            .route("/lending/borrow-activity", get(get_borrow_activity))
            .route("/lending/earn-activity", get(get_earn_activity))
            .route("/lending/positions", get(get_positions))
    }
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct Pagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ActivityQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
    pub market_id: String,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct PositionsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
    pub market_id: Option<String>,
    pub address: Option<String>,
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
    params(Pagination),
    responses(
        (status = 200, description = "List of markets retrieved successfully", body = Vec<Market>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_markets(
    Query(query): Query<Pagination>,
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

#[utoipa::path(
    get,
    path = "/lending/borrow-activity",
    tag = "Borrow Activity",
    params(ActivityQuery),
    responses(
        (status = 200, description = "List of borrow events retrieved successfully", body = Vec<LendingEvent>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_borrow_activity(
    Query(query): Query<ActivityQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let borrow_events = [
        String::from("Borrow"),
        String::from("Repay"),
        String::from("Liquidate"),
        String::from("SupplyCollateral"),
        String::from("WithdrawCollateral"),
    ];
    let borrow_activity =
        lending_repo.get_activity(query.market_id, &borrow_events, query.page, query.limit).await?;

    Ok(Json(borrow_activity))
}

#[utoipa::path(
    get,
    path = "/lending/earn-activity",
    tag = "Earn Activity",
    params(ActivityQuery),
    responses(
        (status = 200, description = "List of earn events retrieved successfully", body = Vec<LendingEvent>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_earn_activity(
    Query(query): Query<ActivityQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let earn_events = [String::from("Supply"), String::from("Withdraw")];
    let earn_activity =
        lending_repo.get_activity(query.market_id, &earn_events, query.page, query.limit).await?;

    Ok(Json(earn_activity))
}

#[utoipa::path(
    get,
    path = "/lending/positions",
    tag = "Positions",
    params(PositionsQuery),
    responses(
        (status = 200, description = "List of positions retrieved successfully", body = Vec<Position>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_positions(
    Query(query): Query<PositionsQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    if query.market_id.is_none() && query.address.is_none() {
        return Err(AppError::BadRequest(
            "Either market_id or address must be provided".to_string(),
        ));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let positions =
        lending_repo.get_positions(query.market_id, query.address, query.page, query.limit).await?;

    Ok(Json(positions))
}

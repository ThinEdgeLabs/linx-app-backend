use axum::{Json, extract::State, response::IntoResponse, routing::get};
use bento_server::{AppState, error::AppError};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;

use crate::repository::{PointsRepository, PointsRepositoryTrait};

pub struct StatsRouter;

impl StatsRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new().route("/stats/v1", get(get_stats_handler))
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatsResponse {
    pub unique_users: i64,
}

#[utoipa::path(
    get,
    path = "/stats/v1",
    responses(
        (status = 200, description = "Stats retrieved successfully", body = StatsResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "stats"
)]
async fn get_stats_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let repo = PointsRepository::new(state.db.clone());
    let unique_users = repo.get_unique_users_count().await?;
    Ok(Json(StatsResponse { unique_users }))
}

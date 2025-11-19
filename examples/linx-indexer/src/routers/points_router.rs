use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
};
use bento_server::{AppState, error::AppError};
use bigdecimal::BigDecimal;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;

use crate::repository::{PointsRepository, PointsRepositoryTrait};

pub struct PointsRouter;

impl PointsRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new()
            .route("/points/leaderboard", get(get_leaderboard_handler))
            .route("/points/{address}", get(get_user_points_handler))
    }
}

// ==================== Response Models ====================

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardEntry {
    pub user: String,
    #[schema(value_type = String)]
    pub total_points: BigDecimal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPointsResponse {
    #[schema(value_type = String)]
    pub points: BigDecimal,
}

// ==================== Handler Functions ====================

/// Get points leaderboard
///
/// Returns the top 50 users ranked by their total points from the latest snapshot.
#[utoipa::path(
    get,
    path = "/points/leaderboard",
    tag = "Points",
    responses(
        (status = 200, description = "Successfully retrieved leaderboard", body = Vec<LeaderboardEntry>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_leaderboard_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // Create repository
    let repo = PointsRepository::new(state.db.clone());

    // Fetch top 50 from latest snapshot (snapshot_date = None, page = 1, limit = 50)
    let snapshots = repo.get_leaderboard(None, 1, 50).await?;

    // Map to simplified response format
    let leaderboard: Vec<LeaderboardEntry> = snapshots
        .into_iter()
        .map(|snapshot| LeaderboardEntry {
            user: snapshot.address,
            total_points: snapshot.total_points,
        })
        .collect();

    Ok(Json(leaderboard))
}

/// Get user points
///
/// Returns the total points for a specific user address from the latest snapshot.
#[utoipa::path(
    get,
    path = "/points/{address}",
    tag = "Points",
    params(
        ("address" = String, Path, description = "User wallet address")
    ),
    responses(
        (status = 200, description = "Successfully retrieved user points", body = UserPointsResponse),
        (status = 404, description = "User snapshot not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_points_handler(
    Path(address): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let repo = PointsRepository::new(state.db.clone());
    let snapshot = repo.get_latest_snapshot(&address).await?;

    match snapshot {
        Some(snapshot) => Ok(Json(UserPointsResponse { points: snapshot.total_points })),
        None => Err(AppError::NotFound(format!("No points found for address {}", address))),
    }
}

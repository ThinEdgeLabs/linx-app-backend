use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
};
use bento_server::{AppState, error::AppError};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;

use crate::models::Season;
use crate::repository::{PointsRepository, PointsRepositoryTrait};

pub struct PointsRouter;

impl PointsRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new()
            .route("/points/leaderboard", get(get_leaderboard_handler))
            .route("/points/season", get(get_current_season_handler))
            .route("/points/{address}", get(get_user_points_handler))
    }
}

// ==================== Response Models ====================

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardEntry {
    pub user: String,
    pub points: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPointsResponse {
    pub points: i32,
    pub rank: i64,
}

// ==================== Handler Functions ====================

/// Get points leaderboard
///
/// Returns the top 50 users ranked by their total points from the latest snapshot for the active season.
#[utoipa::path(
    get,
    path = "/points/leaderboard",
    tag = "Points",
    responses(
        (status = 200, description = "Successfully retrieved leaderboard", body = Vec<LeaderboardEntry>),
        (status = 404, description = "No active season found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_leaderboard_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // Create repository
    let repo = PointsRepository::new(state.db.clone());

    // Get active season
    let active_season = repo.get_active_season().await?
        .ok_or_else(|| AppError::NotFound("No active season found".to_string()))?;

    // Fetch top 50 from latest snapshot (season_id, snapshot_date = None, page = 1, limit = 50)
    let snapshots = repo.get_leaderboard(active_season.id, None, 1, 50).await?;

    // Map to simplified response format
    let leaderboard: Vec<LeaderboardEntry> = snapshots
        .into_iter()
        .map(|snapshot| LeaderboardEntry { user: snapshot.address, points: snapshot.total_points })
        .collect();

    Ok(Json(leaderboard))
}

/// Get user points
///
/// Returns the total points for a specific user address from the latest snapshot for the active season.
#[utoipa::path(
    get,
    path = "/points/{address}",
    tag = "Points",
    params(
        ("address" = String, Path, description = "User wallet address")
    ),
    responses(
        (status = 200, description = "Successfully retrieved user points", body = UserPointsResponse),
        (status = 404, description = "User snapshot not found or no active season"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_points_handler(
    Path(address): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let repo = PointsRepository::new(state.db.clone());

    // Get active season
    let active_season = repo.get_active_season().await?
        .ok_or_else(|| AppError::NotFound("No active season found".to_string()))?;

    let snapshot = repo.get_latest_snapshot(&address, active_season.id).await?;

    match snapshot {
        Some(snapshot) => {
            let rank = repo.get_user_rank(&snapshot).await?;

            Ok(Json(UserPointsResponse { points: snapshot.total_points, rank }))
        }
        None => Err(AppError::NotFound(format!("No points found for address {}", address))),
    }
}

/// Get current season
///
/// Returns the currently active season information.
#[utoipa::path(
    get,
    path = "/points/season",
    tag = "Points",
    responses(
        (status = 200, description = "Successfully retrieved current season", body = Season),
        (status = 404, description = "No active season found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_current_season_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let repo = PointsRepository::new(state.db.clone());

    let active_season = repo.get_active_season().await?
        .ok_or_else(|| AppError::NotFound("No active season found".to_string()))?;

    Ok(Json(active_season))
}

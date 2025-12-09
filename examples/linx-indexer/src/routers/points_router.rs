use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
};
use bento_server::{AppState, error::AppError};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;

use crate::models::{NewUserReferral, Season};
use crate::repository::{PointsRepository, PointsRepositoryTrait};

pub struct PointsRouter;

impl PointsRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new()
            .route("/points/leaderboard", get(get_leaderboard_handler))
            .route("/points/season", get(get_current_season_handler))
            .route("/points/apply-referral", post(apply_referral_handler))
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
    pub referral_code: String,
    pub referrals: i64,
}

#[derive(Debug, serde::Deserialize, ToSchema)]
pub struct ApplyReferralRequest {
    pub user_address: String,
    pub public_key: String,
    pub referral_code: String,
    pub signature: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplyReferralResponse {
    pub success: bool,
    pub message: String,
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
    let active_season = repo
        .get_active_season()
        .await?
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
/// Returns the total points for a specific user address from the latest snapshot for the active season,
/// along with their referral code.
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
    let active_season = repo
        .get_active_season()
        .await?
        .ok_or_else(|| AppError::NotFound("No active season found".to_string()))?;

    let snapshot = repo.get_latest_snapshot(&address, active_season.id).await?;

    match snapshot {
        Some(snapshot) => {
            let rank = repo.get_user_rank(&snapshot).await?;

            // Get or create referral code for this user
            let referral_code = repo.get_or_create_referral_code(&address).await?;

            // Get count of users who used this user's referral code
            let referrals = repo.count_referrals_by_address(&address).await?;

            Ok(Json(UserPointsResponse {
                points: snapshot.total_points,
                rank,
                referral_code,
                referrals,
            }))
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

    let active_season = repo
        .get_active_season()
        .await?
        .ok_or_else(|| AppError::NotFound("No active season found".to_string()))?;

    Ok(Json(active_season))
}

/// Apply a referral code
///
/// Links a user to a referrer by applying their referral code. Can only be done once per user.
/// Requires a signed message to prove ownership of the address.
///
/// Message format to sign: "Apply referral: {referral_code} at {timestamp}"
#[utoipa::path(
    post,
    path = "/points/apply-referral",
    tag = "Points",
    request_body = ApplyReferralRequest,
    responses(
        (status = 200, description = "Referral code processing result", body = ApplyReferralResponse),
        (status = 400, description = "Invalid referral code"),
        (status = 403, description = "Invalid signature"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn apply_referral_handler(
    State(state): State<AppState>,
    Json(request): Json<ApplyReferralRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify timestamp is recent (within 5 minutes)
    const MAX_TIME_DIFF_MS: i64 = 5 * 60 * 1000;
    let current_timestamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
            as i64;

    let timestamp_diff = (current_timestamp - request.timestamp).abs();
    if timestamp_diff > MAX_TIME_DIFF_MS {
        return Err(AppError::Forbidden("Invalid signature".to_string()));
    }

    // Verify that the public key matches the address
    let pubkey_matches =
        crate::crypto::verify_public_key_for_address(&request.public_key, &request.user_address)
            .map_err(|_| AppError::Forbidden("Invalid signature".to_string()))?;

    if !pubkey_matches {
        return Err(AppError::Forbidden("Invalid signature".to_string()));
    }

    // Construct the message that should have been signed
    let message = format!("Apply referral: {} at {}", request.referral_code, request.timestamp);

    // Verify the signature
    let is_valid =
        crate::crypto::verify_signature(&request.public_key, &message, &request.signature)
            .map_err(|_| AppError::Forbidden("Invalid signature".to_string()))?;

    if !is_valid {
        return Err(AppError::Forbidden("Invalid signature".to_string()));
    }

    let repo = PointsRepository::new(state.db.clone());

    // Check if user already has a referral
    if let Some(_existing) = repo.get_user_referral(&request.user_address).await? {
        return Ok(Json(ApplyReferralResponse {
            success: false,
            message: "User has already used a referral code".to_string(),
        }));
    }

    // Get the referral code details
    let referral_code = repo
        .get_referral_code(&request.referral_code)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid referral code".to_string()))?;

    // Check that user is not using their own referral code
    if referral_code.owner_address.to_lowercase() == request.user_address.to_lowercase() {
        return Ok(Json(ApplyReferralResponse {
            success: false,
            message: "Cannot use your own referral code".to_string(),
        }));
    }

    // Create the user referral entry
    let new_referral = NewUserReferral {
        user_address: request.user_address.clone(),
        referred_by_address: referral_code.owner_address.clone(),
    };

    repo.insert_user_referral(new_referral).await?;

    Ok(Json(ApplyReferralResponse {
        success: true,
        message: format!("Successfully applied referral code from {}", referral_code.owner_address),
    }))
}

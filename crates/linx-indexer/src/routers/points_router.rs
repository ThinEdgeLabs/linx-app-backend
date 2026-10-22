use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use bento_server::{AppState, error::AppError};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;

use crate::models::{NewUserReferral, ReferralDetailsResponse, Season};
use crate::repository::{PointsRepository, PointsRepositoryTrait};

pub struct PointsRouter;

impl PointsRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new()
            .route("/points/v1/leaderboard", get(get_leaderboard_handler))
            .route("/points/v1/season", get(get_current_season_handler))
            .route("/points/v1/apply-referral", post(apply_referral_handler))
            .route("/points/v1/referrals/{address}", get(get_referral_details_handler))
            .route("/points/v1/share/{referral_code}", get(get_share_image_handler))
            .route("/points/v1/{address}", get(get_user_points_handler))
    }
}

// ==================== Response Models ====================

#[derive(Debug, Serialize, ToSchema, diesel::QueryableByName)]
pub struct LeaderboardEntry {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub user: String,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub points: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPointsResponse {
    pub points: i64,
    pub rank: i64,
    pub referral_code: String,
    pub referrals: i64,
    pub has_applied_referral_code: bool,
    pub token_allocation: String,
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

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ReferralDetailsQuery {
    /// Page number for pagination (starts at 0)
    #[serde(default = "default_page")]
    pub page: i64,
    /// Number of results per page (max 100)
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct UserPointsQuery {
    /// Optional season ID. If not provided, uses the active season. Mutually exclusive with all_seasons.
    pub season_id: Option<i32>,
    /// When true, return points and rank aggregated across all seasons.
    pub all_seasons: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct LeaderboardQuery {
    /// Specific season id. Omit for the active season. Mutually exclusive with all_seasons.
    pub season_id: Option<i32>,
    /// When true, return an aggregated leaderboard across all seasons.
    pub all_seasons: Option<bool>,
}

fn default_page() -> i64 {
    0
}

fn default_limit() -> i64 {
    20
}

// ==================== Helper Functions ====================

// fn calculate_token_allocation(user_points: i32, total_points: i64, tokens_allocation: &BigDecimal) -> String {
//     if total_points == 0 || user_points == 0 {
//         return "0".to_string();
//     }
//     let user_bd = BigDecimal::from(user_points);
//     let total_bd = BigDecimal::from(total_points);
//     let allocation = ((user_bd / total_bd) * tokens_allocation).round(0);
//     let decimals = BigDecimal::from(10u64.pow(18));
//     (allocation * decimals).to_string()
// }

// ==================== Handler Functions ====================

/// Get points leaderboard
///
/// Returns the top 50 users ranked by their total points. By default uses the latest snapshot
/// for the active season. Pass `season_id` for a specific season, or `all_seasons=true` for an
/// aggregated leaderboard across all seasons. `season_id` and `all_seasons` are mutually exclusive.
#[utoipa::path(
    get,
    path = "/points/leaderboard",
    tag = "Points",
    params(LeaderboardQuery),
    responses(
        (status = 200, description = "Successfully retrieved leaderboard", body = Vec<LeaderboardEntry>),
        (status = 400, description = "season_id and all_seasons are mutually exclusive"),
        (status = 404, description = "No active season found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_leaderboard_handler(
    Query(query): Query<LeaderboardQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let repo = PointsRepository::new(state.db.clone());

    let all_seasons = query.all_seasons.unwrap_or(false);

    if query.season_id.is_some() && all_seasons {
        return Err(AppError::BadRequest("season_id and all_seasons are mutually exclusive".to_string()));
    }

    // Aggregated all-seasons leaderboard.
    if all_seasons {
        let leaderboard = repo.get_global_leaderboard(50).await?;
        return Ok(Json(leaderboard));
    }

    // Resolve the season: explicit season_id, or fall back to the active season.
    let season_id = match query.season_id {
        Some(id) => id,
        None => {
            repo.get_active_season().await?.ok_or_else(|| AppError::NotFound("No active season found".to_string()))?.id
        }
    };

    // Fetch top 50 from the latest snapshot for the season.
    let snapshots = repo.get_leaderboard(season_id, None, 1, 50).await?;

    // Map to simplified response format
    let leaderboard: Vec<LeaderboardEntry> = snapshots
        .into_iter()
        .map(|snapshot| LeaderboardEntry { user: snapshot.address, points: i64::from(snapshot.total_points) })
        .collect();

    Ok(Json(leaderboard))
}

/// Get user points
///
/// Returns the total points for a specific user address from the latest snapshot for a season,
/// along with their referral code. If no season_id is provided, uses the active season.
/// Pass `all_seasons=true` for points and rank aggregated across all seasons
/// (`season_id` and `all_seasons` are mutually exclusive).
#[utoipa::path(
    get,
    path = "/points/{address}",
    tag = "Points",
    params(
        ("address" = String, Path, description = "User wallet address"),
        UserPointsQuery
    ),
    responses(
        (status = 200, description = "Successfully retrieved user points", body = UserPointsResponse),
        (status = 400, description = "season_id and all_seasons are mutually exclusive"),
        (status = 404, description = "User snapshot not found or no active season"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_points_handler(
    Path(address): Path<String>,
    Query(query): Query<UserPointsQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let repo = PointsRepository::new(state.db.clone());

    let all_seasons = query.all_seasons.unwrap_or(false);

    if query.season_id.is_some() && all_seasons {
        return Err(AppError::BadRequest("season_id and all_seasons are mutually exclusive".to_string()));
    }

    // Check if user has applied a referral code
    let user_referral = repo.get_user_referral(&address).await?;
    let has_applied_referral_code = user_referral.is_some();

    // Aggregated all-seasons view: points and rank summed/ranked across every season.
    // Referral fields below are season-independent and computed the same way as the per-season path.
    if all_seasons {
        let (points, rank) = repo.get_global_user_points(&address).await?.unwrap_or((0, 0));

        let referral_code = repo.get_or_create_referral_code(&address).await?;
        let referrals = repo.count_referrals_by_address(&address).await?;

        return Ok(Json(UserPointsResponse {
            points,
            rank,
            referral_code,
            referrals,
            has_applied_referral_code,
            token_allocation: "0".to_string(),
        }));
    }

    // Get season by ID or fall back to active season
    let season = match query.season_id {
        Some(id) => repo
            .get_season_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Season with id {} not found", id)))?,
        None => {
            repo.get_active_season().await?.ok_or_else(|| AppError::NotFound("No active season found".to_string()))?
        }
    };

    let snapshot = repo.get_latest_snapshot(&address, season.id).await?;
    // let total_points = repo.get_total_points_for_season(season.id).await?;

    match snapshot {
        Some(snapshot) => {
            let rank = repo.get_user_rank(&snapshot).await?;

            // Get or create referral code for this user
            let referral_code = repo.get_or_create_referral_code(&address).await?;

            // Get count of users who used this user's referral code
            let referrals = repo.count_referrals_by_address(&address).await?;

            // let token_allocation =
            //     calculate_token_allocation(snapshot.total_points, total_points, &season.tokens_allocation);

            // Return "0" for token allocation for now since the actual allocation logic and tokenomics are still being finalized
            let token_allocation = "0".to_string();

            Ok(Json(UserPointsResponse {
                points: i64::from(snapshot.total_points),
                rank,
                referral_code,
                referrals,
                has_applied_referral_code,
                token_allocation,
            }))
        }
        None => {
            let referral_code = repo.get_or_create_referral_code(&address).await?;
            Ok(Json(UserPointsResponse {
                points: 0,
                rank: 0,
                referral_code,
                referrals: 0,
                has_applied_referral_code,
                token_allocation: "0".to_string(),
            }))
        }
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
pub async fn get_current_season_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let repo = PointsRepository::new(state.db.clone());

    let active_season =
        repo.get_active_season().await?.ok_or_else(|| AppError::NotFound("No active season found".to_string()))?;

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
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;

    let timestamp_diff = (current_timestamp - request.timestamp).abs();
    if timestamp_diff > MAX_TIME_DIFF_MS {
        return Err(AppError::Forbidden("Invalid signature".to_string()));
    }

    // Verify that the public key matches the address
    let pubkey_matches = crate::crypto::verify_public_key_for_address(&request.public_key, &request.user_address)
        .map_err(|_| AppError::Forbidden("Invalid signature".to_string()))?;

    if !pubkey_matches {
        return Err(AppError::Forbidden("Invalid signature".to_string()));
    }

    // Construct the message that should have been signed
    let message = format!("Apply referral: {} at {}", request.referral_code, request.timestamp);

    // Verify the signature
    let is_valid = crate::crypto::verify_signature(&request.public_key, &message, &request.signature)
        .map_err(|_| AppError::Forbidden("Invalid signature".to_string()))?;

    if !is_valid {
        return Err(AppError::Forbidden("Invalid signature".to_string()));
    }

    let repo = PointsRepository::new(state.db.clone());

    // Check if user already has a referral
    if let Some(_existing) = repo.get_user_referral(&request.user_address).await? {
        return Err(AppError::BadRequest("User has already used a referral code".to_string()));
    }

    // Get the referral code details
    let referral_code = repo
        .get_referral_code(&request.referral_code)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid referral code".to_string()))?;

    // Check that user is not using their own referral code
    if referral_code.owner_address.to_lowercase() == request.user_address.to_lowercase() {
        return Err(AppError::BadRequest("Cannot use your own referral code".to_string()));
    }

    // Create the user referral entry
    let new_referral = NewUserReferral {
        user_address: request.user_address.clone(),
        referred_by_address: referral_code.owner_address.clone(),
    };

    repo.insert_user_referral(new_referral).await?;

    // Award signup bonus immediately
    // Get active season
    let active_season =
        repo.get_active_season().await?.ok_or_else(|| AppError::Internal(anyhow::anyhow!("No active season found")))?;

    // Load config to get bonus amount
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let config = bento_cli::load_config(&config_path)?;
    let points_config = config.points.expect("Points configuration section is required");
    let bonus_amount = points_config.signup_bonus;

    // Award the bonus
    repo.award_bonus_points(&request.user_address, bonus_amount, active_season.id).await?;

    Ok(Json(ApplyReferralResponse {
        success: true,
        message: format!("Successfully applied referral code from {}", referral_code.owner_address),
    }))
}

/// Get referral details for a referrer
///
/// Returns a paginated list of users referred by the specified address,
/// along with the bonus points earned from each referral.
#[utoipa::path(
    get,
    path = "/points/referrals/{address}",
    tag = "Points",
    params(
        ("address" = String, Path, description = "Referrer wallet address"),
        ReferralDetailsQuery
    ),
    responses(
        (status = 200, description = "Successfully retrieved referral details", body = ReferralDetailsResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 404, description = "No active season found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_referral_details_handler(
    Path(address): Path<String>,
    Query(query): Query<ReferralDetailsQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // Validate pagination parameters
    if query.page < 0 {
        return Err(AppError::BadRequest("Page number must be non-negative".to_string()));
    }
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    let repo = PointsRepository::new(state.db.clone());

    // Get active season
    let active_season =
        repo.get_active_season().await?.ok_or_else(|| AppError::Internal(anyhow::anyhow!("No active season found")))?;

    // Get summary (total count and total bonus points)
    let summary = repo.get_referral_summary(&address, active_season.id).await?;

    // Get paginated referral details
    let referrals = repo.get_referral_details_paginated(&address, active_season.id, query.page, query.limit).await?;

    // Calculate if there are more results
    let has_more = (query.page + 1) * query.limit < summary.total_referrals;

    Ok(Json(ReferralDetailsResponse {
        referrer_address: address,
        total_referrals: summary.total_referrals,
        total_bonus_points: summary.total_bonus_points,
        referrals,
        page: query.page,
        limit: query.limit,
        has_more,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ShareImageQuery {
    /// Image format: "portrait" (default) or "landscape"
    #[serde(default = "default_format")]
    pub format: String,
    /// Include the referral code on the image (default: false / no-referral template)
    #[serde(default)]
    pub referral: bool,
}

fn default_format() -> String {
    "portrait".to_string()
}

/// Get share image
///
/// Returns a PNG image for social sharing, showing the user's total points across all seasons.
/// By default the no-referral template is used; pass `?referral=true` to include the referral code.
/// The path parameter is the user's referral code (not their address) to avoid exposing addresses in shared links.
pub async fn get_share_image_handler(
    Path(referral_code): Path<String>,
    Query(query): Query<ShareImageQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let image_format = match query.format.as_str() {
        "portrait" => crate::share_image::ImageFormat::Portrait,
        "landscape" => crate::share_image::ImageFormat::Landscape,
        _ => return Err(AppError::BadRequest("Invalid format: must be 'portrait' or 'landscape'".to_string())),
    };

    let repo = PointsRepository::new(state.db.clone());

    let referral = repo
        .get_referral_code(&referral_code)
        .await?
        .ok_or_else(|| AppError::NotFound("Referral code not found".to_string()))?;

    let (total_points, _rank) = repo
        .get_global_user_points(&referral.owner_address)
        .await?
        .ok_or_else(|| AppError::NotFound("No points found for this user".to_string()))?;

    let points = i32::try_from(total_points).unwrap_or(i32::MAX);

    let include_referral = query.referral;
    let png_bytes = tokio::task::spawn_blocking(move || {
        crate::share_image::generate_share_image(points, &referral_code, image_format, include_referral)
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Image generation task failed: {}", e)))?
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to generate share image: {}", e)))?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=300")],
        png_bytes,
    ))
}

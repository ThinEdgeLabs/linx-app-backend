use crate::schema;
use bigdecimal::BigDecimal;
use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::{AsChangeset, Insertable, Queryable};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ==================== Points Config ====================

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::points_config)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct PointsConfig {
    pub id: i32,
    pub action_type: String,
    #[schema(value_type = Option<String>)]
    pub points_per_usd: Option<BigDecimal>,
    #[schema(value_type = Option<String>)]
    pub points_per_usd_per_day: Option<BigDecimal>,
    pub is_active: bool,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
    #[schema(value_type = String)]
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::points_config)]
pub struct NewPointsConfig {
    pub action_type: String,
    pub points_per_usd: Option<BigDecimal>,
    pub points_per_usd_per_day: Option<BigDecimal>,
    pub is_active: bool,
}

// ==================== Points Multipliers ====================

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::points_multipliers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct PointsMultiplier {
    pub id: i32,
    pub multiplier_type: String,
    #[schema(value_type = String)]
    pub threshold_value: BigDecimal,
    #[schema(value_type = String)]
    pub multiplier: BigDecimal,
    pub is_active: bool,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::points_multipliers)]
pub struct NewPointsMultiplier {
    pub multiplier_type: String,
    pub threshold_value: BigDecimal,
    pub multiplier: BigDecimal,
    pub is_active: bool,
}

// ==================== Seasons ====================

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::points_seasons)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Season {
    pub id: i32,
    pub season_number: i32,
    #[schema(value_type = String)]
    pub start_date: NaiveDate,
    #[schema(value_type = String)]
    pub end_date: NaiveDate,
    #[schema(value_type = String)]
    pub max_tokens_distribution: BigDecimal,
    pub is_active: bool,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::points_seasons)]
pub struct NewSeason {
    pub season_number: i32,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub max_tokens_distribution: BigDecimal,
    pub is_active: bool,
}

// ==================== Referral Codes ====================

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::referral_codes)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ReferralCode {
    pub id: i32,
    pub code: String,
    pub owner_address: String,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::referral_codes)]
pub struct NewReferralCode {
    pub code: String,
    pub owner_address: String,
}

// ==================== User Referrals ====================

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::user_referrals)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct UserReferral {
    pub id: i32,
    pub user_address: String,
    pub referred_by_address: String,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::user_referrals)]
pub struct NewUserReferral {
    pub user_address: String,
    pub referred_by_address: String,
}

// ==================== Points Snapshots ====================

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::points_snapshots)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct PointsSnapshot {
    pub id: i32,
    pub address: String,
    #[schema(value_type = String)]
    pub snapshot_date: NaiveDate,
    pub swap_points: i32,
    pub supply_points: i32,
    pub borrow_points: i32,
    pub base_points_total: i32,
    pub multiplier_type: Option<String>,
    #[schema(value_type = String)]
    pub multiplier_value: BigDecimal,
    pub multiplier_points: i32,
    pub referral_points: i32,
    pub total_points: i32,
    #[schema(value_type = String)]
    pub total_volume_usd: BigDecimal,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
    pub season_id: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::points_snapshots)]
pub struct NewPointsSnapshot {
    pub address: String,
    pub snapshot_date: NaiveDate,
    pub swap_points: i32,
    pub supply_points: i32,
    pub borrow_points: i32,
    pub base_points_total: i32,
    pub multiplier_type: Option<String>,
    pub multiplier_value: BigDecimal,
    pub multiplier_points: i32,
    pub referral_points: i32,
    pub total_points: i32,
    pub total_volume_usd: BigDecimal,
    pub season_id: i32,
}

/// Individual referral entry with bonus points information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReferralDetail {
    /// Address of the referred user
    pub referred_user_address: String,
    /// Bonus points earned by the referrer from this specific user
    pub bonus_points_earned: i32,
    /// Total points earned by the referred user (for context)
    pub referred_user_total_points: i32,
}

/// Summary of referral information (used internally)
pub struct ReferralSummary {
    /// Total number of users referred
    pub total_referrals: i64,
    /// Total bonus points earned from all referrals
    pub total_bonus_points: i32,
}

/// Complete API response for referral details endpoint
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReferralDetailsResponse {
    /// Address of the referrer
    pub referrer_address: String,
    /// Total number of users referred
    pub total_referrals: i64,
    /// Total bonus points earned from all referrals
    pub total_bonus_points: i32,
    /// Paginated list of referrals with individual details
    pub referrals: Vec<ReferralDetail>,
    /// Current page number
    pub page: i64,
    /// Number of results per page
    pub limit: i64,
    /// Whether there are more results available
    pub has_more: bool,
}

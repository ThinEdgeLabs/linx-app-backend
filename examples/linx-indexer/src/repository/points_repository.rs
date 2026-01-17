use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bento_types::DbPool;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, QueryableByName};
use diesel_async::RunQueryDsl;
#[cfg(test)]
use mockall::automock;
use rand::distr::{Alphanumeric, SampleString};

use crate::{
    models::{
        NewPointsConfig, NewPointsMultiplier, NewPointsSnapshot, NewReferralCode, NewSeason,
        NewUserReferral, PointsConfig, PointsMultiplier, PointsSnapshot, ReferralCode, Season,
        UserReferral,
    },
    schema,
};

#[cfg_attr(test, automock)]
#[async_trait]
pub trait PointsRepositoryTrait {
    // ==================== Points Config ====================

    async fn get_points_config(&self) -> Result<Vec<PointsConfig>>;

    async fn get_points_config_for_action(&self, action_type: &str)
    -> Result<Option<PointsConfig>>;

    async fn insert_points_config(&self, configs: &[NewPointsConfig]) -> Result<()>;

    // ==================== Points Multipliers ====================

    async fn get_active_multipliers(&self) -> Result<Vec<PointsMultiplier>>;

    async fn get_multipliers_by_type(&self, multiplier_type: &str)
    -> Result<Vec<PointsMultiplier>>;

    async fn insert_multipliers(&self, multipliers: &[NewPointsMultiplier]) -> Result<()>;

    // ==================== Referral Codes ====================

    async fn get_referral_code(&self, code: &str) -> Result<Option<ReferralCode>>;

    async fn get_referral_code_by_owner(&self, owner_address: &str)
    -> Result<Option<ReferralCode>>;

    async fn insert_referral_code(&self, code: NewReferralCode) -> Result<ReferralCode>;

    async fn get_or_create_referral_code(&self, owner_address: &str) -> Result<String>;

    // ==================== User Referrals ====================

    async fn get_all_user_referrals(&self) -> Result<Vec<UserReferral>>;

    async fn get_user_referral(&self, user_address: &str) -> Result<Option<UserReferral>>;

    async fn insert_user_referral(&self, referral: NewUserReferral) -> Result<UserReferral>;

    async fn count_referrals_by_address(&self, referred_by_address: &str) -> Result<i64>;

    async fn get_referral_summary(
        &self,
        referrer_address: &str,
        season_id: i32,
    ) -> Result<crate::models::ReferralSummary>;

    async fn get_referral_details_paginated(
        &self,
        referrer_address: &str,
        season_id: i32,
        page: i64,
        limit: i64,
    ) -> Result<Vec<crate::models::ReferralDetail>>;

    // ==================== Seasons ====================

    async fn get_active_season(&self) -> Result<Option<Season>>;

    async fn get_season_by_id(&self, id: i32) -> Result<Option<Season>>;

    async fn create_season(&self, season: NewSeason) -> Result<Season>;

    async fn activate_season(&self, season_id: i32) -> Result<()>;

    // ==================== Points Snapshots ====================

    async fn get_snapshot(
        &self,
        address: &str,
        snapshot_date: NaiveDate,
        season_id: i32,
    ) -> Result<Option<PointsSnapshot>>;

    async fn get_user_snapshots(
        &self,
        address: &str,
        season_id: i32,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsSnapshot>>;

    async fn get_latest_snapshot(
        &self,
        address: &str,
        season_id: i32,
    ) -> Result<Option<PointsSnapshot>>;

    async fn get_snapshots_by_date(
        &self,
        snapshot_date: NaiveDate,
        season_id: i32,
    ) -> Result<Vec<PointsSnapshot>>;

    async fn get_user_rank(&self, snapshot: &PointsSnapshot) -> Result<i64>;

    async fn get_leaderboard(
        &self,
        season_id: i32,
        snapshot_date: Option<NaiveDate>,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsSnapshot>>;

    async fn insert_snapshots(&self, snapshots: &[NewPointsSnapshot]) -> Result<()>;
    async fn upsert_snapshot(&self, snapshot: NewPointsSnapshot) -> Result<PointsSnapshot>;

    async fn award_bonus_points(
        &self,
        user_address: &str,
        bonus_amount: i32,
        season_id: i32,
    ) -> Result<()>;
}

pub struct PointsRepository {
    db_pool: Arc<DbPool>,
}

#[cfg_attr(test, automock)]
impl PointsRepository {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl PointsRepositoryTrait for PointsRepository {
    // ==================== Points Config ====================

    async fn get_points_config(&self) -> Result<Vec<PointsConfig>> {
        let mut conn = self.db_pool.get().await?;

        let configs: Vec<PointsConfig> = schema::points_config::table
            .filter(schema::points_config::is_active.eq(true))
            .load(&mut conn)
            .await?;

        Ok(configs)
    }

    async fn get_points_config_for_action(
        &self,
        action_type: &str,
    ) -> Result<Option<PointsConfig>> {
        let mut conn = self.db_pool.get().await?;

        let config: Option<PointsConfig> = schema::points_config::table
            .filter(schema::points_config::action_type.eq(action_type))
            .filter(schema::points_config::is_active.eq(true))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(config)
    }

    async fn insert_points_config(&self, configs: &[NewPointsConfig]) -> Result<()> {
        if configs.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::points_config::table)
            .values(configs)
            .on_conflict(schema::points_config::action_type)
            .do_nothing()
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    // ==================== Points Multipliers ====================

    async fn get_active_multipliers(&self) -> Result<Vec<PointsMultiplier>> {
        let mut conn = self.db_pool.get().await?;

        let multipliers: Vec<PointsMultiplier> = schema::points_multipliers::table
            .filter(schema::points_multipliers::is_active.eq(true))
            .order(schema::points_multipliers::threshold_value.asc())
            .load(&mut conn)
            .await?;

        Ok(multipliers)
    }

    async fn get_multipliers_by_type(
        &self,
        multiplier_type: &str,
    ) -> Result<Vec<PointsMultiplier>> {
        let mut conn = self.db_pool.get().await?;

        let multipliers: Vec<PointsMultiplier> = schema::points_multipliers::table
            .filter(schema::points_multipliers::multiplier_type.eq(multiplier_type))
            .filter(schema::points_multipliers::is_active.eq(true))
            .order(schema::points_multipliers::threshold_value.asc())
            .load(&mut conn)
            .await?;

        Ok(multipliers)
    }

    async fn insert_multipliers(&self, multipliers: &[NewPointsMultiplier]) -> Result<()> {
        if multipliers.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::points_multipliers::table)
            .values(multipliers)
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    // ==================== Referral Codes ====================

    async fn get_referral_code(&self, code: &str) -> Result<Option<ReferralCode>> {
        let mut conn = self.db_pool.get().await?;

        let referral_code: Option<ReferralCode> = schema::referral_codes::table
            .filter(schema::referral_codes::code.eq(code))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(referral_code)
    }

    async fn get_referral_code_by_owner(
        &self,
        owner_address: &str,
    ) -> Result<Option<ReferralCode>> {
        let mut conn = self.db_pool.get().await?;

        let referral_code: Option<ReferralCode> = schema::referral_codes::table
            .filter(schema::referral_codes::owner_address.eq(owner_address))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(referral_code)
    }

    async fn insert_referral_code(&self, code: NewReferralCode) -> Result<ReferralCode> {
        let mut conn = self.db_pool.get().await?;

        let inserted_code: ReferralCode = diesel::insert_into(schema::referral_codes::table)
            .values(&code)
            .get_result(&mut conn)
            .await?;

        Ok(inserted_code)
    }

    async fn get_or_create_referral_code(&self, owner_address: &str) -> Result<String> {
        // Check if user already has a referral code
        if let Some(existing_code) = self.get_referral_code_by_owner(owner_address).await? {
            return Ok(existing_code.code);
        }

        // Try readable codes first (up to 5 attempts)
        for attempt in 0..5 {
            let code = generate_readable_code(owner_address, attempt);

            let new_code =
                NewReferralCode { code: code.clone(), owner_address: owner_address.to_string() };

            match self.insert_referral_code(new_code).await {
                Ok(created_code) => {
                    if attempt > 0 {
                        tracing::info!(
                            "Generated readable referral code after {} attempts",
                            attempt + 1
                        );
                    }
                    return Ok(created_code.code);
                }
                Err(e) => {
                    let error_msg = e.to_string().to_lowercase();
                    if error_msg.contains("unique constraint")
                        || error_msg.contains("duplicate key")
                    {
                        tracing::debug!(
                            "Readable code collision on attempt {}, retrying...",
                            attempt + 1
                        );
                        continue;
                    }
                    return Err(e); // Other error type
                }
            }
        }

        tracing::warn!("Could not generate readable code after 5 attempts, using random fallback");
        loop {
            // Generate random 8-character alphanumeric code
            let code = Alphanumeric.sample_string(&mut rand::rng(), 8).to_uppercase();

            let new_code =
                NewReferralCode { code: code.clone(), owner_address: owner_address.to_string() };

            match self.insert_referral_code(new_code).await {
                Ok(created_code) => {
                    tracing::info!("Generated random referral code as fallback");
                    return Ok(created_code.code);
                }
                Err(e) => {
                    let error_msg = e.to_string().to_lowercase();
                    if error_msg.contains("unique constraint")
                        || error_msg.contains("duplicate key")
                    {
                        continue;
                    }
                    return Err(e); // Other error type
                }
            }
        }
    }

    // ==================== User Referrals ====================

    async fn get_all_user_referrals(&self) -> Result<Vec<UserReferral>> {
        let mut conn = self.db_pool.get().await?;

        let referrals: Vec<UserReferral> = schema::user_referrals::table.load(&mut conn).await?;

        Ok(referrals)
    }

    async fn get_user_referral(&self, user_address: &str) -> Result<Option<UserReferral>> {
        let mut conn = self.db_pool.get().await?;

        let referral: Option<UserReferral> = schema::user_referrals::table
            .filter(schema::user_referrals::user_address.eq(user_address))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(referral)
    }

    async fn insert_user_referral(&self, referral: NewUserReferral) -> Result<UserReferral> {
        let mut conn = self.db_pool.get().await?;

        let inserted_referral: UserReferral = diesel::insert_into(schema::user_referrals::table)
            .values(&referral)
            .get_result(&mut conn)
            .await?;

        Ok(inserted_referral)
    }

    async fn count_referrals_by_address(&self, referred_by_address: &str) -> Result<i64> {
        use crate::schema::user_referrals::dsl;

        let mut conn = self.db_pool.get().await?;

        let count: i64 = dsl::user_referrals
            .filter(dsl::referred_by_address.eq(referred_by_address))
            .count()
            .get_result(&mut conn)
            .await?;

        Ok(count)
    }

    async fn get_referral_summary(
        &self,
        referrer_address: &str,
        season_id: i32,
    ) -> Result<crate::models::ReferralSummary> {
        use crate::schema::user_referrals::dsl;
        use diesel::sql_types::{Integer, Text};

        let mut conn = self.db_pool.get().await?;

        // Count total referrals
        let total_referrals: i64 = dsl::user_referrals
            .filter(dsl::referred_by_address.eq(referrer_address))
            .count()
            .get_result(&mut conn)
            .await?;

        // Get total bonus points from the referrer's latest snapshot
        #[derive(QueryableByName, Debug)]
        struct ReferralPointsResult {
            #[diesel(sql_type = diesel::sql_types::Integer)]
            referral_points: i32,
        }

        let query = diesel::sql_query(
            r#"
            SELECT COALESCE(referral_points, 0) as referral_points
            FROM points_snapshots
            WHERE address = $1 AND season_id = $2
            ORDER BY snapshot_date DESC
            LIMIT 1
            "#,
        )
        .bind::<Text, _>(referrer_address)
        .bind::<Integer, _>(season_id);

        let result: Option<ReferralPointsResult> = query.get_result(&mut conn).await.optional()?;

        let total_bonus_points = result.map(|r| r.referral_points).unwrap_or(0);

        Ok(crate::models::ReferralSummary { total_referrals, total_bonus_points })
    }

    async fn get_referral_details_paginated(
        &self,
        referrer_address: &str,
        season_id: i32,
        page: i64,
        limit: i64,
    ) -> Result<Vec<crate::models::ReferralDetail>> {
        use diesel::sql_types::{BigInt, Integer, Text};

        // TODO: Make referral percentage configurable (currently hardcoded to match config: 5%)
        const REFERRAL_PERCENTAGE: f64 = 0.05;

        let mut conn = self.db_pool.get().await?;

        let offset = page * limit;

        // Get referred users with their latest snapshot's total_points
        #[derive(QueryableByName, Debug)]
        struct ReferralRow {
            #[diesel(sql_type = diesel::sql_types::Text)]
            user_address: String,
            #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Integer>)]
            total_points: Option<i32>,
        }

        let query = diesel::sql_query(
            r#"
            SELECT
                ur.user_address,
                ps.total_points
            FROM user_referrals ur
            LEFT JOIN LATERAL (
                SELECT total_points
                FROM points_snapshots
                WHERE address = ur.user_address AND season_id = $1
                ORDER BY snapshot_date DESC
                LIMIT 1
            ) ps ON true
            WHERE ur.referred_by_address = $2
            ORDER BY COALESCE(ps.total_points, 0) DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind::<Integer, _>(season_id)
        .bind::<Text, _>(referrer_address)
        .bind::<BigInt, _>(limit)
        .bind::<BigInt, _>(offset);

        let rows: Vec<ReferralRow> = query.load(&mut conn).await?;

        // Convert rows to ReferralDetail structs
        // Bonus is percentage of referred user's total points
        let details = rows
            .into_iter()
            .map(|row| {
                let referred_user_total_points = row.total_points.unwrap_or(0);
                let bonus_points_earned =
                    (referred_user_total_points as f64 * REFERRAL_PERCENTAGE) as i32;

                crate::models::ReferralDetail {
                    referred_user_address: row.user_address,
                    bonus_points_earned,
                    referred_user_total_points,
                }
            })
            .collect();

        Ok(details)
    }

    // ==================== Seasons ====================

    async fn get_active_season(&self) -> Result<Option<Season>> {
        let mut conn = self.db_pool.get().await?;

        let season: Option<Season> = schema::points_seasons::table
            .filter(schema::points_seasons::is_active.eq(true))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(season)
    }

    async fn get_season_by_id(&self, id: i32) -> Result<Option<Season>> {
        let mut conn = self.db_pool.get().await?;

        let season: Option<Season> = schema::points_seasons::table
            .filter(schema::points_seasons::id.eq(id))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(season)
    }

    async fn create_season(&self, season: NewSeason) -> Result<Season> {
        let mut conn = self.db_pool.get().await?;

        let created_season: Season = diesel::insert_into(schema::points_seasons::table)
            .values(&season)
            .get_result(&mut conn)
            .await?;

        Ok(created_season)
    }

    async fn activate_season(&self, season_id: i32) -> Result<()> {
        let mut conn = self.db_pool.get().await?;

        // First, deactivate all seasons
        diesel::update(schema::points_seasons::table)
            .set(schema::points_seasons::is_active.eq(false))
            .execute(&mut conn)
            .await?;

        // Then activate the specified season
        diesel::update(schema::points_seasons::table)
            .filter(schema::points_seasons::id.eq(season_id))
            .set(schema::points_seasons::is_active.eq(true))
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    // ==================== Points Snapshots ====================

    async fn get_snapshot(
        &self,
        address: &str,
        snapshot_date: NaiveDate,
        season_id: i32,
    ) -> Result<Option<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshot: Option<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::address.eq(address))
            .filter(schema::points_snapshots::snapshot_date.eq(snapshot_date))
            .filter(schema::points_snapshots::season_id.eq(season_id))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(snapshot)
    }

    async fn get_user_snapshots(
        &self,
        address: &str,
        season_id: i32,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshots: Vec<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::address.eq(address))
            .filter(schema::points_snapshots::season_id.eq(season_id))
            .order(schema::points_snapshots::snapshot_date.desc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;

        Ok(snapshots)
    }

    async fn get_latest_snapshot(
        &self,
        address: &str,
        season_id: i32,
    ) -> Result<Option<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshot: Option<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::address.eq(address))
            .filter(schema::points_snapshots::season_id.eq(season_id))
            .order(schema::points_snapshots::snapshot_date.desc())
            .first(&mut conn)
            .await
            .optional()?;

        Ok(snapshot)
    }

    async fn get_snapshots_by_date(
        &self,
        snapshot_date: NaiveDate,
        season_id: i32,
    ) -> Result<Vec<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshots: Vec<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::snapshot_date.eq(snapshot_date))
            .filter(schema::points_snapshots::season_id.eq(season_id))
            .load(&mut conn)
            .await?;

        Ok(snapshots)
    }

    async fn get_user_rank(&self, snapshot: &PointsSnapshot) -> Result<i64> {
        let mut conn = self.db_pool.get().await?;

        // Count users with higher points on the same date and season (rank = count + 1)
        let count: i64 = schema::points_snapshots::table
            .filter(schema::points_snapshots::snapshot_date.eq(snapshot.snapshot_date))
            .filter(schema::points_snapshots::season_id.eq(snapshot.season_id))
            .filter(schema::points_snapshots::total_points.gt(&snapshot.total_points))
            .count()
            .get_result(&mut conn)
            .await?;

        Ok(count + 1)
    }

    async fn get_leaderboard(
        &self,
        season_id: i32,
        snapshot_date: Option<NaiveDate>,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        // Determine which date to query
        let date_to_query = match snapshot_date {
            Some(date) => date,
            None => {
                // Get the latest snapshot date for this season
                let latest_date: Option<NaiveDate> = schema::points_snapshots::table
                    .select(schema::points_snapshots::snapshot_date)
                    .filter(schema::points_snapshots::season_id.eq(season_id))
                    .order(schema::points_snapshots::snapshot_date.desc())
                    .first(&mut conn)
                    .await
                    .optional()?;

                match latest_date {
                    Some(date) => date,
                    None => return Ok(vec![]), // No snapshots exist for this season
                }
            }
        };

        // Query snapshots for the determined date and season, ordered by total_points
        let snapshots: Vec<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::snapshot_date.eq(date_to_query))
            .filter(schema::points_snapshots::season_id.eq(season_id))
            .order(schema::points_snapshots::total_points.desc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;

        Ok(snapshots)
    }

    async fn insert_snapshots(&self, snapshots: &[NewPointsSnapshot]) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        // Use do_update instead of do_nothing to allow recalculation
        diesel::insert_into(schema::points_snapshots::table)
            .values(snapshots)
            .on_conflict((
                schema::points_snapshots::address,
                schema::points_snapshots::snapshot_date,
                schema::points_snapshots::season_id,
            ))
            .do_update()
            .set((
                schema::points_snapshots::swap_points.eq(diesel::dsl::sql("EXCLUDED.swap_points")),
                schema::points_snapshots::supply_points
                    .eq(diesel::dsl::sql("EXCLUDED.supply_points")),
                schema::points_snapshots::borrow_points
                    .eq(diesel::dsl::sql("EXCLUDED.borrow_points")),
                schema::points_snapshots::base_points_total
                    .eq(diesel::dsl::sql("EXCLUDED.base_points_total")),
                schema::points_snapshots::multiplier_type
                    .eq(diesel::dsl::sql("EXCLUDED.multiplier_type")),
                schema::points_snapshots::multiplier_value
                    .eq(diesel::dsl::sql("EXCLUDED.multiplier_value")),
                schema::points_snapshots::multiplier_points
                    .eq(diesel::dsl::sql("EXCLUDED.multiplier_points")),
                schema::points_snapshots::referral_points
                    .eq(diesel::dsl::sql("EXCLUDED.referral_points")),
                schema::points_snapshots::total_points
                    .eq(diesel::dsl::sql("EXCLUDED.total_points")),
                schema::points_snapshots::total_volume_usd
                    .eq(diesel::dsl::sql("EXCLUDED.total_volume_usd")),
            ))
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    async fn upsert_snapshot(&self, snapshot: NewPointsSnapshot) -> Result<PointsSnapshot> {
        let mut conn = self.db_pool.get().await?;

        let upserted_snapshot: PointsSnapshot =
            diesel::insert_into(schema::points_snapshots::table)
                .values(&snapshot)
                .on_conflict((
                    schema::points_snapshots::address,
                    schema::points_snapshots::snapshot_date,
                ))
                .do_update()
                .set((
                    schema::points_snapshots::swap_points.eq(&snapshot.swap_points),
                    schema::points_snapshots::supply_points.eq(&snapshot.supply_points),
                    schema::points_snapshots::borrow_points.eq(&snapshot.borrow_points),
                    schema::points_snapshots::base_points_total.eq(&snapshot.base_points_total),
                    schema::points_snapshots::multiplier_type.eq(&snapshot.multiplier_type),
                    schema::points_snapshots::multiplier_value.eq(&snapshot.multiplier_value),
                    schema::points_snapshots::multiplier_points.eq(&snapshot.multiplier_points),
                    schema::points_snapshots::referral_points.eq(&snapshot.referral_points),
                    schema::points_snapshots::total_points.eq(&snapshot.total_points),
                    schema::points_snapshots::total_volume_usd.eq(&snapshot.total_volume_usd),
                ))
                .get_result(&mut conn)
                .await?;

        Ok(upserted_snapshot)
    }

    async fn award_bonus_points(
        &self,
        user_address: &str,
        bonus_amount: i32,
        season_id: i32,
    ) -> Result<()> {
        let mut conn = self.db_pool.get().await?;

        // Get user's latest snapshot in current season
        let latest_snapshot: Option<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::address.eq(user_address))
            .filter(schema::points_snapshots::season_id.eq(season_id))
            .order(schema::points_snapshots::snapshot_date.desc())
            .first(&mut conn)
            .await
            .optional()?;

        if let Some(snapshot) = latest_snapshot {
            // Update existing snapshot: add bonus to referral_points and total_points
            let new_referral_points = snapshot.referral_points + bonus_amount;
            let new_total_points = snapshot.total_points + bonus_amount;

            diesel::update(schema::points_snapshots::table)
                .filter(schema::points_snapshots::id.eq(snapshot.id))
                .set((
                    schema::points_snapshots::referral_points.eq(new_referral_points),
                    schema::points_snapshots::total_points.eq(new_total_points),
                ))
                .execute(&mut conn)
                .await?;
        } else {
            // Create new snapshot for yesterday with only the signup bonus
            let yesterday = chrono::Utc::now().date_naive() - chrono::Duration::days(1);

            let new_snapshot = NewPointsSnapshot {
                address: user_address.to_string(),
                snapshot_date: yesterday,
                swap_points: 0,
                supply_points: 0,
                borrow_points: 0,
                base_points_total: 0,
                multiplier_type: None,
                multiplier_value: BigDecimal::from(0),
                multiplier_points: 0,
                referral_points: bonus_amount,
                total_points: bonus_amount,
                total_volume_usd: BigDecimal::from(0),
                season_id,
            };

            diesel::insert_into(schema::points_snapshots::table)
                .values(&new_snapshot)
                .execute(&mut conn)
                .await?;
        }

        Ok(())
    }
}

// ==================== Helper Functions ====================

/// Generate a readable referral code from an address with collision resistance
fn generate_readable_code(address: &str, attempt: u32) -> String {
    use sha2::{Digest, Sha256};

    // Hash the address combined with attempt number for uniqueness
    let mut hasher = Sha256::new();
    hasher.update(address.as_bytes());
    hasher.update(attempt.to_le_bytes());
    let hash = hasher.finalize();

    // Word lists for readable codes
    const ADJECTIVES: &[&str] = &[
        "swift", "bright", "calm", "bold", "wise", "cool", "warm", "dark", "light", "quick",
        "slow", "soft", "hard", "fair", "wild", "tame", "grand", "small", "tall", "short", "long",
        "wide", "thin", "thick", "young", "old", "new", "pure", "clear", "deep", "flat", "steep",
        "sharp", "blunt", "rough", "smooth", "clean", "dirty", "fresh", "stale", "sweet", "sour",
        "spicy", "mild", "hot", "cold", "wet", "dry", "loud", "quiet", "high", "low", "fast",
        "slow", "strong", "weak", "rich", "poor", "full", "empty", "heavy", "light", "tight",
        "loose", "dense", "thin", "solid", "fluid", "stable", "shaky", "firm", "soft", "rigid",
        "flexible", "hard", "gentle", "harsh", "kind", "mean", "nice", "good", "bad", "great",
        "small", "huge", "tiny", "giant", "mini", "super", "ultra", "mega", "micro", "prime",
        "royal", "noble", "humble", "proud", "modest", "brave", "timid", "fierce", "meek", "bold",
        "shy",
    ];

    const NOUNS: &[&str] = &[
        "tiger",
        "moon",
        "wave",
        "peak",
        "star",
        "wind",
        "fire",
        "rain",
        "stone",
        "leaf",
        "cloud",
        "river",
        "ocean",
        "mountain",
        "forest",
        "desert",
        "eagle",
        "wolf",
        "bear",
        "lion",
        "hawk",
        "fox",
        "deer",
        "horse",
        "dragon",
        "phoenix",
        "griffin",
        "falcon",
        "raven",
        "sparrow",
        "owl",
        "swan",
        "thunder",
        "lightning",
        "storm",
        "breeze",
        "gust",
        "gale",
        "mist",
        "fog",
        "sun",
        "comet",
        "planet",
        "galaxy",
        "cosmos",
        "void",
        "abyss",
        "horizon",
        "crystal",
        "diamond",
        "pearl",
        "ruby",
        "sapphire",
        "emerald",
        "jade",
        "opal",
        "shadow",
        "spirit",
        "ghost",
        "soul",
        "heart",
        "mind",
        "dream",
        "vision",
        "sword",
        "shield",
        "arrow",
        "spear",
        "blade",
        "hammer",
        "axe",
        "bow",
        "crown",
        "throne",
        "castle",
        "tower",
        "gate",
        "bridge",
        "path",
        "road",
        "flame",
        "ember",
        "spark",
        "blaze",
        "inferno",
        "torch",
        "beacon",
        "light",
        "frost",
        "ice",
        "snow",
        "winter",
        "spring",
        "summer",
        "autumn",
        "season",
        "dawn",
        "dusk",
        "twilight",
        "midnight",
        "noon",
        "morning",
        "evening",
        "night",
    ];

    // Use hash bytes to deterministically select words
    let adj_index = (hash[0] as usize) % ADJECTIVES.len();
    let noun_index = (hash[1] as usize) % NOUNS.len();

    // Use 2 bytes for number to get range 100-999
    let num = ((hash[2] as u16) << 8 | hash[3] as u16) % 900 + 100;

    format!("{}-{}-{}", ADJECTIVES[adj_index], NOUNS[noun_index], num).to_uppercase()
}

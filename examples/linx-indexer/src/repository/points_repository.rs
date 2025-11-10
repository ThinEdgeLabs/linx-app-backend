use std::sync::Arc;

use anyhow::Result;
use bento_types::DbPool;
use chrono::{NaiveDate, NaiveDateTime};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use diesel_async::RunQueryDsl;

use crate::{
    models::{
        NewPointsConfig, NewPointsMultiplier, NewPointsSnapshot, NewPointsTransaction,
        NewReferralCode, NewUserReferral, PointsConfig, PointsMultiplier, PointsSnapshot,
        PointsTransaction, ReferralCode, UserReferral,
    },
    schema,
};

pub struct PointsRepository {
    db_pool: Arc<DbPool>,
}

impl PointsRepository {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    // ==================== Points Config ====================

    pub async fn get_points_config(&self) -> Result<Vec<PointsConfig>> {
        let mut conn = self.db_pool.get().await?;

        let configs: Vec<PointsConfig> = schema::points_config::table
            .filter(schema::points_config::is_active.eq(true))
            .load(&mut conn)
            .await?;

        Ok(configs)
    }

    pub async fn get_points_config_for_action(
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

    pub async fn insert_points_config(&self, configs: &[NewPointsConfig]) -> Result<()> {
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

    pub async fn get_active_multipliers(&self) -> Result<Vec<PointsMultiplier>> {
        let mut conn = self.db_pool.get().await?;

        let multipliers: Vec<PointsMultiplier> = schema::points_multipliers::table
            .filter(schema::points_multipliers::is_active.eq(true))
            .order(schema::points_multipliers::threshold_value.asc())
            .load(&mut conn)
            .await?;

        Ok(multipliers)
    }

    pub async fn get_multipliers_by_type(
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

    pub async fn insert_multipliers(&self, multipliers: &[NewPointsMultiplier]) -> Result<()> {
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

    pub async fn get_referral_code(&self, code: &str) -> Result<Option<ReferralCode>> {
        let mut conn = self.db_pool.get().await?;

        let referral_code: Option<ReferralCode> = schema::referral_codes::table
            .filter(schema::referral_codes::code.eq(code))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(referral_code)
    }

    pub async fn get_referral_code_by_owner(
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

    pub async fn insert_referral_code(&self, code: NewReferralCode) -> Result<ReferralCode> {
        let mut conn = self.db_pool.get().await?;

        let inserted_code: ReferralCode = diesel::insert_into(schema::referral_codes::table)
            .values(&code)
            .get_result(&mut conn)
            .await?;

        Ok(inserted_code)
    }

    // ==================== User Referrals ====================

    pub async fn get_user_referral(&self, user_address: &str) -> Result<Option<UserReferral>> {
        let mut conn = self.db_pool.get().await?;

        let referral: Option<UserReferral> = schema::user_referrals::table
            .filter(schema::user_referrals::user_address.eq(user_address))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(referral)
    }

    pub async fn get_referrals_for_code(&self, referral_code: &str) -> Result<Vec<UserReferral>> {
        let mut conn = self.db_pool.get().await?;

        let referrals: Vec<UserReferral> = schema::user_referrals::table
            .filter(schema::user_referrals::referral_code.eq(referral_code))
            .load(&mut conn)
            .await?;

        Ok(referrals)
    }

    pub async fn insert_user_referral(&self, referral: NewUserReferral) -> Result<UserReferral> {
        let mut conn = self.db_pool.get().await?;

        let inserted_referral: UserReferral = diesel::insert_into(schema::user_referrals::table)
            .values(&referral)
            .get_result(&mut conn)
            .await?;

        Ok(inserted_referral)
    }

    // ==================== Points Snapshots ====================

    pub async fn get_snapshot(
        &self,
        address: &str,
        snapshot_date: NaiveDate,
    ) -> Result<Option<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshot: Option<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::address.eq(address))
            .filter(schema::points_snapshots::snapshot_date.eq(snapshot_date))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(snapshot)
    }

    pub async fn get_user_snapshots(
        &self,
        address: &str,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshots: Vec<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::address.eq(address))
            .order(schema::points_snapshots::snapshot_date.desc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;

        Ok(snapshots)
    }

    pub async fn get_latest_snapshot(&self, address: &str) -> Result<Option<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshot: Option<PointsSnapshot> = schema::points_snapshots::table
            .filter(schema::points_snapshots::address.eq(address))
            .order(schema::points_snapshots::snapshot_date.desc())
            .first(&mut conn)
            .await
            .optional()?;

        Ok(snapshot)
    }

    pub async fn get_leaderboard(
        &self,
        snapshot_date: Option<NaiveDate>,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let mut query = schema::points_snapshots::table.into_boxed();

        if let Some(date) = snapshot_date {
            query = query.filter(schema::points_snapshots::snapshot_date.eq(date));
        }

        let snapshots: Vec<PointsSnapshot> = query
            .order(schema::points_snapshots::total_points.desc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;

        Ok(snapshots)
    }

    pub async fn insert_snapshots(&self, snapshots: &[NewPointsSnapshot]) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::points_snapshots::table)
            .values(snapshots)
            .on_conflict((
                schema::points_snapshots::address,
                schema::points_snapshots::snapshot_date,
            ))
            .do_nothing()
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn upsert_snapshot(&self, snapshot: NewPointsSnapshot) -> Result<PointsSnapshot> {
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

    // ==================== Points Transactions ====================

    pub async fn get_user_transactions(
        &self,
        address: &str,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsTransaction>> {
        let mut conn = self.db_pool.get().await?;

        let transactions: Vec<PointsTransaction> = schema::points_transactions::table
            .filter(schema::points_transactions::address.eq(address))
            .order(schema::points_transactions::created_at.desc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;

        Ok(transactions)
    }

    pub async fn get_transactions_by_action(
        &self,
        action_type: &str,
        page: i64,
        limit: i64,
    ) -> Result<Vec<PointsTransaction>> {
        let mut conn = self.db_pool.get().await?;

        let transactions: Vec<PointsTransaction> = schema::points_transactions::table
            .filter(schema::points_transactions::action_type.eq(action_type))
            .order(schema::points_transactions::created_at.desc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;

        Ok(transactions)
    }

    pub async fn get_transactions_in_period(
        &self,
        address: &str,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<PointsTransaction>> {
        let mut conn = self.db_pool.get().await?;

        let transactions: Vec<PointsTransaction> = schema::points_transactions::table
            .filter(schema::points_transactions::address.eq(address))
            .filter(schema::points_transactions::created_at.ge(start_time))
            .filter(schema::points_transactions::created_at.lt(end_time))
            .order(schema::points_transactions::created_at.asc())
            .load(&mut conn)
            .await?;

        Ok(transactions)
    }

    pub async fn insert_transactions(&self, transactions: &[NewPointsTransaction]) -> Result<()> {
        if transactions.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::points_transactions::table)
            .values(transactions)
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn insert_transaction(
        &self,
        transaction: NewPointsTransaction,
    ) -> Result<PointsTransaction> {
        let mut conn = self.db_pool.get().await?;

        let inserted_transaction: PointsTransaction =
            diesel::insert_into(schema::points_transactions::table)
                .values(&transaction)
                .get_result(&mut conn)
                .await?;

        Ok(inserted_transaction)
    }
}

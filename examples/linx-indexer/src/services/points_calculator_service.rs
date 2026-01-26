use crate::repository::{
    AccountTransactionRepository, AccountTransactionRepositoryTrait, LendingRepository,
    LendingRepositoryTrait, PointsRepository, PointsRepositoryTrait,
};
use crate::services::price::token_service::{TokenService, TokenServiceTrait};
use anyhow::{Context, Result};
use bento_cli::types::PointsConfig as PointsConfigToml;
use bento_types::DbPool;
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use std::collections::HashMap;
use std::sync::Arc;

pub struct PointsCalculatorService<
    PR = PointsRepository,
    LR = LendingRepository,
    ATR = AccountTransactionRepository,
    PS = TokenService,
> where
    PR: PointsRepositoryTrait,
    LR: LendingRepositoryTrait,
    ATR: AccountTransactionRepositoryTrait,
    PS: TokenServiceTrait,
{
    points_repository: PR,
    lending_repository: LR,
    account_tx_repository: ATR,
    price_service: Arc<PS>,
    config: PointsConfigToml,
}

#[derive(Debug, Clone)]
struct UserDailyActivity {
    address: String,
    swap_points: i32,
    swap_volume_usd: BigDecimal,
    supply_points: i32,
    supply_volume_usd: BigDecimal,
    borrow_points: i32,
    borrow_volume_usd: BigDecimal,
    base_points_total: i32,
    referral_points: i32,
    total_volume_usd: BigDecimal,
}

impl UserDailyActivity {
    fn new(address: String) -> Self {
        Self {
            address,
            swap_points: 0,
            swap_volume_usd: BigDecimal::zero(),
            supply_points: 0,
            supply_volume_usd: BigDecimal::zero(),
            borrow_points: 0,
            borrow_volume_usd: BigDecimal::zero(),
            base_points_total: 0,
            referral_points: 0,
            total_volume_usd: BigDecimal::zero(),
        }
    }

    fn finalize(&mut self) {
        self.base_points_total = self.swap_points + self.supply_points + self.borrow_points;
        self.total_volume_usd =
            &self.swap_volume_usd + &self.supply_volume_usd + &self.borrow_volume_usd;
    }
}

impl PointsCalculatorService {
    pub fn new(
        db_pool: Arc<DbPool>,
        price_service: Arc<TokenService>,
        config: PointsConfigToml,
    ) -> Self {
        Self {
            points_repository: PointsRepository::new(db_pool.clone()),
            lending_repository: LendingRepository::new(db_pool.clone()),
            account_tx_repository: AccountTransactionRepository::new(db_pool.clone()),
            price_service,
            config,
        }
    }
}

impl<PR, LR, ATR, PS> PointsCalculatorService<PR, LR, ATR, PS>
where
    PR: PointsRepositoryTrait,
    LR: LendingRepositoryTrait,
    ATR: AccountTransactionRepositoryTrait,
    PS: TokenServiceTrait,
{
    #[cfg(test)]
    fn new_for_test(
        points_repository: PR,
        lending_repository: LR,
        account_tx_repository: ATR,
        price_service: Arc<PS>,
        config: PointsConfigToml,
    ) -> Self {
        Self { points_repository, lending_repository, account_tx_repository, price_service, config }
    }

    /// Calculate points for a specific date
    pub async fn calculate_points_for_date(&self, date: NaiveDate) -> Result<()> {
        tracing::info!("Starting points calculation for date: {}", date);

        // Fetch active season
        let active_season = self
            .points_repository
            .get_active_season()
            .await?
            .context("No active season found. Cannot calculate points.")?;

        tracing::info!("Using season {} for calculation", active_season.season_number);

        // Load point earning rules from database
        let points_config = self.load_points_config().await?;

        // Collect all user activities for this date
        let mut user_activities = HashMap::new();

        // 1. Calculate swap points
        self.calculate_swap_points(date, &points_config, &mut user_activities).await?;

        // 2. Calculate lending points (supply + borrow)
        self.calculate_lending_points(date, &points_config, &mut user_activities).await?;

        // 4. Finalize base points and volume for each user
        for activity in user_activities.values_mut() {
            activity.finalize();
        }

        // 5. Apply volume multipliers
        let multipliers = self.points_repository.get_multipliers_by_type("volume").await?;
        self.apply_multipliers(&mut user_activities, &multipliers);

        // 6. Calculate referral points
        self.calculate_referral_points(&mut user_activities).await?;

        // 7. Store results
        self.store_snapshots(date, active_season.id, &user_activities).await?;

        tracing::info!("Completed points calculation for date: {}", date);
        Ok(())
    }

    /// Calculate points for a date range
    pub async fn calculate_points_for_range(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<()> {
        let mut current_date = start_date;
        while current_date <= end_date {
            self.calculate_points_for_date(current_date).await?;
            current_date = current_date.succ_opt().context("Failed to get next date")?;
        }
        Ok(())
    }

    /// Run as a daemon, calculating points daily at configured time
    pub async fn run_scheduler(&self) -> Result<()> {
        // Parse configured calculation time (e.g., "01:00")
        let time_parts: Vec<&str> = self.config.calculation_time.split(':').collect();
        let target_hour: u32 = time_parts
            .first()
            .and_then(|h| h.parse().ok())
            .context("Invalid calculation_time hour")?;
        let target_minute: u32 = time_parts
            .get(1)
            .and_then(|m| m.parse().ok())
            .context("Invalid calculation_time minute")?;

        tracing::info!(
            "Points calculator daemon started. Will run daily at {:02}:{:02}",
            target_hour,
            target_minute
        );

        let interval = tokio::time::Duration::from_secs(300); // Check every 5 minutes
        let mut interval_timer = tokio::time::interval(interval);
        let mut last_run_date: Option<NaiveDate> = None;

        loop {
            interval_timer.tick().await;

            let now = chrono::Utc::now().naive_utc();
            let current_date = now.date();
            let current_hour = now.hour();
            let current_minute = now.minute();

            // Check if we're at or past the target time and haven't run today yet
            let should_run = (current_hour > target_hour
                || (current_hour == target_hour && current_minute >= target_minute))
                && last_run_date != Some(current_date);

            if should_run {
                // Calculate for yesterday (complete day)
                let yesterday =
                    current_date.pred_opt().context("Failed to get yesterday's date")?;

                tracing::info!(
                    "Starting scheduled points calculation for {} at {:02}:{:02}...",
                    yesterday,
                    current_hour,
                    current_minute
                );

                if let Err(e) = self.calculate_points_for_date(yesterday).await {
                    tracing::error!("Error during scheduled points calculation: {}", e);
                }

                last_run_date = Some(current_date);
            }
        }
    }

    /// Load point earning rules from database
    async fn load_points_config(&self) -> Result<HashMap<String, crate::models::PointsConfig>> {
        let configs = self.points_repository.get_points_config().await?;
        Ok(configs.into_iter().map(|c| (c.action_type.clone(), c)).collect())
    }

    /// Calculate swap points for all users on a given date
    async fn calculate_swap_points(
        &self,
        date: NaiveDate,
        points_config: &HashMap<String, crate::models::PointsConfig>,
        user_activities: &mut HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        let swap_config = match points_config.get("swap") {
            Some(config) => config,
            None => {
                tracing::warn!("No points config found for 'swap' action, skipping");
                return Ok(());
            }
        };

        let points_per_usd = match &swap_config.points_per_usd {
            Some(ppu) => ppu,
            None => {
                tracing::warn!("No points_per_usd configured for 'swap' action, skipping");
                return Ok(());
            }
        };

        // Get date range (start of day to end of day)
        let start_time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let end_time = NaiveDateTime::new(
            date.succ_opt().context("Failed to get next date")?,
            NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        );

        // Query Linx swaps for this date (only swaps submitted through the Linx App earn points)
        let swaps =
            self.account_tx_repository.get_linx_swaps_in_period(start_time, end_time).await?;

        tracing::info!("Processing {} Linx swaps for points calculation on {}", swaps.len(), date);

        // Accumulate USD volume per user first
        let mut user_swap_volumes: HashMap<String, BigDecimal> = HashMap::new();

        for swap_tx in swaps {
            // Get token info (including decimals and price)
            let token_info = match self.price_service.get_token_info(&swap_tx.token_in).await {
                Ok(info) => info,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get token info for {} in swap {}: {}",
                        swap_tx.token_in,
                        swap_tx.tx_id,
                        e
                    );
                    continue;
                }
            };

            // Convert raw amount to decimal amount using token decimals
            let decimal_amount = token_info.convert_to_decimal(&swap_tx.amount_in);

            // Get token price
            let token_price = match self.price_service.get_token_price(&swap_tx.token_in).await {
                Ok(price) => price,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get price for token {} in swap {}: {}",
                        swap_tx.token_in,
                        swap_tx.tx_id,
                        e
                    );
                    continue;
                }
            };

            // Calculate USD value and accumulate per user
            let amount_usd = &decimal_amount * &token_price;
            *user_swap_volumes
                .entry(swap_tx.address.clone())
                .or_insert_with(BigDecimal::zero) += amount_usd;
        }

        // Calculate points from accumulated USD volumes
        for (address, total_volume_usd) in user_swap_volumes {
            let activity = user_activities
                .entry(address.clone())
                .or_insert_with(|| UserDailyActivity::new(address));

            // Calculate points once from total volume (round up)
            let points_earned_decimal = &total_volume_usd * points_per_usd;
            let points_earned = points_earned_decimal
                .with_scale_round(0, bigdecimal::RoundingMode::Ceiling)
                .to_i32()
                .unwrap_or(0);
            activity.swap_points = points_earned;
            activity.swap_volume_usd = total_volume_usd;
        }

        Ok(())
    }

    /// Calculate lending points (supply + borrow) based on position snapshots
    /// Uses time-weighted calculation: points = amount_usd × points_per_second × time_held
    async fn calculate_lending_points(
        &self,
        date: NaiveDate,
        points_config: &HashMap<String, crate::models::PointsConfig>,
        user_activities: &mut HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        // Get supply and borrow configs
        let supply_config = match points_config.get("supply") {
            Some(config) => config,
            None => {
                tracing::warn!(
                    "No points config found for 'supply' action, skipping lending points"
                );
                return Ok(());
            }
        };

        let borrow_config = match points_config.get("borrow") {
            Some(config) => config,
            None => {
                tracing::warn!(
                    "No points config found for 'borrow' action, skipping lending points"
                );
                return Ok(());
            }
        };

        let supply_points_per_usd_per_day = match &supply_config.points_per_usd_per_day {
            Some(ppu) => ppu,
            None => {
                tracing::warn!(
                    "No points_per_usd_per_day configured for 'supply' action, skipping lending points"
                );
                return Ok(());
            }
        };

        let borrow_points_per_usd_per_day = match &borrow_config.points_per_usd_per_day {
            Some(ppu) => ppu,
            None => {
                tracing::warn!(
                    "No points_per_usd_per_day configured for 'borrow' action, skipping lending points"
                );
                return Ok(());
            }
        };

        // Convert daily rate to per-second rate
        const SECONDS_PER_DAY: i64 = 86400;
        let supply_points_per_usd_per_second =
            supply_points_per_usd_per_day / BigDecimal::from(SECONDS_PER_DAY);
        let borrow_points_per_usd_per_second =
            borrow_points_per_usd_per_day / BigDecimal::from(SECONDS_PER_DAY);

        // Get date range (midnight to midnight)
        let day_start = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let day_end = NaiveDateTime::new(
            date.succ_opt().context("Failed to get next date")?,
            NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        );

        // Query position snapshots for this date
        let snapshots =
            self.lending_repository.get_position_snapshots_in_period(day_start, day_end).await?;

        tracing::info!("Processing {} position snapshots for date {}", snapshots.len(), date);

        // Group snapshots by user
        let mut user_snapshots: HashMap<String, Vec<_>> = HashMap::new();
        for snapshot in snapshots {
            user_snapshots.entry(snapshot.address.clone()).or_default().push(snapshot);
        }

        // Process each user's snapshots
        for (address, mut snapshots) in user_snapshots {
            // Sort snapshots by timestamp (critical for time-weighted calculation)
            snapshots.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            if snapshots.is_empty() {
                continue;
            }

            let activity = user_activities
                .entry(address.clone())
                .or_insert_with(|| UserDailyActivity::new(address.clone()));

            // Track current state per market (market_id -> (supply_usd, borrow_usd))
            let mut market_states: HashMap<String, (BigDecimal, BigDecimal)> = HashMap::new();

            // Accumulate weighted amounts (dollar-seconds) across all periods
            let mut total_supply_usd_seconds = BigDecimal::zero();
            let mut total_borrow_usd_seconds = BigDecimal::zero();

            // Collect all unique timestamps where state changes occur
            let mut timestamps: Vec<NaiveDateTime> =
                snapshots.iter().map(|s| s.timestamp).collect();
            timestamps.sort();
            timestamps.dedup();
            timestamps.push(day_end); // Add end of day as final boundary

            let mut snapshot_idx = 0;

            // Process each time period between consecutive timestamps
            for i in 0..timestamps.len() - 1 {
                let period_start = timestamps[i];
                let period_end = timestamps[i + 1];

                // Apply all snapshots that occur at period_start to update market states
                while snapshot_idx < snapshots.len()
                    && snapshots[snapshot_idx].timestamp == period_start
                {
                    let snap = &snapshots[snapshot_idx];
                    market_states.insert(
                        snap.market_id.clone(),
                        (snap.supply_amount_usd.clone(), snap.borrow_amount_usd.clone()),
                    );
                    snapshot_idx += 1;
                }

                // Calculate aggregate supply and borrow across all markets
                let mut total_supply_usd = BigDecimal::zero();
                let mut total_borrow_usd = BigDecimal::zero();
                for (supply, borrow) in market_states.values() {
                    total_supply_usd += supply;
                    total_borrow_usd += borrow;
                }

                // Calculate duration in seconds
                let duration_seconds = (period_end - period_start).num_seconds();
                if duration_seconds <= 0 {
                    continue;
                }

                // Accumulate weighted amounts (dollar-seconds)
                total_supply_usd_seconds += &total_supply_usd * BigDecimal::from(duration_seconds);
                total_borrow_usd_seconds += &total_borrow_usd * BigDecimal::from(duration_seconds);

                // Track volume (sum of amounts held, not time-weighted)
                activity.supply_volume_usd += &total_supply_usd;
                activity.borrow_volume_usd += &total_borrow_usd;
            }

            // Calculate points from accumulated dollar-seconds
            // points = dollar_seconds × (points_per_usd_per_day / seconds_per_day)
            if !total_supply_usd_seconds.is_zero() {
                let supply_points_decimal =
                    &total_supply_usd_seconds * &supply_points_per_usd_per_second;
                activity.supply_points = supply_points_decimal.round(0).to_i32().unwrap_or(0);
            }

            if !total_borrow_usd_seconds.is_zero() {
                let borrow_points_decimal =
                    &total_borrow_usd_seconds * &borrow_points_per_usd_per_second;
                activity.borrow_points = borrow_points_decimal.round(0).to_i32().unwrap_or(0);
            }
        }

        Ok(())
    }

    /// Apply volume multipliers to user activities
    fn apply_multipliers(
        &self,
        user_activities: &mut HashMap<String, UserDailyActivity>,
        multipliers: &[crate::models::PointsMultiplier],
    ) {
        for activity in user_activities.values_mut() {
            // Find the highest applicable multiplier
            let mut best_multiplier: Option<&crate::models::PointsMultiplier> = None;

            for multiplier in multipliers {
                if activity.total_volume_usd >= multiplier.threshold_value {
                    match best_multiplier {
                        None => best_multiplier = Some(multiplier),
                        Some(current_best) => {
                            if multiplier.threshold_value > current_best.threshold_value {
                                best_multiplier = Some(multiplier);
                            }
                        }
                    }
                }
            }

            // Apply the multiplier if found
            if let Some(multiplier) = best_multiplier {
                // Convert base_points_total to BigDecimal, multiply, then round back to i32
                let base_points_bd = BigDecimal::from(activity.base_points_total);
                let multiplier_points_decimal = &base_points_bd * &multiplier.multiplier;
                let multiplier_points = multiplier_points_decimal.round(0).to_i32().unwrap_or(0);
                activity.base_points_total += multiplier_points;
            }
        }
    }

    /// Calculate referral points for referrers
    async fn calculate_referral_points(
        &self,
        user_activities: &mut HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        let referral_percentage = BigDecimal::try_from(self.config.referral_percentage)
            .context("Failed to convert referral_percentage to BigDecimal")?;

        // Get all user referrals
        let all_referrals = self.points_repository.get_all_user_referrals().await?;

        // Group referrals by referrer
        let mut referrals_by_referrer: HashMap<String, Vec<String>> = HashMap::new();
        for referral in all_referrals {
            referrals_by_referrer
                .entry(referral.referred_by_address.clone())
                .or_default()
                .push(referral.user_address);
        }

        // Calculate referral points for each referrer
        for (referrer_address, referred_users) in referrals_by_referrer {
            let mut total_referred_points = 0i32;

            for referred_user in referred_users {
                if let Some(referred_activity) = user_activities.get(&referred_user) {
                    // Only count users with actual activity (swaps or lending)
                    // This prevents exploitation via fake accounts that only claim signup bonus
                    let has_activity = referred_activity.swap_points > 0
                        || referred_activity.supply_points > 0
                        || referred_activity.borrow_points > 0;

                    if has_activity {
                        total_referred_points += referred_activity.base_points_total;
                    }
                }
            }

            // Calculate referral points as percentage of total referred points
            let total_referred_bd = BigDecimal::from(total_referred_points);
            let referral_points_decimal = total_referred_bd * &referral_percentage;
            let referral_points = referral_points_decimal.round(0).to_i32().unwrap_or(0);

            // Add referral points to referrer
            let referrer_activity = user_activities
                .entry(referrer_address.clone())
                .or_insert_with(|| UserDailyActivity::new(referrer_address.clone()));

            referrer_activity.referral_points += referral_points;
        }

        Ok(())
    }

    /// Store daily snapshots in database
    /// Creates snapshots for all users:
    /// - Users with activity today: new snapshot with updated points
    /// - Users without activity today: copy of their latest snapshot with same total_points
    async fn store_snapshots(
        &self,
        date: NaiveDate,
        season_id: i32,
        user_activities: &HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        use crate::models::NewPointsSnapshot;

        let mut snapshots = Vec::new();

        // Get the previous day's date to fetch previous snapshots
        let previous_date = date.pred_opt().context("Failed to get previous date")?;

        // Step 1: Create snapshots for users with activity today
        for activity in user_activities.values() {
            // Fetch the previous day's snapshot to get cumulative total
            let previous_total = match self
                .points_repository
                .get_snapshot(&activity.address, previous_date, season_id)
                .await?
            {
                Some(prev_snapshot) => prev_snapshot.total_points,
                None => 0, // No previous snapshot means this is their first day
            };

            // Calculate cumulative total: previous total + today's points (base + referral)
            let cumulative_total =
                previous_total + activity.base_points_total + activity.referral_points;

            snapshots.push(NewPointsSnapshot {
                address: activity.address.clone(),
                snapshot_date: date,
                swap_points: activity.swap_points,
                supply_points: activity.supply_points,
                borrow_points: activity.borrow_points,
                base_points_total: activity.base_points_total,
                multiplier_type: None, // TODO: Track which multiplier was applied
                multiplier_value: BigDecimal::zero(),
                multiplier_points: 0,
                referral_points: activity.referral_points,
                total_points: cumulative_total,
                total_volume_usd: activity.total_volume_usd.clone(),
                season_id,
            });
        }

        // Step 2: Get all users who had snapshots on the previous day but no activity today
        let all_previous_snapshots =
            self.points_repository.get_snapshots_by_date(previous_date, season_id).await?;

        for prev_snapshot in all_previous_snapshots {
            // Skip users who already have activity today
            if user_activities.contains_key(&prev_snapshot.address) {
                continue;
            }

            // Copy the previous snapshot with the new date (no new points earned)
            snapshots.push(NewPointsSnapshot {
                address: prev_snapshot.address,
                snapshot_date: date,
                swap_points: 0, // No new activity today
                supply_points: 0,
                borrow_points: 0,
                base_points_total: 0,
                multiplier_type: prev_snapshot.multiplier_type,
                multiplier_value: prev_snapshot.multiplier_value,
                multiplier_points: 0,
                referral_points: 0,
                total_points: prev_snapshot.total_points, // Carry forward cumulative total
                total_volume_usd: BigDecimal::zero(),     // No new volume today
                season_id,
            });
        }

        self.points_repository.insert_snapshots(&snapshots).await?;

        tracing::info!(
            "Stored {} snapshots for date {} ({} with activity, {} carried forward)",
            snapshots.len(),
            date,
            user_activities.len(),
            snapshots.len() - user_activities.len()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        MockAccountTransactionRepositoryTrait, MockLendingRepositoryTrait,
        MockPointsRepositoryTrait,
    };
    use crate::services::price::token_service::MockTokenServiceTrait;
    use bigdecimal::BigDecimal;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    use mockall::predicate::*;
    use std::str::FromStr;

    // Test helpers and builders
    struct TestSetup {
        points_repo: MockPointsRepositoryTrait,
        lending_repo: MockLendingRepositoryTrait,
        account_repo: MockAccountTransactionRepositoryTrait,
        price_service: MockTokenServiceTrait,
    }

    impl TestSetup {
        fn new() -> Self {
            Self {
                points_repo: MockPointsRepositoryTrait::new(),
                lending_repo: MockLendingRepositoryTrait::new(),
                account_repo: MockAccountTransactionRepositoryTrait::new(),
                price_service: MockTokenServiceTrait::new(),
            }
        }

        fn with_no_swaps(mut self) -> Self {
            self.account_repo.expect_get_linx_swaps_in_period().returning(|_, _| Ok(vec![]));
            self
        }

        fn with_no_multipliers(mut self) -> Self {
            self.points_repo.expect_get_multipliers_by_type().returning(|_| Ok(vec![]));
            self
        }

        fn with_no_referrals(mut self) -> Self {
            self.points_repo.expect_get_all_user_referrals().returning(|| Ok(vec![]));
            self
        }

        fn with_no_previous_snapshots(mut self) -> Self {
            self.points_repo.expect_get_snapshot().returning(|_, _, _| Ok(None));
            self.points_repo.expect_get_snapshots_by_date().returning(|_, _| Ok(vec![]));
            self
        }

        fn with_active_season(mut self, season_id: i32) -> Self {
            use crate::models::Season;
            use chrono::NaiveDate;

            self.points_repo.expect_get_active_season().returning(move || {
                Ok(Some(Season {
                    id: season_id,
                    season_number: 1,
                    start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
                    max_tokens_distribution: BigDecimal::from_str("1000000").unwrap(),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                }))
            });
            self
        }

        fn expect_snapshots<F>(mut self, validator: F) -> Self
        where
            F: Fn(&[crate::models::NewPointsSnapshot]) -> bool + Send + 'static,
        {
            self.points_repo.expect_insert_snapshots().withf(validator).returning(|_| Ok(()));
            self
        }

        fn build(
            self,
        ) -> PointsCalculatorService<
            MockPointsRepositoryTrait,
            MockLendingRepositoryTrait,
            MockAccountTransactionRepositoryTrait,
            MockTokenServiceTrait,
        > {
            let config = PointsConfigToml {
                referral_percentage: 0.05,
                signup_bonus: 1000,
                calculation_time: "01:00".to_string(),
            };

            PointsCalculatorService::new_for_test(
                self.points_repo,
                self.lending_repo,
                self.account_repo,
                Arc::new(self.price_service),
                config,
            )
        }
    }

    // ==================== Time-Weighted Lending Points Tests ====================

    fn position_snapshot(
        address: &str,
        market_id: &str,
        supply_usd: &str,
        borrow_usd: &str,
        timestamp: NaiveDateTime,
    ) -> crate::models::PositionSnapshot {
        crate::models::PositionSnapshot {
            id: 1,
            address: address.to_string(),
            market_id: market_id.to_string(),
            supply_amount: BigDecimal::zero(),
            supply_amount_usd: BigDecimal::from_str(supply_usd).unwrap(),
            borrow_amount: BigDecimal::zero(),
            borrow_amount_usd: BigDecimal::from_str(borrow_usd).unwrap(),
            timestamp,
        }
    }

    #[tokio::test]
    async fn test_lending_single_snapshot_full_day() {
        // Scenario: User has one snapshot for the entire day (00:05)
        // Position: $1000 supplied, $0 borrowed
        // Config: 10 points per USD per day for supply
        // Expected: $1000 × (10/86400) × 86100 = 9,965 points
        // (snapshot at 00:05 held until end of day = 86,100 seconds out of 86,400)

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let snapshot_time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 5, 0).unwrap());

        let snapshots = vec![position_snapshot("user1", "market1", "1000", "0", snapshot_time)];

        let mut setup = TestSetup::new();
        setup.points_repo.expect_get_points_config().returning(|| {
            Ok(vec![
                crate::models::PointsConfig {
                    id: 1,
                    action_type: "supply".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("10").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
                crate::models::PointsConfig {
                    id: 2,
                    action_type: "borrow".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("5").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
            ])
        });

        setup
            .lending_repo
            .expect_get_position_snapshots_in_period()
            .returning(move |_, _| Ok(snapshots.clone()));

        let service = setup
            .with_no_swaps()
            .with_no_multipliers()
            .with_no_referrals()
            .with_no_previous_snapshots()
            .with_active_season(1)
            .expect_snapshots(|snapshots| {
                if let Some(s) = snapshots.iter().find(|s| s.address == "user1") {
                    s.supply_points == 9965 && s.borrow_points == 0 && s.season_id == 1
                } else {
                    false
                }
            })
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lending_multiple_snapshots_time_weighted() {
        // Scenario: User has multiple snapshots throughout the day
        // 00:05-06:00 (5h55m = 21300s): $1000 supplied
        // 06:00-12:00 (6h = 21600s): $2000 supplied (doubled position)
        // 12:00-24:00 (12h = 43200s): $500 supplied (reduced position)
        // Config: 10 points per USD per day = 10/86400 per second
        // Expected: (1000×10/86400×21300) + (2000×10/86400×21600) + (500×10/86400×43200)
        //         = 2465 + 5000 + 2500 = 9,965 points

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let snapshot1 = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 5, 0).unwrap());
        let snapshot2 = NaiveDateTime::new(date, NaiveTime::from_hms_opt(6, 0, 0).unwrap());
        let snapshot3 = NaiveDateTime::new(date, NaiveTime::from_hms_opt(12, 0, 0).unwrap());

        let snapshots = vec![
            position_snapshot("user1", "market1", "1000", "0", snapshot1),
            position_snapshot("user1", "market1", "2000", "0", snapshot2),
            position_snapshot("user1", "market1", "500", "0", snapshot3),
        ];

        let mut setup = TestSetup::new();
        setup.points_repo.expect_get_points_config().returning(|| {
            Ok(vec![
                crate::models::PointsConfig {
                    id: 1,
                    action_type: "supply".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("10").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
                crate::models::PointsConfig {
                    id: 2,
                    action_type: "borrow".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("5").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
            ])
        });

        setup
            .lending_repo
            .expect_get_position_snapshots_in_period()
            .returning(move |_, _| Ok(snapshots.clone()));

        let service = setup
            .with_no_swaps()
            .with_no_multipliers()
            .with_no_referrals()
            .with_no_previous_snapshots()
            .with_active_season(1)
            .expect_snapshots(|snapshots| {
                if let Some(s) = snapshots.iter().find(|s| s.address == "user1") {
                    s.supply_points == 9965 && s.season_id == 1
                } else {
                    false
                }
            })
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lending_supply_and_borrow_combined() {
        // Scenario: User has both supply and borrow positions
        // Single snapshot at 00:05 held until end of day (86100 seconds)
        // Position: $1000 supplied, $500 borrowed
        // Config: 10 points/USD/day for supply, 5 points/USD/day for borrow
        // Expected: Supply: 1000×(10/86400)×86100 = 9965, Borrow: 500×(5/86400)×86100 = 2491, Total: 12456

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let snapshot_time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 5, 0).unwrap());

        let snapshots = vec![position_snapshot("user1", "market1", "1000", "500", snapshot_time)];

        let mut setup = TestSetup::new();
        setup.points_repo.expect_get_points_config().returning(|| {
            Ok(vec![
                crate::models::PointsConfig {
                    id: 1,
                    action_type: "supply".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("10").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
                crate::models::PointsConfig {
                    id: 2,
                    action_type: "borrow".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("5").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
            ])
        });

        setup
            .lending_repo
            .expect_get_position_snapshots_in_period()
            .returning(move |_, _| Ok(snapshots.clone()));

        let service = setup
            .with_no_swaps()
            .with_no_multipliers()
            .with_no_referrals()
            .with_no_previous_snapshots()
            .with_active_season(1)
            .expect_snapshots(|snapshots| {
                if let Some(s) = snapshots.iter().find(|s| s.address == "user1") {
                    s.supply_points == 9965
                        && s.borrow_points == 2491
                        && s.base_points_total == 12456
                        && s.season_id == 1
                } else {
                    false
                }
            })
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lending_position_closed_mid_day() {
        // Scenario: User closes position halfway through the day
        // 00:05-12:00 (11h55m = 42900s): $1000 supplied
        // 12:00-24:00: $0 supplied (position closed, no points earned)
        // Config: 10 points per USD per day
        // Expected: Only earn for 00:05-12:00 = 1000×(10/86400)×42900 = 4,965 points

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let snapshot1 = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 5, 0).unwrap());
        let snapshot2 = NaiveDateTime::new(date, NaiveTime::from_hms_opt(12, 0, 0).unwrap());

        let snapshots = vec![
            position_snapshot("user1", "market1", "1000", "0", snapshot1),
            position_snapshot("user1", "market1", "0", "0", snapshot2), // Position closed
        ];

        let mut setup = TestSetup::new();
        setup.points_repo.expect_get_points_config().returning(|| {
            Ok(vec![
                crate::models::PointsConfig {
                    id: 1,
                    action_type: "supply".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("10").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
                crate::models::PointsConfig {
                    id: 2,
                    action_type: "borrow".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("5").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
            ])
        });

        setup
            .lending_repo
            .expect_get_position_snapshots_in_period()
            .returning(move |_, _| Ok(snapshots.clone()));

        let service = setup
            .with_no_swaps()
            .with_no_multipliers()
            .with_no_referrals()
            .with_no_previous_snapshots()
            .with_active_season(1)
            .expect_snapshots(|snapshots| {
                if let Some(s) = snapshots.iter().find(|s| s.address == "user1") {
                    s.supply_points == 4965 && s.borrow_points == 0 && s.season_id == 1
                } else {
                    false
                }
            })
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lending_multiple_users() {
        // Scenario: Two users with different positions
        // User1: $1000 supplied from 00:05 until end of day (86100 seconds)
        // User2: $500 borrowed from 00:05 until end of day (86100 seconds)
        // Config: 10 points/USD/day supply, 5 points/USD/day borrow
        // Expected: User1: 1000×(10/86400)×86100 = 9,965 supply points
        //           User2: 500×(5/86400)×86100 = 2,491 borrow points

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let snapshot_time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 5, 0).unwrap());

        let snapshots = vec![
            position_snapshot("user1", "market1", "1000", "0", snapshot_time),
            position_snapshot("user2", "market1", "0", "500", snapshot_time),
        ];

        let mut setup = TestSetup::new();
        setup.points_repo.expect_get_points_config().returning(|| {
            Ok(vec![
                crate::models::PointsConfig {
                    id: 1,
                    action_type: "supply".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("10").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
                crate::models::PointsConfig {
                    id: 2,
                    action_type: "borrow".to_string(),
                    points_per_usd: None,
                    points_per_usd_per_day: Some(BigDecimal::from_str("5").unwrap()),
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
            ])
        });

        setup
            .lending_repo
            .expect_get_position_snapshots_in_period()
            .returning(move |_, _| Ok(snapshots.clone()));

        let service = setup
            .with_no_swaps()
            .with_no_multipliers()
            .with_no_referrals()
            .with_no_previous_snapshots()
            .with_active_season(1)
            .expect_snapshots(|snapshots| {
                let user1 = snapshots.iter().find(|s| s.address == "user1");
                let user2 = snapshots.iter().find(|s| s.address == "user2");

                if let (Some(u1), Some(u2)) = (user1, user2) {
                    u1.supply_points == 9965
                        && u1.borrow_points == 0
                        && u2.supply_points == 0
                        && u2.borrow_points == 2491
                } else {
                    false
                }
            })
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }
}

use crate::models::LendingEvent;
use crate::repository::{
    AccountTransactionRepository, AccountTransactionRepositoryTrait, LendingRepository,
    LendingRepositoryTrait, PointsRepository, PointsRepositoryTrait,
};
use crate::services::price::token_service::{TokenService, TokenServiceTrait};
use anyhow::{Context, Result};
use bento_cli::types::PointsConfig as PointsConfigToml;
use bento_types::DbPool;
use bigdecimal::{BigDecimal, Zero};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
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
    swap_points: BigDecimal,
    swap_volume_usd: BigDecimal,
    supply_points: BigDecimal,
    supply_volume_usd: BigDecimal,
    borrow_points: BigDecimal,
    borrow_volume_usd: BigDecimal,
    base_points_total: BigDecimal,
    total_volume_usd: BigDecimal,
    transactions: Vec<TransactionDetail>,
}

#[derive(Debug, Clone)]
struct TransactionDetail {
    action_type: String,
    transaction_id: Option<String>,
    amount_usd: BigDecimal,
    points_earned: BigDecimal,
}

impl UserDailyActivity {
    fn new(address: String) -> Self {
        Self {
            address,
            swap_points: BigDecimal::zero(),
            swap_volume_usd: BigDecimal::zero(),
            supply_points: BigDecimal::zero(),
            supply_volume_usd: BigDecimal::zero(),
            borrow_points: BigDecimal::zero(),
            borrow_volume_usd: BigDecimal::zero(),
            base_points_total: BigDecimal::zero(),
            total_volume_usd: BigDecimal::zero(),
            transactions: Vec::new(),
        }
    }

    fn finalize(&mut self) {
        self.base_points_total = &self.swap_points + &self.supply_points + &self.borrow_points;
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

        // Load point earning rules from database
        let points_config = self.load_points_config().await?;

        // Collect all user activities for this date
        let mut user_activities = HashMap::new();

        // 1. Calculate swap points
        self.calculate_swap_points(date, &points_config, &mut user_activities).await?;

        // 2. Calculate supply points
        // self.calculate_supply_points(date, &points_config, &mut user_activities).await?;

        // 3. Calculate borrow points
        self.calculate_borrow_points(date, &points_config, &mut user_activities).await?;

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
        self.store_snapshots(date, &user_activities).await?;
        self.store_transactions(date, &user_activities).await?;

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
        let interval = tokio::time::Duration::from_secs(300); // Check every 5 minutes
        let mut interval_timer = tokio::time::interval(interval);

        loop {
            interval_timer.tick().await;

            // Calculate for yesterday
            let yesterday = chrono::Utc::now()
                .date_naive()
                .pred_opt()
                .context("Failed to get yesterday's date")?;

            tracing::info!("Starting scheduled points calculation for {}...", yesterday);
            if let Err(e) = self.calculate_points_for_date(yesterday).await {
                tracing::error!("Error during scheduled points calculation: {}", e);
            } else {
                tracing::info!("Scheduled points calculation completed successfully.");
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

        // Query swaps for this date
        let swaps = self.account_tx_repository.get_swaps_in_period(start_time, end_time).await?;

        tracing::info!("Processing {} swaps for date {}", swaps.len(), date);

        for swap_tx in swaps {
            // Get token info (including decimals and price)
            let token_info = match self.price_service.get_token_info(&swap_tx.swap.token_in).await {
                Ok(info) => info,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get token info for {} in swap {}: {}",
                        swap_tx.swap.token_in,
                        swap_tx.swap.tx_id,
                        e
                    );
                    continue;
                }
            };

            // Convert raw amount to decimal amount using token decimals
            let decimal_amount = token_info.convert_to_decimal(&swap_tx.swap.amount_in);

            // Get token price
            let token_price = match self.price_service.get_token_price(&swap_tx.swap.token_in).await
            {
                Ok(price) => price,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get price for token {} in swap {}: {}",
                        swap_tx.swap.token_in,
                        swap_tx.swap.tx_id,
                        e
                    );
                    continue;
                }
            };

            // Calculate USD value
            let amount_usd = &decimal_amount * &token_price;

            // Calculate points
            let points_earned = &amount_usd * points_per_usd;

            // Get or create user activity
            let activity =
                user_activities.entry(swap_tx.account_transaction.address.clone()).or_insert_with(
                    || UserDailyActivity::new(swap_tx.account_transaction.address.clone()),
                );

            activity.swap_points += &points_earned;
            activity.swap_volume_usd += &amount_usd;
            activity.transactions.push(TransactionDetail {
                action_type: "swap".to_string(),
                transaction_id: Some(swap_tx.swap.tx_id),
                amount_usd,
                points_earned,
            });
        }

        Ok(())
    }

    /// Calculate supply points based on daily snapshots
    async fn calculate_supply_points(
        &self,
        date: NaiveDate,
        points_config: &HashMap<String, crate::models::PointsConfig>,
        user_activities: &mut HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        let supply_config = match points_config.get("supply") {
            Some(config) => config,
            None => {
                tracing::warn!("No points config found for 'supply' action, skipping");
                return Ok(());
            }
        };

        let points_per_usd_per_day = match &supply_config.points_per_usd_per_day {
            Some(ppu) => ppu,
            None => {
                tracing::warn!(
                    "No points_per_usd_per_day configured for 'supply' action, skipping"
                );
                return Ok(());
            }
        };

        // Get date range
        let start_time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let end_time = NaiveDateTime::new(
            date.succ_opt().context("Failed to get next date")?,
            NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        );

        // Query deposit snapshots for this date
        let snapshots =
            self.lending_repository.get_deposit_snapshots_in_period(start_time, end_time).await?;

        tracing::info!("Processing {} deposit snapshots for date {}", snapshots.len(), date);

        for snapshot in snapshots {
            // Use amount_usd from snapshot
            let amount_usd = snapshot.amount_usd;

            // Calculate points
            let points_earned = &amount_usd * points_per_usd_per_day;

            // Get or create user activity
            let activity = user_activities
                .entry(snapshot.address.clone())
                .or_insert_with(|| UserDailyActivity::new(snapshot.address.clone()));

            activity.supply_points += &points_earned;
            activity.supply_volume_usd += &amount_usd;
            activity.transactions.push(TransactionDetail {
                action_type: "supply".to_string(),
                transaction_id: None, // Snapshots don't have a single tx_id
                amount_usd,
                points_earned,
            });
        }

        Ok(())
    }

    /// Calculate borrow points from lending events
    async fn calculate_borrow_points(
        &self,
        date: NaiveDate,
        points_config: &HashMap<String, crate::models::PointsConfig>,
        user_activities: &mut HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        let borrow_config = match points_config.get("borrow") {
            Some(config) => config,
            None => {
                tracing::warn!("No points config found for 'borrow' action, skipping");
                return Ok(());
            }
        };

        let points_per_usd = match &borrow_config.points_per_usd {
            Some(ppu) => ppu,
            None => {
                tracing::warn!("No points_per_usd configured for 'borrow' action, skipping");
                return Ok(());
            }
        };

        // Get date range
        let start_time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let end_time = NaiveDateTime::new(
            date.succ_opt().context("Failed to get next date")?,
            NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        );

        // Query borrow events for this date
        let events: Vec<LendingEvent> =
            self.lending_repository.get_borrow_events_in_period(start_time, end_time).await?;

        tracing::info!("Processing {} borrow events for date {}", events.len(), date);

        for event in events {
            // Get token info (including decimals and price)
            let token_info = match self.price_service.get_token_info(&event.token_id).await {
                Ok(info) => info,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get token info for {} in borrow event {}: {}",
                        event.token_id,
                        event.transaction_id,
                        e
                    );
                    continue;
                }
            };

            // Convert raw amount to decimal amount using token decimals
            let decimal_amount = token_info.convert_to_decimal(&event.amount);

            // Get token price
            let token_price = match self.price_service.get_token_price(&event.token_id).await {
                Ok(price) => price,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get price for token {} in borrow event {}: {}",
                        event.token_id,
                        event.transaction_id,
                        e
                    );
                    continue;
                }
            };

            // Calculate USD value
            let amount_usd = &decimal_amount * &token_price;

            // Calculate points
            let points_earned = &amount_usd * points_per_usd;

            // Get or create user activity
            let activity = user_activities
                .entry(event.on_behalf.clone())
                .or_insert_with(|| UserDailyActivity::new(event.on_behalf.clone()));

            activity.borrow_points += &points_earned;
            activity.borrow_volume_usd += &amount_usd;
            activity.transactions.push(TransactionDetail {
                action_type: "borrow".to_string(),
                transaction_id: Some(event.transaction_id),
                amount_usd,
                points_earned,
            });
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
                let multiplier_points = &activity.base_points_total * &multiplier.multiplier;
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
                .or_insert_with(Vec::new)
                .push(referral.user_address);
        }

        // Calculate referral points for each referrer
        for (referrer_address, referred_users) in referrals_by_referrer {
            let mut total_referred_points = BigDecimal::zero();

            for referred_user in referred_users {
                if let Some(referred_activity) = user_activities.get(&referred_user) {
                    total_referred_points += &referred_activity.base_points_total;
                }
            }

            let referral_points = total_referred_points * &referral_percentage;

            // Add referral points to referrer
            let referrer_activity = user_activities
                .entry(referrer_address.clone())
                .or_insert_with(|| UserDailyActivity::new(referrer_address.clone()));

            referrer_activity.base_points_total += &referral_points;
        }

        Ok(())
    }

    /// Store daily snapshots in database
    async fn store_snapshots(
        &self,
        date: NaiveDate,
        user_activities: &HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        use crate::models::NewPointsSnapshot;

        let mut snapshots = Vec::new();

        for activity in user_activities.values() {
            snapshots.push(NewPointsSnapshot {
                address: activity.address.clone(),
                snapshot_date: date,
                swap_points: activity.swap_points.clone(),
                supply_points: activity.supply_points.clone(),
                borrow_points: activity.borrow_points.clone(),
                base_points_total: activity.base_points_total.clone(),
                multiplier_type: None, // TODO: Track which multiplier was applied
                multiplier_value: BigDecimal::zero(),
                multiplier_points: BigDecimal::zero(),
                referral_points: BigDecimal::zero(), // TODO: Track referral points separately
                total_points: activity.base_points_total.clone(),
                total_volume_usd: activity.total_volume_usd.clone(),
            });
        }

        self.points_repository.insert_snapshots(&snapshots).await?;

        tracing::info!("Stored {} snapshots for date {}", snapshots.len(), date);
        Ok(())
    }

    /// Store transaction details in database
    async fn store_transactions(
        &self,
        date: NaiveDate,
        user_activities: &HashMap<String, UserDailyActivity>,
    ) -> Result<()> {
        use crate::models::NewPointsTransaction;

        // Delete existing transactions for this date to allow recalculation
        let deleted_count = self.points_repository.delete_transactions_by_date(date).await?;
        if deleted_count > 0 {
            tracing::info!("Deleted {} existing transactions for date {}", deleted_count, date);
        }

        let mut transactions = Vec::new();

        for activity in user_activities.values() {
            for tx_detail in &activity.transactions {
                transactions.push(NewPointsTransaction {
                    address: activity.address.clone(),
                    action_type: tx_detail.action_type.clone(),
                    transaction_id: tx_detail.transaction_id.clone(),
                    amount_usd: tx_detail.amount_usd.clone(),
                    points_earned: tx_detail.points_earned.clone(),
                    snapshot_date: date,
                });
            }
        }

        self.points_repository.insert_transactions(&transactions).await?;

        tracing::info!("Stored {} transaction details for date {}", transactions.len(), date);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountTransaction, DepositSnapshot, LendingEvent, SwapTransactionDto};
    use crate::repository::{
        MockAccountTransactionRepositoryTrait, MockLendingRepositoryTrait,
        MockPointsRepositoryTrait,
    };
    use crate::services::price::linx_price_service::TokenInfo;
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

        fn with_borrow_config(mut self, points_per_usd: &str) -> Self {
            let points_per_usd = points_per_usd.to_string();
            self.points_repo.expect_get_points_config().returning(move || {
                Ok(vec![crate::models::PointsConfig {
                    id: 1,
                    action_type: "borrow".to_string(),
                    points_per_usd: Some(BigDecimal::from_str(&points_per_usd).unwrap()),
                    points_per_usd_per_day: None,
                    is_active: true,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                }])
            });
            self
        }

        fn with_swap_and_borrow_config(mut self, swap_ppu: &str, borrow_ppu: &str) -> Self {
            let swap_ppu = swap_ppu.to_string();
            let borrow_ppu = borrow_ppu.to_string();
            self.points_repo.expect_get_points_config().returning(move || {
                Ok(vec![
                    crate::models::PointsConfig {
                        id: 1,
                        action_type: "swap".to_string(),
                        points_per_usd: Some(BigDecimal::from_str(&swap_ppu).unwrap()),
                        points_per_usd_per_day: None,
                        is_active: true,
                        created_at: chrono::Utc::now().naive_utc(),
                        updated_at: chrono::Utc::now().naive_utc(),
                    },
                    crate::models::PointsConfig {
                        id: 2,
                        action_type: "borrow".to_string(),
                        points_per_usd: Some(BigDecimal::from_str(&borrow_ppu).unwrap()),
                        points_per_usd_per_day: None,
                        is_active: true,
                        created_at: chrono::Utc::now().naive_utc(),
                        updated_at: chrono::Utc::now().naive_utc(),
                    },
                ])
            });
            self
        }

        fn with_swaps(mut self, swaps: Vec<SwapTransactionDto>) -> Self {
            self.account_repo.expect_get_swaps_in_period().returning(move |_, _| Ok(swaps.clone()));
            self
        }

        fn with_swaps_checked_dates(
            mut self,
            expected_start: NaiveDateTime,
            expected_end: NaiveDateTime,
            swaps: Vec<SwapTransactionDto>,
        ) -> Self {
            self.account_repo
                .expect_get_swaps_in_period()
                .with(eq(expected_start), eq(expected_end))
                .returning(move |_, _| Ok(swaps.clone()));
            self
        }

        fn with_borrow_events(mut self, events: Vec<LendingEvent>) -> Self {
            self.lending_repo
                .expect_get_borrow_events_in_period()
                .returning(move |_, _| Ok(events.clone()));
            self
        }

        fn with_borrow_events_checked_dates(
            mut self,
            expected_start: NaiveDateTime,
            expected_end: NaiveDateTime,
            events: Vec<LendingEvent>,
        ) -> Self {
            self.lending_repo
                .expect_get_borrow_events_in_period()
                .with(eq(expected_start), eq(expected_end))
                .returning(move |_, _| Ok(events.clone()));
            self
        }

        fn with_token_price(mut self, token_id: &'static str, price: &str) -> Self {
            let price_value = BigDecimal::from_str(price).unwrap();
            self.price_service
                .expect_get_token_price()
                .with(eq(token_id))
                .returning(move |_| Ok(price_value.clone()));
            self
        }

        fn with_token_info(mut self, token_id: &'static str, decimals: u8, price: &str) -> Self {
            let token_id_owned = token_id.to_string();
            let price_value = price.parse::<f64>().unwrap();

            // Mock get_token_info - allow multiple calls
            self.price_service.expect_get_token_info().with(eq(token_id)).times(..).returning(
                move |_| {
                    Ok(TokenInfo {
                        id: token_id_owned.clone(),
                        name: "Test Token".to_string(),
                        symbol: "TEST".to_string(),
                        decimals,
                        description: "".to_string(),
                        logo_uri: "".to_string(),
                        price_usd: price_value,
                    })
                },
            );

            // Also mock get_token_price for the same token - allow multiple calls
            let price_bd = BigDecimal::from_str(price).unwrap();
            self.price_service
                .expect_get_token_price()
                .with(eq(token_id))
                .times(..)
                .returning(move |_| Ok(price_bd.clone()));

            self
        }

        fn with_no_swaps(mut self) -> Self {
            self.account_repo.expect_get_swaps_in_period().returning(|_, _| Ok(vec![]));
            self
        }

        fn with_no_supply(mut self) -> Self {
            self.lending_repo.expect_get_deposit_snapshots_in_period().returning(|_, _| Ok(vec![]));
            self
        }

        fn with_no_supply_checked_dates(
            mut self,
            expected_start: NaiveDateTime,
            expected_end: NaiveDateTime,
        ) -> Self {
            self.lending_repo
                .expect_get_deposit_snapshots_in_period()
                .with(eq(expected_start), eq(expected_end))
                .returning(|_, _| Ok(vec![]));
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

        fn expect_snapshots<F>(mut self, validator: F) -> Self
        where
            F: Fn(&[crate::models::NewPointsSnapshot]) -> bool + Send + 'static,
        {
            self.points_repo.expect_insert_snapshots().withf(validator).returning(|_| Ok(()));
            self
        }

        fn expect_transactions<F>(mut self, validator: F) -> Self
        where
            F: Fn(&[crate::models::NewPointsTransaction]) -> bool + Send + 'static,
        {
            // Mock delete_transactions_by_date - allow it to be called
            self.points_repo.expect_delete_transactions_by_date().returning(|_| Ok(0));

            self.points_repo.expect_insert_transactions().withf(validator).returning(|_| Ok(()));
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

    fn borrow_event(
        on_behalf: &str,
        token_id: &str,
        amount: &str,
        time: NaiveDateTime,
    ) -> LendingEvent {
        LendingEvent {
            id: 1,
            market_id: "market1".to_string(),
            event_type: "Borrow".to_string(),
            token_id: token_id.to_string(),
            on_behalf: on_behalf.to_string(),
            amount: BigDecimal::from_str(amount).unwrap(),
            shares: BigDecimal::zero(),
            transaction_id: format!("tx_{}", on_behalf),
            event_index: 0,
            block_time: time,
            created_at: time,
            fields: serde_json::json!({}),
        }
    }

    fn bd(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    fn swap_event(
        address: &str,
        token_in: &str,
        amount_in: &str,
        time: NaiveDateTime,
    ) -> SwapTransactionDto {
        SwapTransactionDto {
            account_transaction: AccountTransaction {
                id: 1,
                address: address.to_string(),
                tx_type: "swap".to_string(),
                from_group: 0,
                to_group: 0,
                block_height: 1000,
                tx_id: format!("swap_tx_{}", address),
                timestamp: time,
            },
            swap: crate::models::SwapDetails {
                id: 1,
                token_in: token_in.to_string(),
                token_out: "tokenB".to_string(),
                amount_in: BigDecimal::from_str(amount_in).unwrap(),
                amount_out: BigDecimal::zero(),
                pool_address: "pool1".to_string(),
                tx_id: format!("swap_tx_{}", address),
            },
        }
    }

    #[tokio::test]
    async fn test_multiple_users_multiple_events() {
        // Scenario: user1 borrows 100 tokenA and 50 tokenA, user2 borrows 200 tokenB
        // Expected: user1 gets 3000 points, user2 gets 2000 points

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(12, 0, 0).unwrap());

        let events = vec![
            borrow_event("user1", "tokenA", "100", time),
            borrow_event("user2", "tokenB", "200", time),
            borrow_event("user1", "tokenA", "50", time),
        ];

        let service = TestSetup::new()
            .with_borrow_config("2.0")
            .with_borrow_events(events)
            .with_token_info("tokenA", 0, "10") // 0 decimals means amount is already in decimal form
            .with_token_info("tokenB", 0, "5")
            .with_no_swaps()
            .with_no_supply()
            .with_no_multipliers()
            .with_no_referrals()
            .expect_snapshots(|snapshots| {
                snapshots.len() == 2
                    && snapshots
                        .iter()
                        .any(|s| s.address == "user1" && s.borrow_points == bd("3000"))
                    && snapshots
                        .iter()
                        .any(|s| s.address == "user2" && s.borrow_points == bd("2000"))
            })
            .expect_transactions(|txs| txs.len() == 3)
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_boundary_date_range() {
        // Scenario: Verify that repository methods are called with correct date boundaries
        // Expected: start_time = date 00:00:00, end_time = next_day 00:00:00

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let expected_start = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let expected_end =
            NaiveDateTime::new(date.succ_opt().unwrap(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());

        let events = vec![borrow_event("user1", "tokenA", "100", expected_start)];

        let service = TestSetup::new()
            .with_borrow_config("2.0")
            .with_borrow_events_checked_dates(expected_start, expected_end, events)
            .with_token_info("tokenA", 0, "10")
            .with_no_supply_checked_dates(expected_start, expected_end)
            .with_swaps_checked_dates(expected_start, expected_end, vec![])
            .with_no_multipliers()
            .with_no_referrals()
            .expect_snapshots(|snapshots| snapshots.len() == 1)
            .expect_transactions(|txs| txs.len() == 1)
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_existing_user_activity() {
        // Scenario: user1 swaps 500 tokenA and borrows 100 tokenA
        // Expected: Both activities counted, swap: 5000pts, borrow: 2000pts, total: 7000pts

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(12, 0, 0).unwrap());

        let service = TestSetup::new()
            .with_swap_and_borrow_config("1.0", "2.0")
            .with_swaps(vec![swap_event("user1", "tokenA", "500", time)])
            .with_borrow_events(vec![borrow_event("user1", "tokenA", "100", time)])
            .with_token_info("tokenA", 0, "10")
            .with_no_supply()
            .with_no_multipliers()
            .with_no_referrals()
            .expect_snapshots(|snapshots| {
                if let Some(s) = snapshots.iter().find(|s| s.address == "user1") {
                    s.swap_points == bd("5000")
                        && s.borrow_points == bd("2000")
                        && s.base_points_total == bd("7000")
                } else {
                    false
                }
            })
            .expect_transactions(|txs| txs.len() == 2)
            .build();

        let result = service.calculate_points_for_date(date).await;
        assert!(result.is_ok());
    }
}

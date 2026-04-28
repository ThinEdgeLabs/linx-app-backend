use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
};
use bento_server::{AppState, error::AppError};
use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;

use crate::{
    constants::WAD,
    models::{LendingEvent, Market, MarketStateSnapshot, Position, Timeframe, UserPositionHistoryPoint},
    repository::LendingRepository,
    services::market_state_snapshot_service::derive_supply_apy,
};

pub struct LendingRouter;

impl LendingRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new()
            .route("/lending/v1/markets", get(get_markets))
            .route("/lending/v1/borrow-activity", get(get_borrow_activity))
            .route("/lending/v1/earn-activity", get(get_earn_activity))
            .route("/lending/v1/positions", get(get_positions))
            .route("/lending/v1/history/user-positions", get(get_user_position_history))
            .route("/lending/v1/stats", get(get_lending_stats))
    }
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct Pagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ActivityQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
    pub market_id: String,
    pub address: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct PositionsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
    pub market_id: Option<String>,
    pub address: Option<String>,
}

fn default_limit() -> i64 {
    20
}

fn default_page() -> i64 {
    1
}

#[utoipa::path(
    get,
    path = "/lending/markets",
    tag = "Markets",
    params(Pagination),
    responses(
        (status = 200, description = "List of markets retrieved successfully", body = Vec<Market>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_markets(
    Query(query): Query<Pagination>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let markets = lending_repo.get_markets(query.page, query.limit).await?;

    Ok(Json(markets))
}

#[utoipa::path(
    get,
    path = "/lending/borrow-activity",
    tag = "Borrow Activity",
    params(ActivityQuery),
    responses(
        (status = 200, description = "List of borrow events retrieved successfully", body = Vec<LendingEvent>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_borrow_activity(
    Query(query): Query<ActivityQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let borrow_events = [
        String::from("Borrow"),
        String::from("Repay"),
        String::from("Liquidate"),
        String::from("SupplyCollateral"),
        String::from("WithdrawCollateral"),
    ];
    let borrow_activity =
        lending_repo.get_activity(query.market_id, &borrow_events, query.address, query.page, query.limit).await?;

    Ok(Json(borrow_activity))
}

#[utoipa::path(
    get,
    path = "/lending/earn-activity",
    tag = "Earn Activity",
    params(ActivityQuery),
    responses(
        (status = 200, description = "List of earn events retrieved successfully", body = Vec<LendingEvent>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_earn_activity(
    Query(query): Query<ActivityQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let earn_events = [String::from("Supply"), String::from("Withdraw")];
    let earn_activity =
        lending_repo.get_activity(query.market_id, &earn_events, query.address, query.page, query.limit).await?;

    Ok(Json(earn_activity))
}

#[utoipa::path(
    get,
    path = "/lending/positions",
    tag = "Positions",
    params(PositionsQuery),
    responses(
        (status = 200, description = "List of positions retrieved successfully", body = Vec<Position>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_positions(
    Query(query): Query<PositionsQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    if query.page < 1 {
        return Err(AppError::BadRequest("Page must be a positive integer".to_string()));
    }

    if query.market_id.is_none() && query.address.is_none() {
        return Err(AppError::BadRequest("Either market_id or address must be provided".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let positions = lending_repo.get_positions(query.market_id, query.address, query.page, query.limit).await?;

    Ok(Json(positions))
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct UserPositionHistoryQuery {
    /// The user's wallet address.
    pub address: String,
    /// Optional market ID to filter by a single market. When omitted, returns
    /// the aggregated position across all markets.
    pub market_id: Option<String>,
    /// Time window for the chart. Determines both the date range and the bucket
    /// granularity: `1m` uses hourly buckets, `3m`, `1y` and `all` use daily
    /// buckets. Each bucket contains the last snapshot value in that period.
    pub timeframe: Timeframe,
}

#[utoipa::path(
    get,
    path = "/lending/history/user-positions",
    tag = "History",
    summary = "Get user position history",
    description = "Returns time-series data for charting a user's lending position value over time.\n\n\
        Each data point represents the last snapshot in a time bucket (hourly or daily, \
        depending on the timeframe). When no `market_id` is provided, values are summed \
        across all markets the user has positions in.\n\n\
        **Timeframe options:**\n\
        | Value | Range | Bucket size |\n\
        |-------|-------|-------------|\n\
        | `1m`  | Last 30 days | 1 hour |\n\
        | `3m`  | Last 90 days | 1 day |\n\
        | `1y`  | Last 365 days | 1 day |\n\
        | `all` | All time | 1 day |",
    params(UserPositionHistoryQuery),
    responses(
        (status = 200, description = "Time-series of position values bucketed by the requested timeframe", body = Vec<UserPositionHistoryPoint>),
        (status = 400, description = "Invalid query parameters — address is empty or market_id is blank"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_position_history(
    Query(query): Query<UserPositionHistoryQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.address.trim().is_empty() {
        return Err(AppError::BadRequest("address is required".to_string()));
    }

    if let Some(ref mid) = query.market_id
        && mid.trim().is_empty()
    {
        return Err(AppError::BadRequest("market_id cannot be blank".to_string()));
    }

    let lending_repo = LendingRepository::new(state.db.clone());
    let history =
        lending_repo.get_user_position_history(&query.address, query.market_id.as_deref(), query.timeframe).await?;

    Ok(Json(history))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LendingStatsResponse {
    pub markets: Vec<MarketStatsItem>,
    pub aggregate: AggregateStatsItem,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarketStatsItem {
    /// Market id (the ByteVec hash used by the linx contract).
    pub id: String,
    /// `[collateral_token_id, loan_token_id]`
    pub tokens: [String; 2],
    #[schema(value_type = String, example = "1234.56")]
    pub tvl_usd: BigDecimal,
    #[schema(value_type = String, example = "2000.00")]
    pub supply_usd: BigDecimal,
    #[schema(value_type = String, example = "500.00")]
    pub borrow_usd: BigDecimal,
    /// Borrow / supply ratio (0..1).
    #[schema(value_type = String, example = "0.250")]
    pub utilization: BigDecimal,
    /// `supply − borrow`.
    #[schema(value_type = String, example = "1500.00")]
    pub liquidity: BigDecimal,
    #[serde(rename = "supplyAPR")]
    #[schema(value_type = String, example = "0.0543")]
    pub supply_apr: BigDecimal,
    #[serde(rename = "borrowAPR")]
    #[schema(value_type = String, example = "0.0712")]
    pub borrow_apr: BigDecimal,
    /// Realized bad debt
    #[serde(rename = "badDebt")]
    #[schema(value_type = String, example = "0.00")]
    pub bad_debt: BigDecimal,
    pub risk: RiskParams,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AggregateStatsItem {
    /// Always `"all"` for the protocol-wide aggregate.
    pub id: String,
    #[schema(value_type = String, example = "1234.56")]
    pub tvl_usd: BigDecimal,
    #[schema(value_type = String, example = "2000.00")]
    pub supply_usd: BigDecimal,
    #[schema(value_type = String, example = "500.00")]
    pub borrow_usd: BigDecimal,
    #[schema(value_type = String, example = "100.00")]
    pub collateral_usd: BigDecimal,
    /// TVL-weighted across markets.
    #[schema(value_type = String, example = "0.250")]
    pub utilization: BigDecimal,
    #[schema(value_type = String, example = "1500.00")]
    pub liquidity: BigDecimal,
    /// TVL-weighted across markets.
    #[serde(rename = "supplyAPR")]
    #[schema(value_type = String, example = "0.0543")]
    pub supply_apr: BigDecimal,
    /// TVL-weighted across markets.
    #[serde(rename = "borrowAPR")]
    #[schema(value_type = String, example = "0.0712")]
    pub borrow_apr: BigDecimal,
    #[serde(rename = "badDebt")]
    #[schema(value_type = String, example = "0.00")]
    pub bad_debt: BigDecimal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RiskParams {
    /// Loan-to-value ratio at which a position becomes unhealthy. Decimal fraction (0..1).
    #[schema(value_type = String, example = "0.860")]
    pub ltv: BigDecimal,
    /// Liquidator bonus as a decimal fraction.
    #[serde(rename = "liqBonus")]
    #[schema(value_type = String, example = "0.044")]
    pub liq_bonus: BigDecimal,
    /// IRM model name (mapped from the IRM contract id).
    #[serde(rename = "irModel")]
    #[schema(example = "Dynamic Adaptive")]
    pub ir_model: String,
    pub oracle: String,
    #[serde(rename = "irContract")]
    pub ir_contract: String,
}

#[utoipa::path(
    get,
    path = "/lending/v1/stats",
    tag = "Stats",
    summary = "Get lending stats — per-market and protocol-wide aggregate",
    description = "Returns the latest snapshot for every market plus a protocol-wide aggregate. \
        All numeric values are decimal-encoded as strings (preserves precision through JSON). \
        Aggregate `utilization`, `supplyAPR`, `borrowAPR` are TVL-weighted across markets.",
    responses(
        (status = 200, description = "Lending stats", body = LendingStatsResponse),
        (status = 404, description = "No snapshots ingested yet"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_lending_stats(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let lending_repo = LendingRepository::new(state.db.clone());
    let snapshots = lending_repo.get_latest_market_state_snapshots().await?;
    if snapshots.is_empty() {
        return Err(AppError::NotFound("no market state snapshots available yet".to_string()));
    }
    let markets_meta = lending_repo.get_all_markets().await?;
    let meta_by_id: HashMap<String, Market> = markets_meta.into_iter().map(|m| (m.id.clone(), m)).collect();

    let mut markets = Vec::with_capacity(snapshots.len());
    for snap in &snapshots {
        let Some(meta) = meta_by_id.get(&snap.market_id) else {
            tracing::warn!("snapshot for unknown market {}", snap.market_id);
            continue;
        };
        markets.push(build_market_item(snap, meta));
    }

    let aggregate = build_aggregate(&snapshots);

    Ok(Json(LendingStatsResponse { markets, aggregate }))
}

fn build_market_item(snap: &MarketStateSnapshot, meta: &Market) -> MarketStatsItem {
    let util = utilization(&snap.total_supply_assets, &snap.total_borrow_assets);
    let supply_apy =
        derive_supply_apy(&snap.borrow_apy, &snap.total_supply_assets, &snap.total_borrow_assets, &snap.fee);
    let liquidity = (&snap.total_supply_usd - &snap.total_borrow_usd).with_scale(2);

    MarketStatsItem {
        id: snap.market_id.clone(),
        tokens: [meta.collateral_token.clone(), meta.loan_token.clone()],
        tvl_usd: (&snap.total_supply_usd + &snap.total_collateral_usd).with_scale(2),
        supply_usd: snap.total_supply_usd.clone(),
        borrow_usd: snap.total_borrow_usd.clone(),
        utilization: util.with_scale(3),
        liquidity,
        supply_apr: supply_apy.with_scale(3),
        borrow_apr: snap.borrow_apy.clone(),
        bad_debt: BigDecimal::zero(),
        risk: build_risk(meta),
    }
}

/// Sums per-market USD values; TVL-weights the rate metrics.
fn build_aggregate(snapshots: &[MarketStateSnapshot]) -> AggregateStatsItem {
    let mut total_supply = BigDecimal::zero();
    let mut total_borrow = BigDecimal::zero();
    let mut total_collateral = BigDecimal::zero();
    let mut total_tvl = BigDecimal::zero();
    let mut weighted_util = BigDecimal::zero();
    let mut weighted_sup = BigDecimal::zero();
    let mut weighted_bor = BigDecimal::zero();

    for snap in snapshots {
        let tvl = &snap.total_supply_usd + &snap.total_collateral_usd;
        let util = utilization(&snap.total_supply_assets, &snap.total_borrow_assets);
        let supply_apy =
            derive_supply_apy(&snap.borrow_apy, &snap.total_supply_assets, &snap.total_borrow_assets, &snap.fee);

        total_supply += &snap.total_supply_usd;
        total_borrow += &snap.total_borrow_usd;
        total_collateral += &snap.total_collateral_usd;
        weighted_util += &util * &tvl;
        weighted_sup += &supply_apy * &tvl;
        weighted_bor += &snap.borrow_apy * &tvl;
        total_tvl += tvl;
    }

    let (util, sup_apr, bor_apr) = if total_tvl.is_zero() {
        (BigDecimal::zero(), BigDecimal::zero(), BigDecimal::zero())
    } else {
        (weighted_util / &total_tvl, weighted_sup / &total_tvl, weighted_bor / &total_tvl)
    };
    let liquidity = (&total_supply - &total_borrow).with_scale(2);

    AggregateStatsItem {
        id: "all".to_string(),
        tvl_usd: total_tvl.with_scale(2),
        supply_usd: total_supply.with_scale(2),
        borrow_usd: total_borrow.with_scale(2),
        collateral_usd: total_collateral.with_scale(2),
        utilization: util.with_scale(3),
        liquidity,
        supply_apr: sup_apr.with_scale(3),
        borrow_apr: bor_apr.with_scale(3),
        bad_debt: BigDecimal::zero(),
    }
}

fn utilization(total_supply_assets: &BigDecimal, total_borrow_assets: &BigDecimal) -> BigDecimal {
    if total_supply_assets.is_zero() { BigDecimal::zero() } else { total_borrow_assets / total_supply_assets }
}

fn build_risk(meta: &Market) -> RiskParams {
    RiskParams {
        ltv: wad_to_fraction(&meta.ltv).with_scale(3),
        liq_bonus: liquidation_bonus_fraction(&meta.ltv).with_scale(3),
        ir_model: ir_model_for(&meta.irm).to_string(),
        oracle: meta.oracle.clone(),
        ir_contract: meta.irm.clone(),
    }
}

/// Single-IRM mapping for now. Add entries as new IRMs ship.
fn ir_model_for(_irm_contract_id: &str) -> &'static str {
    "Dynamic"
}

/// `wad_value / 1e18` as a plain decimal fraction (e.g. `0.860` for 86%).
fn wad_to_fraction(value: &BigDecimal) -> BigDecimal {
    let wad = BigDecimal::from_str(WAD).unwrap();
    value / &wad
}

/// Liquidation incentive: `LIF = min(1.15, 1 / (0.3·lltv + 0.7))`, bonus = `LIF − 1`.
fn liquidation_bonus_fraction(ltv_wad: &BigDecimal) -> BigDecimal {
    let lltv = wad_to_fraction(ltv_wad);
    let cursor = BigDecimal::from_str("0.3").unwrap();
    let max_lif = BigDecimal::from_str("1.15").unwrap();
    let one = BigDecimal::from(1);

    let denom = &cursor * &lltv + (&one - &cursor);
    let lif_uncapped = if denom.is_zero() { max_lif.clone() } else { &one / denom };
    let lif = if lif_uncapped > max_lif { max_lif } else { lif_uncapped };
    lif - one
}

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use chrono::DateTime;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::models::AccountTransactionFlattened;
use crate::repository::AccountTransactionRepository;

use bento_server::AppState;
use bento_server::error::AppError;

pub struct AccountTransactionsRouter;

impl AccountTransactionsRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new().routes(routes!(get_account_transactions_handler))
    }
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct AccountTransactionsQuery {
    pub address: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Cursor for pagination - pass the timestamp (Unix millis) of the last item from the previous page
    pub cursor: Option<i64>,
}

fn default_limit() -> i64 {
    20
}

#[utoipa::path(
    get,
    path = "/history/v1/account-transactions",
    tag = "Account Transactions",
    params(AccountTransactionsQuery),
    responses(
        (status = 200, description = "List of account transactions retrieved successfully", body = Vec<AccountTransactionFlattened>),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_account_transactions_handler(
    Query(query): Query<AccountTransactionsQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if query.limit <= 0 || query.limit > 100 {
        return Err(AppError::BadRequest("Limit must be between 1 and 100".to_string()));
    }

    let address = match query.address {
        Some(addr) if !addr.is_empty() => addr,
        Some(_) => {
            return Err(AppError::BadRequest("Address parameter cannot be empty".to_string()));
        }
        None => return Err(AppError::BadRequest("Missing parameter address".to_string())),
    };

    let cursor = query.cursor.and_then(|millis| {
        DateTime::from_timestamp_millis(millis).map(|dt| dt.naive_utc())
    });

    let account_tx_repo = AccountTransactionRepository::new(state.db.clone());

    let transactions =
        account_tx_repo.get_account_transactions(&address, query.limit, cursor).await?;

    // Convert to flattened format
    let flattened: Vec<AccountTransactionFlattened> =
        transactions.into_iter().map(|tx| tx.into()).collect();

    Ok(Json(flattened))
}

// #[derive(Debug, Deserialize, IntoParams, ToSchema)]
// pub struct BalanceHistoryQuery {
//     pub address: String,
// }

// #[utoipa::path(
//     get,
//     path = "/account-value-history",
//     tag = "Account Value History",
//     params(BalanceHistoryQuery),
//     responses(
//         (status = 200, description = "Account value history retrieved successfully"),
//         (status = 400, description = "Invalid query parameters"),
//         (status = 500, description = "Internal server error")
//     )
// )]
// pub async fn get_account_value_history_handler(
//     Query(query): Query<BalanceHistoryQuery>,
// ) -> Result<impl IntoResponse, AppError> {
//     if query.address.is_empty() {
//         return Err(AppError::BadRequest("Address parameter cannot be empty".to_string()));
//     }

//     let balance_history: Vec<(BigDecimal, i64)> = Vec::new();
//     Ok(Json(balance_history))
// }

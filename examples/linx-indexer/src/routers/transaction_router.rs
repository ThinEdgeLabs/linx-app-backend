use axum::{Json, extract::State, response::IntoResponse, routing::post};
use bento_server::{AppState, error::AppError};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;

use crate::models::NewLinxTransaction;
use crate::repository::LinxTransactionsRepository;

pub struct TransactionsRouter;

impl TransactionsRouter {
    pub fn register() -> OpenApiRouter<AppState> {
        OpenApiRouter::new().route("/transactions/v1/submit", post(submit_swap_handler))
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitSwapRequest {
    /// Unsigned transaction hex string
    #[schema(example = "00a1b2c3...")]
    pub unsigned_tx: String,

    /// Transaction signature hex string
    #[schema(example = "1234abcd...")]
    pub signature: String,

    /// User address submitting the swap
    #[schema(example = "1A2B3C...")]
    pub user_address: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmitSwapResponse {
    /// Transaction ID from blockchain
    pub tx_id: String,

    /// Chain group (shard) the transaction came from
    pub from_group: i32,

    /// Chain group (shard) the transaction goes to
    pub to_group: i32,

    /// Success message
    pub message: String,
}

/// Submit a swap transaction to the blockchain
///
/// This endpoint accepts a signed swap transaction, broadcasts it to the Alephium
/// blockchain, and tracks it as a UI-initiated swap for points calculation.
#[utoipa::path(
    post,
    path = "/transactions/v1/submit",
    tag = "Transactions",
    request_body = SubmitSwapRequest,
    responses(
        (status = 200, description = "Transaction submitted successfully", body = SubmitSwapResponse),
        (status = 400, description = "Invalid request format"),
        (status = 500, description = "Blockchain submission failed")
    )
)]
pub async fn submit_swap_handler(
    State(state): State<AppState>,
    Json(request): Json<SubmitSwapRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Basic validation
    validate_request(&request)?;

    // 2. Submit to blockchain
    let submit_result = state
        .node_client
        .submit_transaction(&request.unsigned_tx, &request.signature)
        .await
        .map_err(|e| {
            tracing::error!("Blockchain submission failed: {}", e);
            AppError::Internal(anyhow::anyhow!("Failed to submit transaction: {}", e))
        })?;

    // 3. Track in database (only on successful blockchain submission)
    let linx_tx_repo = LinxTransactionsRepository::new(state.db.clone());

    let new_transaction = NewLinxTransaction {
        tx_id: submit_result.tx_id.clone(),
        user_address: request.user_address.clone(),
    };

    linx_tx_repo
        .insert_linx_transaction(new_transaction)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to track Linx transaction {}: {}. Transaction succeeded on blockchain.",
                submit_result.tx_id,
                e
            );
            // Don't fail the request - transaction was already submitted successfully
        })
        .ok();

    tracing::info!(
        "Successfully submitted swap tx {} for user {}",
        submit_result.tx_id,
        request.user_address
    );

    Ok(Json(SubmitSwapResponse {
        tx_id: submit_result.tx_id,
        from_group: submit_result.from_group,
        to_group: submit_result.to_group,
        message: "Swap transaction submitted successfully".to_string(),
    }))
}

fn validate_request(request: &SubmitSwapRequest) -> Result<(), AppError> {
    // Validate unsigned_tx
    if request.unsigned_tx.is_empty() {
        return Err(AppError::BadRequest("unsigned_tx cannot be empty".to_string()));
    }

    if !request.unsigned_tx.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("unsigned_tx must be valid hexadecimal".to_string()));
    }

    // Validate signature
    if request.signature.is_empty() {
        return Err(AppError::BadRequest("signature cannot be empty".to_string()));
    }

    if !request.signature.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("signature must be valid hexadecimal".to_string()));
    }

    // Validate user_address
    if request.user_address.is_empty() {
        return Err(AppError::BadRequest("user_address cannot be empty".to_string()));
    }

    Ok(())
}

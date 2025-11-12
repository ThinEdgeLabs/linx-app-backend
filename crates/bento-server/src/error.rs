use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use bento_types::BlockHash;
use serde_json::json;
use std::fmt;

// Define custom error types
#[derive(Debug)]
pub enum AppError {
    // Internal server errors
    Internal(anyhow::Error),

    // Database related errors
    DatabaseError(anyhow::Error),

    // Validation errors
    ValidationError(String),

    // Not found errors
    NotFound(String),

    // Authentication errors
    Unauthorized(String),

    // Authorization errors
    Forbidden(String),

    // Bad request errors
    BadRequest(String),
}

// Implement Display for AppError
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Internal(_) => write!(f, "Internal server error"),
            AppError::DatabaseError(e) => write!(f, "Database error: {}", e),
            AppError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            AppError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
        }
    }
}

// Implement Error trait for AppError
impl std::error::Error for AppError {}

// Convert AppError into Response
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Internal(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal server error: {}", e))
            }
            AppError::DatabaseError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error occurred: {}", e))
            }
            AppError::ValidationError(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, format!("Validation error: {}", msg))
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, format!("Not found: {}", msg)),
            AppError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, format!("Unauthorized: {}", msg))
            }
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, format!("Forbidden: {}", msg)),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, format!("Bad request: {}", msg)),
        };

        // Create a JSON response with error details
        let body = Json(json!({
            "success": false,
            "error": {
                "message": error_message,
                "code": status.as_u16()
            }
        }));

        (status, body).into_response()
    }
}

// Implement the conversion from anyhow::Error to AppError
impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        // categorize based on error message content

        let error_msg = error.to_string().to_lowercase();

        if error_msg.contains("database") || error_msg.contains("SQL") {
            AppError::DatabaseError(error)
        } else if error_msg.contains("validation") {
            AppError::ValidationError(error_msg)
        } else if error_msg.contains("authentication") || error_msg.contains("unauthorized") {
            AppError::Unauthorized(error_msg)
        } else if error_msg.contains("not found") {
            AppError::NotFound(error_msg)
        } else {
            AppError::Internal(error)
        }
    }
}

impl From<diesel::result::Error> for AppError {
    fn from(err: diesel::result::Error) -> Self {
        AppError::DatabaseError(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Block not found: {0}")]
    BlockNotFound(BlockHash),

    #[error("Database error: {0}")]
    DatabaseError(#[from] diesel::result::Error),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl From<RepositoryError> for AppError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::BlockNotFound(hash) => {
                AppError::NotFound(format!("Block not found: {}", hash))
            }
            RepositoryError::DatabaseError(e) => AppError::DatabaseError(e.into()),
            RepositoryError::Other(e) => AppError::Internal(e),
        }
    }
}

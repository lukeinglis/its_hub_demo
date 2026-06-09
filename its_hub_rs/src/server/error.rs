use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::types::LmClientError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error("service not configured")]
    NotConfigured,

    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("streaming not supported")]
    StreamingNotSupported,

    #[error("algorithm error: {0}")]
    Algorithm(String),

    #[error("upstream error: {0}")]
    Upstream(#[from] LmClientError),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::ModelNotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            Self::NotConfigured => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Self::StreamingNotSupported => (StatusCode::NOT_IMPLEMENTED, self.to_string()),
            Self::Algorithm(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
            Self::Upstream(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        let body = serde_json::json!({"error": {"message": message, "type": "server_error"}});
        (status, axum::Json(body)).into_response()
    }
}

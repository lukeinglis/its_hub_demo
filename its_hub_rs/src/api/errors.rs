/// Error types for LM client operations.
///
/// Consolidated from types.rs error section and client/error.rs.

#[derive(Debug, thiserror::Error)]
pub enum LmClientError {
    #[error("rate limit exceeded: {message}")]
    RateLimit { message: String, status: u16 },

    #[error("authentication failed: {message}")]
    Authentication { message: String, status: u16 },

    #[error("context length exceeded: {message}")]
    ContextLength { message: String },

    #[error("bad request: {message}")]
    BadRequest { message: String },

    #[error("server error: {message}")]
    ServerError { message: String, status: u16 },

    #[error("connection error: {0}")]
    Connection(String),
}

impl LmClientError {
    pub fn from_response(status: u16, body: &str) -> Self {
        let message = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("error")?.get("message")?.as_str().map(String::from))
            .unwrap_or_else(|| body.to_string());

        let lower = message.to_lowercase();

        if status == 429 || lower.contains("rate limit") {
            return LmClientError::RateLimit { message, status };
        }

        let context_keywords = [
            "context length",
            "maximum context",
            "too long",
            "token limit",
            "context_length_exceeded",
            "max_tokens",
        ];
        if status == 400 && context_keywords.iter().any(|kw| lower.contains(kw)) {
            return LmClientError::ContextLength { message };
        }

        if status == 401
            || status == 403
            || lower.contains("authentication")
            || lower.contains("unauthorized")
        {
            return LmClientError::Authentication { message, status };
        }

        if status == 400 {
            return LmClientError::BadRequest { message };
        }

        if status >= 500 {
            return LmClientError::ServerError { message, status };
        }

        LmClientError::Connection(message)
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LmClientError::RateLimit { .. }
                | LmClientError::ServerError { .. }
                | LmClientError::Connection(_)
        )
    }

    pub fn status_code(&self) -> Option<u16> {
        match self {
            LmClientError::RateLimit { status, .. } => Some(*status),
            LmClientError::Authentication { status, .. } => Some(*status),
            LmClientError::ServerError { status, .. } => Some(*status),
            LmClientError::ContextLength { .. } => None,
            LmClientError::BadRequest { .. } => None,
            LmClientError::Connection(_) => None,
        }
    }
}

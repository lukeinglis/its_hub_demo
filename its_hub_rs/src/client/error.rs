pub use crate::types::LmClientError;

impl LmClientError {
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

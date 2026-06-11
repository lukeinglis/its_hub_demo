pub mod error;
pub mod handlers;
pub mod metrics;
pub mod passthrough;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, Method, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use state::AppState;

pub fn app(state: Arc<AppState>) -> Router {
    let metrics_handle = metrics::init_metrics();

    Router::new()
        .route("/configure", post(handlers::configure))
        .route("/v1/chat/completions", post(handlers::chat_completions))
        .route("/v1/models", get(handlers::list_models))
        .route("/health", get(handlers::health))
        .route("/health/live", get(handlers::health_live))
        .route("/health/ready", get(handlers::health_ready))
        .route(
            "/metrics",
            get(metrics::metrics_handler).with_state(metrics_handle),
        )
        // Rate limiting: allow at most 100 concurrent in-flight requests.
        .layer(ConcurrencyLimitLayer::new(100))
        .layer(
            CorsLayer::new()
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
                .allow_methods([Method::GET, Method::POST]),
        )
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(300),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
        .with_state(state)
}

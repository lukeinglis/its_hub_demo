//! Prometheus metrics for the ITS gateway.
//!
//! Tracks request counts, durations, cache performance, and upstream calls.

use std::sync::OnceLock;
use std::time::Instant;

use axum::extract::State;
use axum::response::IntoResponse;
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Initialize the Prometheus recorder and return a handle for rendering metrics.
/// Safe to call multiple times (e.g., in tests): only the first call installs the
/// global recorder; subsequent calls return the same handle.
pub fn init_metrics() -> PrometheusHandle {
    METRICS_HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .install_recorder()
                .expect("failed to install Prometheus recorder")
        })
        .clone()
}

/// Record a completed request.
pub fn record_request(algorithm: &str, status_code: u16, duration: &Instant) {
    counter!(
        "its_gateway_requests_total",
        "algorithm" => algorithm.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
    histogram!("its_gateway_request_duration_seconds").record(duration.elapsed().as_secs_f64());
}

/// Record a cache hit.
pub fn record_cache_hit() {
    counter!("its_gateway_cache_hits_total").increment(1);
}

/// Record a cache miss.
pub fn record_cache_miss() {
    counter!("its_gateway_cache_misses_total").increment(1);
}

/// Record an upstream LM request.
pub fn record_upstream_request() {
    counter!("its_gateway_upstream_requests_total").increment(1);
}

/// GET /metrics handler: returns Prometheus text format.
pub async fn metrics_handler(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

# its-hub-rs

Rust inference-time scaling (ITS) gateway for LLMs. Sits in the Envoy AI Gateway request path, applying scaling algorithms to improve response quality by spending more compute at inference time.

This is a gateway microservice, not a library rewrite. It provides production features the Python `its_hub` library does not have: graceful degradation via passthrough, a concurrent token cache, Envoy integration headers, Prometheus metrics, rate limiting, and Kubernetes-style health probes.

## Quick Start

### Build

```bash
cargo build --release
```

### Test

```bash
cargo test
```

### Lint

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

### Run

```bash
./target/release/its-hub-rs --host 0.0.0.0 --port 8108
```

## API Endpoints

### POST /configure

Sets the backend model, scaling algorithm, and gateway options. Requires `Authorization: Bearer <token>` header when `ITS_ADMIN_TOKEN` is set.

```bash
curl -X POST http://localhost:8108/configure \
  -H "Content-Type: application/json" \
  -d '{
    "endpoint": "http://localhost:8100/v1",
    "model": "your-model-name",
    "alg": "self-consistency",
    "passthrough_on_error": true,
    "cache_enabled": true,
    "cache_ttl_seconds": 300,
    "request_timeout_seconds": 30
  }'
```

**Configuration fields:**

| Field | Type | Default | Description |
|---|---|---|---|
| `endpoint` | string | (required) | Upstream vLLM/OpenAI base URL |
| `model` | string | (required) | Model name to use |
| `alg` | string | (required) | Algorithm name (see Algorithms below) |
| `api_key` | string | null | API key for upstream |
| `system_prompt` | string | null | System prompt prepended to all requests |
| `temperature` | float | null | Default temperature for algorithm |
| `max_tokens` | int | null | Default max tokens |
| `max_concurrent_requests` | int | 64 | Max concurrent upstream requests |
| `passthrough_on_error` | bool | true | Fall back to passthrough on algorithm failure |
| `cache_enabled` | bool | false | Enable response caching |
| `cache_ttl_seconds` | int | 300 | Cache entry TTL |
| `cache_max_entries` | int | 10000 | Maximum cache entries |
| `request_timeout_seconds` | int | 30 | Upstream request timeout in seconds |

### POST /v1/chat/completions

OpenAI-compatible chat completions endpoint with inference-time scaling. Accepts all standard OpenAI chat completion fields plus:

| Field | Type | Default | Description |
|---|---|---|---|
| `budget` | int | 8 | Number of parallel generations (1 to 1000) |
| `return_response_only` | bool | true | Return only the selected response |

```bash
curl -X POST http://localhost:8108/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "your-model-name",
    "messages": [{"role": "user", "content": "What is 6 * 7?"}],
    "budget": 5
  }'
```

### GET /v1/models

Lists configured models. Returns OpenAI-compatible model list response.

### GET /health

Returns gateway health status. Only exposes essential information:

```json
{"status": "ok", "algorithm": "self-consistency", "models": 1}
```

### GET /health/live

Kubernetes liveness probe. Always returns 200 if the process is running.

### GET /health/ready

Kubernetes readiness probe. Returns 200 only when an algorithm is configured and at least one model is connected. Returns 503 otherwise.

### GET /metrics

Prometheus metrics endpoint. Returns text format metrics:

- `its_gateway_requests_total` (counter, by algorithm, status_code): total requests processed
- `its_gateway_request_duration_seconds` (histogram): request processing time
- `its_gateway_cache_hits_total` (counter): cache hit count
- `its_gateway_cache_misses_total` (counter): cache miss count
- `its_gateway_upstream_requests_total` (counter): upstream LM requests made

## Configuration

### Environment Variables

| Variable | Description |
|---|---|
| `ITS_ADMIN_TOKEN` | When set, requires `Authorization: Bearer <token>` on `/configure` requests |
| `RUST_LOG` | Log level filter (e.g., `info`, `debug`, `its_hub_rs=debug`) |

### Envoy Headers

The gateway integrates with Envoy AI Gateway through request/response headers.

**Request headers (from Envoy):**

| Header | Effect |
|---|---|
| `x-its-algorithm` | Override the configured algorithm for this request |
| `x-its-budget` | Override the budget parameter (must be valid u32, returns 400 if invalid) |

**Response headers (added by gateway):**

| Header | Example | Description |
|---|---|---|
| `x-its-algorithm-used` | `self-consistency` | Which algorithm actually ran |
| `x-its-cache-hit` | `true` / `false` | Whether the response came from cache |
| `x-its-latency-ms` | `123` | Processing time in milliseconds |

## Token Cache

LRU response cache keyed by (model, messages, temperature, max_tokens, budget). Thread-safe via `Arc<RwLock<>>` for concurrent access.

- Entries expire after `cache_ttl_seconds` (default 300)
- Maximum capacity controlled by `cache_max_entries` (default 10000)
- Expired entries are evicted on write; oldest entries evicted when at capacity
- Cache performance metrics available at `/metrics`

## Passthrough / Graceful Degradation

When the ITS algorithm fails and `passthrough_on_error` is enabled (the default), the gateway forwards the request directly to the upstream vLLM backend without disruption. This ensures availability even during algorithm errors.

The response header `x-its-algorithm-used: passthrough` indicates degraded mode.

## Rate Limiting

The gateway applies a concurrency limit of 100 in-flight requests. Requests exceeding this limit receive a 503 response. This prevents budget-amplified requests from overwhelming the upstream backend.

## Algorithms

| Algorithm | Description | Required Config |
|---|---|---|
| `self-consistency` | Generate multiple responses, select most common answer via majority voting | (none) |
| `best-of-n` | Generate N responses, select highest-scored one using an outcome reward model | `rm_endpoint` (or `rm_name: llm-judge` with `judge_model`) |
| `beam-search` | Step-by-step generation with beam width, guided by a process reward model | `step_token`, `prm_endpoint` |
| `particle-filtering` | Probabilistic resampling of partial responses using PRM scores | `step_token`, `prm_endpoint` |
| `particle-gibbs` | Iterative particle filtering with reference trajectory | `step_token`, `prm_endpoint` |
| `entropic-particle-filtering` | Particle filtering with adaptive temperature based on entropy/ESS | `step_token`, `prm_endpoint` |
| `planning-wrapper` | Prepends a planning step before delegating to an inner algorithm | `inner_alg` |

### Budget Interpretation

- **Self-Consistency / Best-of-N**: number of parallel generations
- **Beam Search**: total generations divided by beam width (controls search depth)
- **Particle Filtering / Particle Gibbs**: number of particles maintained during sampling

## Docker

### Build

```bash
docker build -t its-hub-rs .
```

### Run

```bash
docker run -p 8108:8108 its-hub-rs
```

With admin token:

```bash
docker run -p 8108:8108 -e ITS_ADMIN_TOKEN=my-secret its-hub-rs
```

## Architecture

- `src/core/algorithms/`: All scaling algorithm implementations behind the `ScalingAlgorithm` trait
- `src/core/cache.rs`: LRU token cache with TTL eviction and hit/miss statistics
- `src/core/lms/`: LM backend client (OpenAI/vLLM compatible) with configurable timeout
- `src/core/reward_models/`: Reward model integrations (HTTP PRM, LLM-as-a-judge ORM)
- `src/server/handlers.rs`: Axum HTTP handlers for all endpoints
- `src/server/metrics.rs`: Prometheus metrics collection and export
- `src/server/passthrough.rs`: Passthrough handler for graceful degradation
- `src/server/state.rs`: Shared application state with atomic gateway config swap
- `src/api/types.rs`: Shared request/response types matching OpenAI's chat completion schema

## Relationship to Python Version

This crate is a production gateway companion to the Python `its_hub` library. The Python version lives in the repository root and is used for research, prototyping, and benchmarking. This Rust gateway reimplements the same algorithms and server API with lower latency, a concurrent token cache, Envoy integration, Prometheus metrics, and single-binary deployment for production use behind Envoy AI Gateway.

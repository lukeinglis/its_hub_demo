# its-hub-rs

Rust inference-time scaling (ITS) gateway for LLMs. Sits in the Envoy AI Gateway request path, applying scaling algorithms to improve response quality by spending more compute at inference time.

This is a gateway microservice, not a library rewrite. It provides production features the Python `its_hub` library does not have: graceful degradation via passthrough, a concurrent token cache, Envoy integration headers, and Kubernetes-style health probes.

## Quick Start

### Build

```bash
cargo build --release
```

### Test

```bash
cargo test
```

### Run

```bash
./target/release/its-hub-rs --host 0.0.0.0 --port 8108
```

## Gateway Features

### Envoy Integration

The gateway receives traffic routed by Envoy's body-based routing. Envoy headers override request parameters:

| Header | Effect |
|---|---|
| `x-its-algorithm` | Override the configured algorithm for this request |
| `x-its-budget` | Override the budget parameter for this request |

Response headers added to every chat completion response:

| Header | Example | Description |
|---|---|---|
| `x-its-algorithm-used` | `self-consistency` | Which algorithm actually ran |
| `x-its-cache-hit` | `true` / `false` | Whether the response came from cache |
| `x-its-latency-ms` | `123` | Processing time in milliseconds |

### Passthrough / Graceful Degradation

When the ITS algorithm fails and `passthrough_on_error` is enabled (the default), the gateway forwards the request directly to the upstream vLLM backend without disruption. This ensures availability even during algorithm errors.

Configure via `/configure`:
```json
{"passthrough_on_error": true}
```

### Token Cache

LRU response cache keyed by (model, messages, temperature, max_tokens). Thread-safe via `Arc<RwLock<>>` for concurrent access.

Configure via `/configure`:
```json
{
  "cache_enabled": true,
  "cache_ttl_seconds": 300,
  "cache_max_entries": 10000
}
```

Cache statistics are exposed in the `/health` endpoint response.

### Health Probes

Three health endpoints for Kubernetes integration:

| Endpoint | Behavior |
|---|---|
| `GET /health` | Combined status: algorithm, models, cache stats, passthrough config |
| `GET /health/live` | Liveness probe: always returns 200 if process is running |
| `GET /health/ready` | Readiness probe: returns 200 only if algorithm is configured and a model is connected; 503 otherwise |

## Usage

### Configure

Send a `POST /configure` request to set the backend model, scaling algorithm, and gateway options:

```bash
curl -X POST http://localhost:8108/configure \
  -H "Content-Type: application/json" \
  -d '{
    "endpoint": "http://localhost:8100/v1",
    "model": "your-model-name",
    "alg": "self-consistency",
    "passthrough_on_error": true,
    "cache_enabled": true,
    "cache_ttl_seconds": 300
  }'
```

### Query

Send chat completions through the scaling algorithm via the OpenAI-compatible endpoint:

```bash
curl -X POST http://localhost:8108/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "your-model-name",
    "messages": [{"role": "user", "content": "What is 6 * 7?"}],
    "budget": 5
  }'
```

The `budget` parameter controls how many parallel generations the algorithm uses.

## Algorithms

| Algorithm | Description |
|---|---|
| **Self-Consistency** | Generate multiple responses and select the most common answer via majority voting. |
| **Best-of-N** | Generate N responses and select the highest-scored one using an outcome reward model. |
| **Beam Search** | Step-by-step generation with beam width, guided by a process reward model. |
| **Particle Filtering** | Probabilistic resampling of partial responses using process reward model scores. |
| **Particle Gibbs** | Iterative particle filtering with a reference trajectory for improved exploration. |
| **Entropic Particle Filtering** | Particle filtering with adaptive temperature based on entropy or ESS. |
| **Planning Wrapper** | Prepends a planning step before delegating to an inner scaling algorithm. |

## Architecture

- `src/core/algorithms/`: All scaling algorithm implementations behind the `ScalingAlgorithm` trait.
- `src/core/cache.rs`: LRU token cache with TTL eviction and hit/miss statistics.
- `src/core/lms/`: LM backend client (OpenAI/vLLM compatible).
- `src/core/reward_models/`: Reward model integrations (HTTP PRM, LLM-as-a-judge ORM).
- `src/server/handlers.rs`: Axum HTTP server with `/configure`, `/v1/chat/completions`, `/v1/models`, `/health`, `/health/live`, `/health/ready`.
- `src/server/passthrough.rs`: Passthrough handler for graceful degradation.
- `src/server/state.rs`: Shared application state with cache, passthrough config.
- `src/api/types.rs`: Shared request/response types matching OpenAI's chat completion schema.

## Relationship to Python Version

This crate is a production gateway companion to the Python `its_hub` library. The Python version lives in the repository root and is used for research, prototyping, and benchmarking. This Rust gateway reimplements the same algorithms and server API with lower latency, a concurrent token cache, Envoy integration, and single-binary deployment for production use behind Envoy AI Gateway.

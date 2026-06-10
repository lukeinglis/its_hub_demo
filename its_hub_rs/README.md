# its-hub-rs

Rust rewrite of the [its_hub](../README.md) inference-time scaling library for LLMs. Provides an OpenAI-compatible HTTP API that sits in front of any LLM backend and applies inference-time scaling algorithms to improve response quality by spending more compute at inference time.

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

## Usage

### Configure

Send a `POST /configure` request to set the backend model and scaling algorithm:

```bash
curl -X POST http://localhost:8108/configure \
  -H "Content-Type: application/json" \
  -d '{
    "endpoint": "http://localhost:8100/v1",
    "model": "your-model-name",
    "alg": "self-consistency"
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

- `src/algorithms/`: All scaling algorithm implementations behind the `ScalingAlgorithm` trait.
- `src/client/`: LM backend clients (direct OpenAI/vLLM and LiteLLM multi-provider).
- `src/integration/`: Reward model integrations (HTTP PRM, LLM-as-a-judge ORM).
- `src/server/`: Axum HTTP server with `/configure`, `/v1/chat/completions`, `/v1/models`, and `/health` endpoints.
- `src/types.rs`: Shared request/response types matching OpenAI's chat completion schema.
- `src/step_generation.rs`: Incremental text generation with configurable step tokens.
- `src/chat_messages.rs`: Prompt/message abstraction used by PRM scoring.

## Relationship to Python Version

This crate is a port of the Python `its_hub` library. The Python version lives in the repository root and uses `openai`, `litellm`, and `reward_hub` packages. This Rust version reimplements the same algorithms and server API with the goal of lower latency, smaller memory footprint, and single-binary deployment. Both versions expose the same HTTP API and can be used interchangeably.

# Factory Configuration

## Goal

Production-grade Rust reimplementation of the its_hub inference-time scaling microservice for Red Hat AI's Envoy-based gateway, providing low-latency fan-out and response aggregation with Self-Consistency and Best-of-N algorithms.

## Scope

### Modifiable

- its_hub_rs/src/**/*.rs
- its_hub_rs/tests/**/*.rs
- its_hub_rs/Cargo.toml
- its_hub_rs/Dockerfile
- its_hub_rs/.dockerignore

### Read-only

- README.md
- CLAUDE.md
- its_hub/**/*.py
- eval/score.py

## Guards

- Do not delete or overwrite existing tests
- Do not modify files outside the declared scope
- Do not introduce secrets or credentials into the repository
- Do not modify the Python source code (its_hub/) as part of Rust improvements

## Eval

### Command

```bash
python3 eval/score.py
```

### Threshold

0.85

## Target Branch

main

## Smoke Test

```bash
cd its_hub_rs && ~/.cargo/bin/cargo test --quiet 2>&1 | tail -1 | grep -q "ok"
```

## Constraints

- Prefer small, incremental changes over large rewrites
- Each change should be accompanied by at least one test
- Follow Rust idioms: use Result/Option, derive traits, structured error types
- Maintain wire compatibility with the Python IaaS API contract
- All Clippy warnings must be resolved (cargo clippy -- -D warnings)

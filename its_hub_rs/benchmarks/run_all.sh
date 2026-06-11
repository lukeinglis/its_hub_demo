#!/bin/bash
# Run the full Python vs Rust benchmark suite.
# Usage: ./run_all.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RS_DIR="$SCRIPT_DIR/.."
PIDS_TO_KILL=()

cleanup() {
    echo ""
    echo "Cleaning up background processes..."
    for pid in "${PIDS_TO_KILL[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    echo "Done."
}
trap cleanup EXIT

echo "============================================================"
echo "ITS Hub Benchmark: Python vs Rust Gateway"
echo "============================================================"
echo ""

# Step 0: Build Rust binary (release mode)
echo "[0/7] Building Rust gateway (release)..."
cd "$RS_DIR"
if [ -f "$HOME/.cargo/bin/cargo" ]; then
    CARGO="$HOME/.cargo/bin/cargo"
else
    CARGO="cargo"
fi
$CARGO build --release 2>&1 | tail -1
RUST_BINARY="$RS_DIR/target/release/its-hub-rs"
echo "      Binary: $RUST_BINARY"
echo "      Size:   $(ls -lh "$RUST_BINARY" | awk '{print $5}')"
echo ""

# Step 1: Start mock vLLM server
echo "[1/7] Starting mock vLLM server on :9999..."
cd "$PROJECT_ROOT"
uv run python "$SCRIPT_DIR/mock_vllm.py" &
MOCK_PID=$!
PIDS_TO_KILL+=($MOCK_PID)
sleep 2

# Verify mock is up
if ! curl -sf http://127.0.0.1:9999/v1/models > /dev/null 2>&1; then
    echo "ERROR: Mock vLLM server failed to start"
    exit 1
fi
echo "      Mock vLLM server running (PID $MOCK_PID)"
echo ""

# Step 2: Run Python benchmark
echo "[2/7] Running Python benchmark..."
cd "$PROJECT_ROOT"
uv run python "$SCRIPT_DIR/bench_python.py"
echo ""

# Step 3: Capture Python process memory (approximate, from the mock server)
echo "[3/7] Capturing Python memory baseline..."
PYTHON_RSS=$(ps -o rss= -p $MOCK_PID 2>/dev/null || echo "0")
PYTHON_RSS_MB=$(echo "scale=1; ${PYTHON_RSS:-0} / 1024" | bc 2>/dev/null || echo "N/A")
echo "      Mock vLLM server RSS: ${PYTHON_RSS_MB}MB"
echo ""

# Step 4: Start Rust gateway
echo "[4/7] Starting Rust gateway on :8108..."
RUST_LOG=warn "$RUST_BINARY" --host 127.0.0.1 --port 8108 &
RUST_PID=$!
PIDS_TO_KILL+=($RUST_PID)
sleep 2

# Verify gateway is up
if ! curl -sf http://127.0.0.1:8108/health > /dev/null 2>&1; then
    echo "ERROR: Rust gateway failed to start"
    exit 1
fi
echo "      Rust gateway running (PID $RUST_PID)"
echo ""

# Step 5: Capture Rust memory
echo "[5/7] Capturing Rust gateway memory..."
RUST_RSS=$(ps -o rss= -p $RUST_PID 2>/dev/null || echo "0")
RUST_RSS_MB=$(echo "scale=1; ${RUST_RSS:-0} / 1024" | bc 2>/dev/null || echo "N/A")
echo "      Rust gateway RSS: ${RUST_RSS_MB}MB"
echo ""

# Step 6: Run Rust benchmark
echo "[6/7] Running Rust gateway benchmark..."
cd "$PROJECT_ROOT"
uv run python "$SCRIPT_DIR/bench_rust.py"
echo ""

# Step 7: Capture Rust memory after load
RUST_RSS_AFTER=$(ps -o rss= -p $RUST_PID 2>/dev/null || echo "0")
RUST_RSS_AFTER_MB=$(echo "scale=1; ${RUST_RSS_AFTER:-0} / 1024" | bc 2>/dev/null || echo "N/A")

# Step 8: Measure startup time
echo "[7/7] Measuring Rust gateway startup time..."
# Kill existing gateway
kill $RUST_PID 2>/dev/null || true
wait $RUST_PID 2>/dev/null || true
sleep 1

START_TS=$(python3 -c "import time; print(time.perf_counter())")
RUST_LOG=warn "$RUST_BINARY" --host 127.0.0.1 --port 8109 &
STARTUP_PID=$!
PIDS_TO_KILL+=($STARTUP_PID)

# Poll for health
for i in $(seq 1 50); do
    if curl -sf http://127.0.0.1:8109/health > /dev/null 2>&1; then
        break
    fi
    sleep 0.02
done
END_TS=$(python3 -c "import time; print(time.perf_counter())")
STARTUP_MS=$(python3 -c "print(f'{($END_TS - $START_TS) * 1000:.0f}')")
echo "      Startup time: ${STARTUP_MS}ms"
echo ""

# Run comparison
echo "============================================================"
uv run python "$SCRIPT_DIR/compare.py"
echo ""

# Print resource summary
echo "============================================================"
echo "Resource Summary"
echo "============================================================"
echo "  Rust binary size:        $(ls -lh "$RUST_BINARY" | awk '{print $5}')"
echo "  Rust RSS (idle):         ${RUST_RSS_MB}MB"
echo "  Rust RSS (after bench):  ${RUST_RSS_AFTER_MB}MB"
echo "  Rust startup time:       ${STARTUP_MS}ms"
echo "  Mock vLLM RSS:           ${PYTHON_RSS_MB}MB"
echo "============================================================"

# Save resource info
cat > "$SCRIPT_DIR/resources.json" << EOF
{
  "rust_binary_size_bytes": $(stat -f%z "$RUST_BINARY" 2>/dev/null || stat --printf=%s "$RUST_BINARY" 2>/dev/null || echo 0),
  "rust_rss_idle_kb": ${RUST_RSS:-0},
  "rust_rss_after_bench_kb": ${RUST_RSS_AFTER:-0},
  "rust_startup_ms": $STARTUP_MS,
  "mock_vllm_rss_kb": ${PYTHON_RSS:-0}
}
EOF
echo ""
echo "All results saved to $SCRIPT_DIR/"

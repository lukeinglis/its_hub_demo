"""Compare Python vs Rust benchmark results side-by-side."""
import json
import os
import sys


def load_results(path):
    with open(path) as f:
        return json.load(f)


def main():
    bench_dir = os.path.dirname(__file__)
    python_path = os.path.join(bench_dir, "results_python.json")
    rust_path = os.path.join(bench_dir, "results_rust.json")

    if not os.path.exists(python_path):
        print(f"Missing {python_path}. Run bench_python.py first.")
        sys.exit(1)
    if not os.path.exists(rust_path):
        print(f"Missing {rust_path}. Run bench_rust.py first.")
        sys.exit(1)

    python_results = load_results(python_path)
    rust_results = load_results(rust_path)

    print()
    print("=" * 78)
    print("ITS Hub: Python (direct) vs Rust Gateway Benchmark Comparison")
    print("=" * 78)
    print(
        f"{'Budget':>7} | {'Python p50':>11} | {'Rust p50':>11} | {'Speedup':>8} "
        f"| {'Python p95':>11} | {'Rust p95':>11} | {'Speedup':>8}"
    )
    print("-" * 78)

    summary = []
    for budget in sorted(python_results.keys(), key=lambda x: int(x)):
        py = python_results[budget]
        rs = rust_results.get(str(budget), {})
        if not rs:
            continue
        speedup_p50 = py["p50"] / rs["p50"] if rs["p50"] > 0 else float("inf")
        speedup_p95 = py["p95"] / rs["p95"] if rs["p95"] > 0 else float("inf")
        print(
            f"{budget:>7} | {py['p50']:>9.1f}ms | {rs['p50']:>9.1f}ms | {speedup_p50:>6.2f}x "
            f"| {py['p95']:>9.1f}ms | {rs['p95']:>9.1f}ms | {speedup_p95:>6.2f}x"
        )
        summary.append(
            {
                "budget": int(budget),
                "python_p50": py["p50"],
                "rust_p50": rs["p50"],
                "speedup_p50": round(speedup_p50, 2),
                "python_p95": py["p95"],
                "rust_p95": rs["p95"],
                "speedup_p95": round(speedup_p95, 2),
                "python_mean": py["mean"],
                "rust_mean": rs["mean"],
            }
        )

    print()
    print("Note: Python benchmark calls the library directly (no HTTP server).")
    print("      Rust benchmark goes through the gateway HTTP layer.")
    print("      Both share the same mock vLLM backend (5ms simulated latency).")
    print("      Speedup reflects orchestration overhead difference, not model inference.")

    # Save comparison summary
    out_path = os.path.join(bench_dir, "comparison.json")
    with open(out_path, "w") as f:
        json.dump(summary, f, indent=2)
    print(f"\nComparison saved to {out_path}")


if __name__ == "__main__":
    main()

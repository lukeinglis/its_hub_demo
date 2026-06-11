"""Benchmark Python ITS algorithms directly (no gateway server)."""
import asyncio
import json
import os
import sys
import time

# Add project root to path so we can import its_hub
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))

from its_hub.api.types import ChatMessage
from its_hub.core.algorithms.self_consistency import SelfConsistency
from its_hub.core.lms.openai_lm import OpenAICompatibleLanguageModel


async def benchmark():
    lm = OpenAICompatibleLanguageModel(
        endpoint="http://127.0.0.1:9999/v1",
        api_key="mock",
        model_name="mock-model",
        max_concurrency=64,
    )

    sc = SelfConsistency()
    messages = [ChatMessage(role="user", content="What is 2+2?")]

    budgets = [1, 4, 8, 16, 32, 64]
    iterations = 20

    results = {}

    # Warmup: 3 iterations at budget=1
    for _ in range(3):
        await sc.ainfer(lm, messages, budget=1, return_response_only=True)

    print("Python ITS Benchmark (Self-Consistency, direct library call)")
    print("=" * 60)

    for budget in budgets:
        latencies = []
        for _ in range(iterations):
            start = time.perf_counter()
            await sc.ainfer(lm, messages, budget=budget, return_response_only=True)
            elapsed = (time.perf_counter() - start) * 1000  # ms
            latencies.append(elapsed)

        latencies.sort()
        n = len(latencies)
        results[budget] = {
            "p50": latencies[n // 2],
            "p95": latencies[int(n * 0.95)],
            "p99": latencies[int(n * 0.99)],
            "mean": sum(latencies) / n,
            "min": latencies[0],
            "max": latencies[-1],
        }
        print(
            f"Budget {budget:3d}: "
            f"p50={results[budget]['p50']:7.1f}ms  "
            f"p95={results[budget]['p95']:7.1f}ms  "
            f"mean={results[budget]['mean']:7.1f}ms"
        )

    await lm.close()

    out_path = os.path.join(os.path.dirname(__file__), "results_python.json")
    with open(out_path, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nResults saved to {out_path}")


if __name__ == "__main__":
    asyncio.run(benchmark())

"""Benchmark Rust gateway via HTTP (same measurement technique as Python bench)."""
import asyncio
import json
import os
import time

import aiohttp


async def benchmark():
    gateway_url = "http://127.0.0.1:8108"

    # Configure gateway to use self-consistency with mock backend
    async with aiohttp.ClientSession() as session:
        async with session.post(
            f"{gateway_url}/configure",
            json={
                "endpoint": "http://127.0.0.1:9999/v1",
                "api_key": "mock",
                "model": "mock-model",
                "alg": "self-consistency",
                "regex_patterns": ["\\\\boxed\\{(.+?)\\}"],
            },
        ) as resp:
            config_result = await resp.json()
            print(f"Gateway configured: {config_result.get('message', config_result)}")

    budgets = [1, 4, 8, 16, 32, 64]
    iterations = 20
    results = {}

    async with aiohttp.ClientSession() as session:
        # Warmup: 3 iterations at budget=1
        for _ in range(3):
            payload = {
                "model": "mock-model",
                "messages": [{"role": "user", "content": "What is 2+2?"}],
                "budget": 1,
            }
            async with session.post(
                f"{gateway_url}/v1/chat/completions", json=payload
            ) as resp:
                await resp.json()

        print("\nRust Gateway Benchmark (Self-Consistency, via HTTP)")
        print("=" * 60)

        for budget in budgets:
            latencies = []
            for _ in range(iterations):
                payload = {
                    "model": "mock-model",
                    "messages": [{"role": "user", "content": "What is 2+2?"}],
                    "budget": budget,
                }
                start = time.perf_counter()
                async with session.post(
                    f"{gateway_url}/v1/chat/completions", json=payload
                ) as resp:
                    await resp.json()
                elapsed = (time.perf_counter() - start) * 1000
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

    out_path = os.path.join(os.path.dirname(__file__), "results_rust.json")
    with open(out_path, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nResults saved to {out_path}")


if __name__ == "__main__":
    asyncio.run(benchmark())

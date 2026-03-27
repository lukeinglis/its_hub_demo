"""
FastAPI backend for ITS demo.

App setup, CORS, static file serving, and route handlers.
Inference logic is in inference.py, trace building in traces.py.
"""

import asyncio
import logging
import os
import socket
import time as _time
import uuid
from collections import defaultdict
from pathlib import Path
from urllib.parse import urlparse

from dotenv import load_dotenv
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from fastapi.responses import FileResponse

# Configure logging first
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# Load environment variables from .env file
env_path = Path(__file__).parent.parent / ".env"
logger.info(f"Loading .env file from: {env_path}")
logger.info(f".env file exists: {env_path.exists()}")
load_dotenv(dotenv_path=env_path)

# Verify API key is loaded (log only if exists, don't log the actual key)
openai_key = os.getenv("OPENAI_API_KEY")
if openai_key:
    logger.info("OPENAI_API_KEY loaded successfully")
else:
    logger.warning("OPENAI_API_KEY not found in environment!")

from backend.evaluation import evaluate_correctness
from its_hub.utils import QWEN_SYSTEM_PROMPT

from .config import get_model_config, MODEL_REGISTRY
from .example_questions import (
    get_all_questions,
    get_questions_by_algorithm,
    get_tool_calling_questions,
    get_tool_calling_questions_by_algorithm,
)
from .models import (
    CompareRequest,
    CompareResponse,
    ResultDetail,
    HealthResponse,
)
from .inference import (
    create_language_model,
    run_baseline,
    run_its,
    calculate_cost,
    detect_question_type,
)

# ── FastAPI App ───────────────────────────────────────────────────────

app = FastAPI(
    title="ITS Demo API",
    description="Demo API for comparing baseline vs ITS (Inference-Time Scaling)",
    version="1.0.0",
)

# Configure CORS — override defaults via CORS_ORIGINS env var (comma-separated)
cors_origins_str = os.getenv("CORS_ORIGINS", "http://localhost:8000,http://127.0.0.1:8000,https://lukeinglis.github.io")
cors_origins = [origin.strip() for origin in cors_origins_str.split(",")]
app.add_middleware(
    CORSMiddleware,
    allow_origins=cors_origins,
    allow_credentials=True,
    allow_methods=["GET", "POST"],
    allow_headers=["Content-Type"],
)

frontend_dir = Path(__file__).parent.parent / "frontend"


# ── Rate Limiting ─────────────────────────────────────────────────────

MAX_REQUESTS_PER_HOUR = int(os.getenv("RATE_LIMIT_REQUESTS", "100"))
MAX_BUDGET_PER_HOUR = int(os.getenv("RATE_LIMIT_BUDGET", "500"))

_request_log: dict[str, list[tuple[float, int]]] = defaultdict(list)


def _check_rate_limit(client_ip: str, budget: int) -> None:
    """Enforce per-IP rate limiting on requests and total budget."""
    now = _time.monotonic()
    cutoff = now - 3600
    # Prune old entries
    _request_log[client_ip] = [
        (t, b) for t, b in _request_log[client_ip] if t > cutoff
    ]
    entries = _request_log[client_ip]
    if len(entries) >= MAX_REQUESTS_PER_HOUR:
        raise HTTPException(status_code=429, detail="Too many requests. Please try again later.")
    total_budget = sum(b for _, b in entries)
    if total_budget + budget > MAX_BUDGET_PER_HOUR:
        raise HTTPException(status_code=429, detail="Budget limit exceeded. Please try again later.")
    _request_log[client_ip].append((now, budget))


# ── Routes ────────────────────────────────────────────────────────────

@app.get("/")
async def serve_frontend():
    """Serve the frontend HTML."""
    frontend_path = frontend_dir / "index.html"
    return FileResponse(frontend_path)


@app.get("/health", response_model=HealthResponse)
async def health_check():
    """Health check endpoint."""
    return HealthResponse(
        status="ok",
        message="ITS Demo API is running"
    )


def check_server_available(base_url: str, timeout: float = 1.0) -> bool:
    """Check if a server is available by attempting to connect to it."""
    try:
        parsed = urlparse(base_url)
        host = parsed.hostname or 'localhost'
        port = parsed.port or 8100

        # Try to connect to the server
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(timeout)
        result = sock.connect_ex((host, port))
        sock.close()

        return result == 0
    except Exception as e:
        logger.debug(f"Server check failed for {base_url}: {e}")
        return False


def _check_ollama_model_sync(base_url: str, model_name: str, timeout: float = 1.0) -> bool:
    """Synchronous Ollama model check (run via asyncio.to_thread)."""
    import urllib.request
    import json as _json
    try:
        parsed = urlparse(base_url)
        host = parsed.hostname or 'localhost'
        port = parsed.port or 11434
        tags_url = f"http://{host}:{port}/api/tags"
        req = urllib.request.Request(tags_url)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = _json.loads(resp.read())
            available_models = [m.get("name", "") for m in data.get("models", [])]
            return model_name in available_models
    except Exception as e:
        logger.debug(f"Ollama model check failed for {model_name}: {e}")
        return False


async def check_ollama_model_available(base_url: str, model_name: str, timeout: float = 1.0) -> bool:
    """Check if a specific model is pulled and available in Ollama (non-blocking)."""
    return await asyncio.to_thread(_check_ollama_model_sync, base_url, model_name, timeout)


def _get_provider_group(config: dict) -> str:
    """Determine the provider group for a model config."""
    provider = config.get("provider", "")
    base_url = config.get("base_url", "")
    if provider in ("vertex_ai", "vertex_ai_model_garden"):
        return "vertex_ai"
    if "openrouter.ai" in base_url:
        return "openrouter"
    if "api.openai.com" in base_url:
        return "openai"
    return "local"


@app.get("/providers")
async def check_providers():
    """Check which model providers have credentials configured."""
    openai_key = os.getenv("OPENAI_API_KEY")
    vertex_project = os.getenv("VERTEX_PROJECT")
    vllm_url = os.getenv("VLLM_BASE_URL")

    ollama_url = os.getenv("OLLAMA_BASE_URL", "http://localhost:11434/v1")
    local_available = False
    if vllm_url:
        local_available = check_server_available(vllm_url)
    if not local_available:
        local_available = check_server_available(ollama_url)

    providers = {
        "openai": {
            "enabled": bool(openai_key),
            "name": "OpenAI",
            "description": "GPT-4o, GPT-4o Mini, GPT-4.1, GPT-4.1 Mini/Nano, GPT-3.5 Turbo",
            "env_var": "OPENAI_API_KEY",
            "setup": "export OPENAI_API_KEY=sk-...",
        },
        "vertex_ai": {
            "enabled": bool(vertex_project),
            "name": "Google Cloud Vertex AI",
            "description": "Claude Sonnet 4.6, Claude Haiku 4.5",
            "env_var": "VERTEX_PROJECT",
            "setup": "export VERTEX_PROJECT=your-project-id\ngcloud auth application-default login",
        },
        "local": {
            "enabled": local_available,
            "name": "Self-Hosted / Local",
            "description": "IBM Granite 4 3B, Granite 3.3 8B, or any model via Ollama / vLLM",
            "env_var": "OLLAMA_BASE_URL",
            "setup": "ollama pull granite4:3b && ollama serve",
        },
    }

    any_enabled = any(p["enabled"] for p in providers.values())
    return {"providers": providers, "any_enabled": any_enabled}


@app.get("/models")
async def list_models(use_case: str | None = None):
    """
    List available models. Only include models where the server is available.

    Query params:
        use_case: Optional use case to filter models by (e.g., 'tool_consensus')
    """
    available_models = []

    # Determine which providers are enabled
    enabled_providers = {
        "openai": bool(os.getenv("OPENAI_API_KEY")),
        "openrouter": bool(os.getenv("OPENROUTER_API_KEY")),
        "vertex_ai": bool(os.getenv("VERTEX_PROJECT")),
    }

    for model_id, config in MODEL_REGISTRY.items():
        # Filter out models that don't support tools for tool_consensus use case
        if use_case == "tool_consensus":
            supports_tools = config.get("supports_tools", False)
            if not supports_tools:
                continue

        # Check if model requires external server (has non-standard base_url)
        base_url = config.get("base_url", "")
        provider_group = _get_provider_group(config)

        # Skip models from disabled providers
        if provider_group in enabled_providers and not enabled_providers[provider_group]:
            continue

        model_entry = {
            "id": model_id,
            "description": config["description"],
            "model_name": config["model_name"],
            "size": config.get("size", "Unknown"),
            "supports_tools": config.get("supports_tools", False),
            "is_reasoning": config.get("is_reasoning", False),
            "provider": provider_group,
        }

        # Skip server check for standard API-based models
        if (base_url.startswith("https://api.openai.com") or
            base_url.startswith("https://openrouter.ai") or
            config.get("provider") in ("vertex_ai", "vertex_ai_model_garden") or
            not base_url):
            available_models.append(model_entry)
            continue

        # For custom endpoints (Granite, local vLLM), check if server is available
        server_available = check_server_available(base_url, timeout=1.0)

        if server_available:
            # For Ollama-backed models, also verify the model is actually pulled
            if "11434" in base_url:
                model_name = config.get("model_name", "")
                if not await check_ollama_model_available(base_url, model_name):
                    continue
            available_models.append(model_entry)

    return {"models": available_models}


@app.get("/examples")
async def list_examples(algorithm: str | None = None, use_case: str | None = None):
    """
    Get example questions.

    Query params:
        algorithm: Optional algorithm to filter questions by (e.g., 'beam_search')
        use_case: Optional use case to filter questions by (e.g., 'tool_consensus')
    """
    # Get tool calling questions if use_case is tool_consensus
    if use_case == "tool_consensus":
        if algorithm:
            questions = get_tool_calling_questions_by_algorithm(algorithm, limit=10)
        else:
            questions = get_tool_calling_questions()
    else:
        # Regular math questions
        if algorithm:
            questions = get_questions_by_algorithm(algorithm, limit=10)
        else:
            questions = get_all_questions()

    return {
        "examples": [
            {
                "question": q["question"],
                "category": q["category"],
                "difficulty": q["difficulty"],
                "expected_answer": q["expected_answer"],
                "best_for": q["best_for"],
                "why": q["why"],
                "source": q.get("source", "unknown"),
                "source_id": q.get("source_id", ""),
                "expected_tools": q.get("expected_tools", []),
            }
            for q in questions
        ]
    }


@app.post("/compare", response_model=CompareResponse)
async def compare(request: CompareRequest, req: Request):
    """
    Compare baseline vs ITS inference.

    Input:
        - question: The question to answer
        - model_id: Model identifier from the registry
        - algorithm: ITS algorithm (best_of_n or self_consistency)
        - budget: Computation budget

    Output:
        - baseline: { answer, latency_ms, ... }
        - its: { answer, latency_ms, ... }
        - meta: { model_id, algorithm, budget, run_id }
    """
    run_id = str(uuid.uuid4())

    # Rate limiting
    client_ip = req.client.host if req.client else "unknown"
    _check_rate_limit(client_ip, request.budget)

    logger.info(
        f"[{run_id}] Starting comparison: "
        f"model={request.model_id}, algorithm={request.algorithm}, budget={request.budget}"
    )

    try:
        # Get API key for judge (always use OpenAI for judge)
        openai_key = os.getenv("OPENAI_API_KEY")
        if not openai_key:
            raise ValueError("OPENAI_API_KEY required for LLM judge")

        # Detect question type if auto
        question_type = request.question_type
        if question_type == "auto":
            question_type = detect_question_type(
                request.question,
                enable_tools=request.enable_tools,
                question_metadata=None  # Could enhance to pass metadata from examples
            )
            logger.info(f"[{run_id}] Auto-detected question type: {question_type}")

        # Select system prompt based on question type
        system_prompt = None
        if question_type == "math":
            system_prompt = QWEN_SYSTEM_PROMPT
            logger.info(f"[{run_id}] Using QWEN math system prompt")

        # Create language models based on use case
        small_baseline_answer = None
        small_baseline_latency = None
        small_baseline_input_tokens = None
        small_baseline_output_tokens = None
        small_baseline_tool_calls = None

        if request.use_case == "match_frontier":
            # Use Case 2: Small model + ITS vs Large frontier model
            if not request.frontier_model_id:
                raise ValueError("frontier_model_id required for match_frontier use case")

            small_model = create_language_model(request.model_id, system_prompt)
            frontier_model = create_language_model(request.frontier_model_id, system_prompt)

            # Run small baseline first to get token counts for ITS estimation
            small_baseline_answer, small_baseline_latency, small_baseline_input_tokens, small_baseline_output_tokens, small_baseline_tool_calls = await run_baseline(
                small_model, request.question, enable_tools=request.enable_tools
            )

            # Run ITS and frontier baseline in parallel
            its_task = run_its(
                small_model,
                request.question,
                request.algorithm,
                request.budget,
                openai_key,
                small_baseline_input_tokens,
                small_baseline_output_tokens,
                enable_tools=request.enable_tools,
                tool_vote=request.tool_vote,
                exclude_args=request.exclude_args,
                question_type=question_type,
                judge_criterion=request.judge_criterion,
            )
            frontier_baseline_task = run_baseline(frontier_model, request.question, enable_tools=request.enable_tools)

            (its_answer, its_latency, its_input_tokens, its_output_tokens, its_trace, its_tool_calls), (baseline_answer, baseline_latency, baseline_input_tokens, baseline_output_tokens, baseline_tool_calls) = await asyncio.gather(
                its_task,
                frontier_baseline_task,
            )
        elif request.use_case == "tool_consensus":
            # Use Case 3: Tool calling consensus - show baseline vs ITS with tool voting
            lm = create_language_model(request.model_id, system_prompt)

            # Always enable tools for this use case
            enable_tools = True

            # Run baseline with tools but no voting
            baseline_answer, baseline_latency, baseline_input_tokens, baseline_output_tokens, baseline_tool_calls = await run_baseline(
                lm, request.question, enable_tools=enable_tools
            )

            # Run ITS with tool voting enabled
            its_answer, its_latency, its_input_tokens, its_output_tokens, its_trace, its_tool_calls = await run_its(
                lm,
                request.question,
                request.algorithm,
                request.budget,
                openai_key,
                baseline_input_tokens,
                baseline_output_tokens,
                enable_tools=enable_tools,
                tool_vote=request.tool_vote or "tool_name",  # Default to tool_name voting
                exclude_args=request.exclude_args,
                question_type=question_type,
                judge_criterion=request.judge_criterion,
            )

        else:
            # Use Case 1: Same model with/without ITS (default)
            lm = create_language_model(request.model_id, system_prompt)

            # Run baseline first to get token counts for ITS estimation
            baseline_answer, baseline_latency, baseline_input_tokens, baseline_output_tokens, baseline_tool_calls = await run_baseline(
                lm, request.question, enable_tools=request.enable_tools
            )

            # Run ITS with token estimates
            its_answer, its_latency, its_input_tokens, its_output_tokens, its_trace, its_tool_calls = await run_its(
                lm,
                request.question,
                request.algorithm,
                request.budget,
                openai_key,
                baseline_input_tokens,
                baseline_output_tokens,
                enable_tools=request.enable_tools,
                tool_vote=request.tool_vote,
                exclude_args=request.exclude_args,
                question_type=question_type,
                judge_criterion=request.judge_criterion,
            )

        logger.info(
            f"[{run_id}] Comparison complete: "
            f"baseline_latency={baseline_latency}ms, its_latency={its_latency}ms"
        )

        # --- Quality evaluation ---
        baseline_is_correct = None
        baseline_eval_method = None
        its_is_correct = None
        its_eval_method = None
        small_baseline_is_correct = None
        small_baseline_eval_method = None

        if request.expected_answer:
            eval_tasks = [
                evaluate_correctness(request.question, baseline_answer, request.expected_answer, question_type),
                evaluate_correctness(request.question, its_answer, request.expected_answer, question_type),
            ]
            if request.use_case == "match_frontier" and small_baseline_answer is not None:
                eval_tasks.append(
                    evaluate_correctness(request.question, small_baseline_answer, request.expected_answer, question_type)
                )

            eval_results = await asyncio.gather(*eval_tasks)

            baseline_is_correct, baseline_eval_method = eval_results[0]
            its_is_correct, its_eval_method = eval_results[1]
            if len(eval_results) > 2:
                small_baseline_is_correct, small_baseline_eval_method = eval_results[2]

            logger.info(
                f"[{run_id}] Quality evaluation: baseline={baseline_is_correct}, "
                f"its={its_is_correct}, method={its_eval_method}"
            )

        # Get model configs
        model_config = get_model_config(request.model_id)
        model_size = model_config.get("size", "Unknown")

        frontier_model_config = None
        frontier_model_size = None
        if request.use_case == "match_frontier" and request.frontier_model_id:
            frontier_model_config = get_model_config(request.frontier_model_id)
            frontier_model_size = frontier_model_config.get("size", "Unknown")

        # Calculate costs
        if request.use_case == "match_frontier":
            # Small baseline cost
            small_baseline_cost = calculate_cost(
                model_config,
                small_baseline_input_tokens or 0,
                small_baseline_output_tokens or 0
            )
            # ITS cost (small model)
            its_cost = calculate_cost(
                model_config,
                its_input_tokens or 0,
                its_output_tokens or 0
            )
            # Frontier baseline cost
            baseline_cost = calculate_cost(
                frontier_model_config,
                baseline_input_tokens or 0,
                baseline_output_tokens or 0
            )
        else:
            # Both use same model
            baseline_cost = calculate_cost(
                model_config,
                baseline_input_tokens or 0,
                baseline_output_tokens or 0
            )
            its_cost = calculate_cost(
                model_config,
                its_input_tokens or 0,
                its_output_tokens or 0
            )
            small_baseline_cost = None

        # Build response
        response_data = {
            "baseline": ResultDetail(
                answer=baseline_answer,
                latency_ms=baseline_latency,
                model_size=frontier_model_size if request.use_case == "match_frontier" else model_size,
                cost_usd=baseline_cost if baseline_cost is not None else None,
                input_tokens=baseline_input_tokens if baseline_input_tokens else None,
                output_tokens=baseline_output_tokens if baseline_output_tokens else None,
                is_correct=baseline_is_correct,
                eval_method=baseline_eval_method,
                tool_calls=baseline_tool_calls if baseline_tool_calls else None,
            ),
            "its": ResultDetail(
                answer=its_answer,
                latency_ms=its_latency,
                model_size=model_size,
                cost_usd=its_cost if its_cost is not None else None,
                input_tokens=its_input_tokens if its_input_tokens else None,
                output_tokens=its_output_tokens if its_output_tokens else None,
                tokens_estimated=True,
                is_correct=its_is_correct,
                eval_method=its_eval_method,
                trace=its_trace,
                tool_calls=its_tool_calls if its_tool_calls else None,
            ),
            "meta": {
                "model_id": request.model_id,
                "algorithm": request.algorithm,
                "budget": request.budget,
                "run_id": run_id,
                "use_case": request.use_case,
                "question_type": question_type,
                "expected_answer": request.expected_answer,
                "infrastructure": {
                    "model": {
                        "self_hostable": model_config.get("self_hostable", False),
                        "min_gpu": model_config.get("min_gpu"),
                        "gpu_cloud_cost_hr": model_config.get("gpu_cloud_cost_hr"),
                    },
                    "frontier": {
                        "self_hostable": frontier_model_config.get("self_hostable", False) if frontier_model_config else False,
                        "min_gpu": frontier_model_config.get("min_gpu") if frontier_model_config else None,
                        "gpu_cloud_cost_hr": frontier_model_config.get("gpu_cloud_cost_hr") if frontier_model_config else None,
                    } if frontier_model_config else None,
                },
            }
        }

        # Add small baseline if match_frontier use case
        if request.use_case == "match_frontier" and small_baseline_answer is not None:
            response_data["small_baseline"] = ResultDetail(
                answer=small_baseline_answer,
                latency_ms=small_baseline_latency,
                model_size=model_size,
                cost_usd=small_baseline_cost if small_baseline_cost is not None else None,
                input_tokens=small_baseline_input_tokens if small_baseline_input_tokens else None,
                output_tokens=small_baseline_output_tokens if small_baseline_output_tokens else None,
                is_correct=small_baseline_is_correct,
                eval_method=small_baseline_eval_method,
                tool_calls=small_baseline_tool_calls if small_baseline_tool_calls else None,
            )
            response_data["meta"]["frontier_model_id"] = request.frontier_model_id

        response = CompareResponse(**response_data)

        return response

    except ValueError as e:
        logger.error(f"[{run_id}] Validation error: {e}")
        raise HTTPException(status_code=400, detail=str(e))
    except asyncio.TimeoutError:
        logger.error(f"[{run_id}] Request timed out")
        raise HTTPException(
            status_code=504,
            detail="Request timed out. Try a smaller budget or a different model."
        )
    except Exception as e:
        logger.error(f"[{run_id}] Error during comparison: {type(e).__name__}", exc_info=True)
        raise HTTPException(status_code=500, detail="An unexpected error occurred. Please try again.")


# Mount frontend static files at root so relative paths in index.html resolve
# correctly. This must come after all API route definitions because FastAPI
# mounts are greedy — a "/" mount placed before routes would shadow them.
app.mount("/", StaticFiles(directory=str(frontend_dir)), name="frontend")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)

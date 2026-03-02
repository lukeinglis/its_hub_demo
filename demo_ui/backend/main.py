"""
FastAPI backend for ITS demo.

Exposes two endpoints:
- GET /health: Health check
- POST /compare: Compare baseline vs ITS
"""

import asyncio
import json
import logging
import os
import re
import socket
import time
import traceback
import uuid
from pathlib import Path
from typing import Dict
from urllib.parse import urlparse

import litellm
import numpy as np
from dotenv import load_dotenv
from fastapi import FastAPI, HTTPException
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
    logger.info(f"OPENAI_API_KEY loaded successfully (length: {len(openai_key)})")
else:
    logger.warning("OPENAI_API_KEY not found in environment!")

from its_hub.lms import OpenAICompatibleLanguageModel, LiteLLMLanguageModel, StepGeneration
from its_hub.algorithms import (
    BestOfN,
    SelfConsistency,
    BeamSearch,
    ParticleFiltering,
    EntropicParticleFiltering,
    ParticleGibbs,
)
from its_hub.integration.reward_hub import LLMJudgeRewardModel
from its_hub.types import ChatMessage
from its_hub.utils import extract_content_from_lm_response, QWEN_SYSTEM_PROMPT
from its_hub.base import AbstractLanguageModel
from its_hub.algorithms.self_consistency import create_regex_projection_function

from .config import get_model_config, get_api_key, MODEL_REGISTRY, ModelConfig
from .vertex_lm import VertexAIClaudeModel, VertexAIGeminiModel
from .llm_prm import LLMProcessRewardModel

# ── Algorithm / model defaults ────────────────────────────────────────
DEFAULT_JUDGE_MODEL = "gpt-4o-mini"
DEFAULT_PRM_MODEL = "gpt-4o-mini"
DEFAULT_STEP_GEN_MAX_STEPS = 8
DEFAULT_STEP_GEN_TEMPERATURE = 0.8
DEFAULT_STEP_GEN_TOKEN = "\n\n"
DEFAULT_PRM_TEMPERATURE = 0.3
BEAM_WIDTH_MIN = 2
BEAM_WIDTH_MAX = 4
GIBBS_ITERATIONS_MIN = 2
GIBBS_ITERATIONS_MAX = 3
GIBBS_ITERATIONS_DIVISOR = 4


def parse_tool_args(tool_args):
    """Parse tool arguments, handling JSON string encoding."""
    if isinstance(tool_args, str):
        try:
            return json.loads(tool_args)
        except json.JSONDecodeError:
            return {}
    return tool_args


def calculate_cost(
    model_config: ModelConfig,
    input_tokens: int,
    output_tokens: int
) -> float:
    """
    Calculate cost in USD based on token usage and model pricing.

    Returns:
        Cost in USD (rounded to 4 decimal places)
    """
    input_cost_per_1m = model_config.get("input_cost_per_1m", 0.0)
    output_cost_per_1m = model_config.get("output_cost_per_1m", 0.0)

    if input_cost_per_1m == 0.0 and output_cost_per_1m == 0.0:
        return 0.0

    input_cost = (input_tokens / 1_000_000) * input_cost_per_1m
    output_cost = (output_tokens / 1_000_000) * output_cost_per_1m
    total_cost = input_cost + output_cost

    return round(total_cost, 6)  # Round to 6 decimal places for precision


def detect_question_type(
    question: str,
    enable_tools: bool = False,
    question_metadata: dict | None = None
) -> str:
    """
    Detect question type: 'math', 'tool_calling', or 'general'.

    Args:
        question: The question text to analyze
        enable_tools: Whether tools are enabled for this question
        question_metadata: Optional metadata about the question

    Returns:
        Question type: "math", "tool_calling", or "general"
    """
    # Check metadata first
    if question_metadata:
        if question_metadata.get("expected_tools") or question_metadata.get("source") == "tool_calling":
            return "tool_calling"

    # Check if tools enabled
    if enable_tools:
        return "tool_calling"

    # Check for math patterns
    math_indicators = [
        r'\$', r'\\frac', r'\\boxed', r'\^',
        r'probability', r'calculate', r'solve',
        r'find the value', r'what is the.*term'
    ]
    for pattern in math_indicators:
        if re.search(pattern, question, re.IGNORECASE):
            return "math"

    return "general"


def create_math_projection_function():
    """Create projection function for extracting boxed mathematical answers."""
    return create_regex_projection_function(r'\\boxed\{([^}]+)\}')


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
    CandidateResponse,
    SelfConsistencyTrace,
    BestOfNTrace,
    BeamSearchTrace,
    ParticleFilteringTrace,
    ParticleGibbsTrace,
    ToolCall,
    ToolVotingTrace,
)
from .tools import get_tool_schemas, execute_tool
from its_hub.algorithms.self_consistency import SelfConsistencyResult
from its_hub.algorithms.bon import BestOfNResult
from its_hub.algorithms.beam_search import BeamSearchResult
from its_hub.algorithms.particle_gibbs import ParticleFilteringResult, ParticleGibbsResult

# Create FastAPI app
app = FastAPI(
    title="ITS Demo API",
    description="Demo API for comparing baseline vs ITS (Inference-Time Scaling)",
    version="1.0.0",
)

# Configure CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "http://localhost:8000",
        "http://127.0.0.1:8000",
    ],
    allow_credentials=True,
    allow_methods=["GET", "POST"],
    allow_headers=["Content-Type"],
)

frontend_dir = Path(__file__).parent.parent / "frontend"


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
    openrouter_key = os.getenv("OPENROUTER_API_KEY")
    vertex_project = os.getenv("VERTEX_PROJECT")
    vllm_url = os.getenv("VLLM_BASE_URL")

    local_available = False
    if vllm_url:
        local_available = check_server_available(vllm_url)
    else:
        local_available = check_server_available("http://localhost:8100/v1")

    providers = {
        "openai": {
            "enabled": bool(openai_key),
            "name": "OpenAI",
            "description": "GPT-4o, GPT-4o Mini, GPT-4 Turbo, GPT-3.5 Turbo",
            "env_var": "OPENAI_API_KEY",
            "setup": "export OPENAI_API_KEY=sk-...",
        },
        "openrouter": {
            "enabled": bool(openrouter_key),
            "name": "OpenRouter",
            "description": "Llama 3, Mistral 7B, Qwen 2.5, Gemma 2, DeepSeek (15+ open-source models)",
            "env_var": "OPENROUTER_API_KEY",
            "setup": "export OPENROUTER_API_KEY=sk-or-...",
        },
        "vertex_ai": {
            "enabled": bool(vertex_project),
            "name": "Google Cloud Vertex AI",
            "description": "Claude 3.5 Sonnet/Opus/Haiku, Gemini 1.5 Pro/Flash",
            "env_var": "VERTEX_PROJECT",
            "setup": "export VERTEX_PROJECT=your-project-id\ngcloud auth application-default login",
        },
        "local": {
            "enabled": local_available,
            "name": "Self-Hosted / Local",
            "description": "Any model served via vLLM or OpenAI-compatible endpoint",
            "env_var": "VLLM_BASE_URL",
            "setup": "export VLLM_BASE_URL=http://localhost:8100/v1",
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

    for model_id, config in MODEL_REGISTRY.items():
        # Filter out models that don't support tools for tool_consensus use case
        if use_case == "tool_consensus":
            supports_tools = config.get("supports_tools", False)
            if not supports_tools:
                continue

        # Check if model requires external server (has non-standard base_url)
        base_url = config.get("base_url", "")
        provider_group = _get_provider_group(config)

        model_entry = {
            "id": model_id,
            "description": config["description"],
            "model_name": config["model_name"],
            "size": config.get("size", "Unknown"),
            "supports_tools": config.get("supports_tools", False),
            "provider": provider_group,
        }

        # Skip server check for standard OpenAI, OpenRouter, Vertex AI, and Model Garden models
        if (base_url.startswith("https://api.openai.com") or
            base_url.startswith("https://openrouter.ai") or
            config.get("provider") in ("vertex_ai", "vertex_ai_model_garden") or
            not base_url):
            available_models.append(model_entry)
            continue

        # For custom endpoints (Granite, local vLLM), check if server is available
        server_available = check_server_available(base_url, timeout=1.0)

        if server_available:
            available_models.append(model_entry)
        else:
            logger.debug(f"Skipping model {model_id} - server at {base_url} not available")

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


def create_language_model(
    model_id: str,
    system_prompt: str | None = None
) -> AbstractLanguageModel:
    """
    Create a language model instance based on the model configuration.

    All models use OpenAI-compatible endpoints except Vertex AI models.

    Args:
        model_id: Model identifier from the registry
        system_prompt: Optional system prompt to prepend to all messages
    """
    model_config = get_model_config(model_id)
    provider = model_config.get("provider", "openai")

    if provider == "vertex_ai":
        # Use native Vertex AI SDK for Claude and Gemini models (NOT OpenAI-compatible)
        vertex_project = model_config.get("vertex_project")
        vertex_location = model_config.get("vertex_location")

        if not vertex_project or vertex_project == "your-gcp-project-id":
            raise ValueError(
                "VERTEX_PROJECT not configured. Please set VERTEX_PROJECT "
                "environment variable in your .env file"
            )

        model_name = model_config["model_name"]

        # Determine if it's a Claude or Gemini model based on model name
        if "claude" in model_name.lower():
            logger.info(
                f"Creating Vertex AI Claude model: {model_name} "
                f"(project: {vertex_project}, location: {vertex_location})"
            )
            return VertexAIClaudeModel(
                project_id=vertex_project,
                location=vertex_location,
                model_name=model_name,
            )
        elif "gemini" in model_name.lower():
            logger.info(
                f"Creating Vertex AI Gemini model: {model_name} "
                f"(project: {vertex_project}, location: {vertex_location})"
            )
            return VertexAIGeminiModel(
                project_id=vertex_project,
                location=vertex_location,
                model_name=model_name,
            )
        else:
            raise ValueError(f"Unknown Vertex AI model type: {model_name}")

    elif provider == "vertex_ai_model_garden":
        # Open-source models (Llama, Mistral, etc.) hosted on Vertex AI Model Garden
        # Uses litellm's vertex_ai/ prefix for routing and Google ADC for auth
        vertex_project = model_config.get("vertex_project")
        vertex_location = model_config.get("vertex_location")

        if not vertex_project or vertex_project == "your-gcp-project-id":
            raise ValueError(
                "VERTEX_PROJECT not configured. Please set VERTEX_PROJECT "
                "environment variable in your .env file"
            )

        model_name = f"vertex_ai/{model_config['model_name']}"

        logger.info(
            f"Creating Vertex AI Model Garden model: {model_name} "
            f"(project: {vertex_project}, location: {vertex_location})"
        )

        return LiteLLMLanguageModel(
            model_name=model_name,
            vertex_project=vertex_project,
            vertex_location=vertex_location,
        )

    else:
        # All other models use OpenAI-compatible endpoints
        # This includes: OpenAI, OpenRouter (Claude, Gemini), Together AI (open-source), vLLM
        api_key = get_api_key(model_id)

        logger.info(f"Creating OpenAI-compatible model: {model_config['model_name']} via {model_config['base_url']}")

        return OpenAICompatibleLanguageModel(
            endpoint=model_config["base_url"],
            api_key=api_key,
            model_name=model_config["model_name"],
            system_prompt=system_prompt,
        )


async def run_baseline(
    lm: AbstractLanguageModel,
    question: str,
    enable_tools: bool = False
) -> tuple[str, int, int, int, list[ToolCall] | None]:
    """
    Run baseline inference (single completion, no ITS).

    Returns:
        (answer, latency_ms, input_tokens, output_tokens, tool_calls)
    """
    start_time = time.time()

    messages = [ChatMessage(role="user", content=question)]

    # Prepare tools if enabled
    tools = get_tool_schemas() if enable_tools else None
    tool_choice = "auto" if enable_tools else None

    # Get response and try to capture usage information
    input_tokens = 0
    output_tokens = 0

    # For OpenAI-compatible models, try to get usage directly via litellm
    if isinstance(lm, OpenAICompatibleLanguageModel):
        try:
            request_data = lm._prepare_request_data(
                messages,
                stop=None,
                max_tokens=None,
                temperature=None,
                include_stop_str_in_output=None,
                tools=tools,
                tool_choice=tool_choice,
            )
            full_response = await litellm.acompletion(**request_data)

            # Extract usage from full response
            if hasattr(full_response, 'usage') and full_response.usage:
                input_tokens = getattr(full_response.usage, 'prompt_tokens', 0)
                output_tokens = getattr(full_response.usage, 'completion_tokens', 0)

            # Extract message
            response = full_response.choices[0].message.dict()
        except Exception as e:
            logger.warning(f"Could not capture token usage: {e}")
            response = await lm.agenerate(messages)
    else:
        # For Vertex AI or other models, use standard interface
        response = await lm.agenerate(messages)

    answer = extract_content_from_lm_response(response)

    # Extract and execute tool calls if present
    tool_calls_list = None
    if enable_tools and "tool_calls" in response and response["tool_calls"]:
        tool_calls_list = []
        for tc in response["tool_calls"]:
            tool_name = tc.get("function", {}).get("name", "")
            tool_args = parse_tool_args(tc.get("function", {}).get("arguments", {}))
            tool_result = execute_tool(tool_name, tool_args)

            tool_calls_list.append(ToolCall(
                name=tool_name,
                arguments=tool_args,
                result=tool_result
            ))

            if tool_result:
                answer += f"\n\n**Tool Used:** {tool_name}\n**Result:** {tool_result}"

    latency_ms = int((time.time() - start_time) * 1000)

    return answer, latency_ms, input_tokens, output_tokens, tool_calls_list


def _build_pf_trace(result: ParticleFilteringResult) -> ParticleFilteringTrace:
    """Build a ParticleFilteringTrace from a ParticleFilteringResult."""
    log_w = result.log_weights_lst
    # Normalize log-weights to probabilities
    max_lw = max(log_w) if log_w else 0.0
    exp_w = [np.exp(lw - max_lw) for lw in log_w]
    sum_w = sum(exp_w)
    normalized = [w / sum_w for w in exp_w] if sum_w > 0 else [1.0 / len(log_w)] * len(log_w)

    candidates = []
    for i, resp in enumerate(result.responses):
        content = extract_content_from_lm_response(resp)
        candidates.append(CandidateResponse(
            index=i,
            content=content,
            is_selected=(i == result.selected_index),
        ))

    return ParticleFilteringTrace(
        candidates=candidates,
        log_weights=[round(w, 4) for w in log_w],
        normalized_weights=[round(w, 4) for w in normalized],
        steps_used=result.steps_used_lst,
    )


def build_trace(algorithm: str, result, tool_vote: str | None = None) -> dict | None:
    """Convert an algorithm Result object into a serializable trace dict."""
    try:
        logger.debug(f"Building trace for algorithm={algorithm}, tool_vote={tool_vote}, result type={type(result)}")
        if isinstance(result, SelfConsistencyResult):
            candidates = []
            for i, resp in enumerate(result.responses):
                content = extract_content_from_lm_response(resp)
                # Extract tool calls if present
                tool_calls_data = None
                if "tool_calls" in resp and resp["tool_calls"]:
                    tool_calls_data = []
                    for tc in resp["tool_calls"]:
                        tool_name = tc.get("function", {}).get("name", "unknown")
                        tool_args = parse_tool_args(tc.get("function", {}).get("arguments", {}))
                        tool_calls_data.append(ToolCall(
                            name=tool_name,
                            arguments=tool_args,
                            result=None,
                        ))

                candidates.append(CandidateResponse(
                    index=i,
                    content=content,
                    is_selected=(i == result.selected_index),
                    tool_calls=tool_calls_data,
                ))

            vote_counts = {str(k): v for k, v in result.response_counts.items()}

            # Build tool voting trace if tool voting was used
            tool_voting_trace = None
            if tool_vote and any(c.tool_calls for c in candidates):
                # Extract tool voting statistics
                tool_counts = {}
                total_tool_calls = 0
                for candidate in candidates:
                    if candidate.tool_calls:
                        total_tool_calls += 1
                        for tc in candidate.tool_calls:
                            key = tc.name if tool_vote == "tool_name" else str(tc.arguments)
                            tool_counts[key] = tool_counts.get(key, 0) + 1

                # Find winning tool (most votes)
                winning_tool = max(tool_counts, key=tool_counts.get) if tool_counts else "none"

                tool_voting_trace = ToolVotingTrace(
                    tool_vote_type=tool_vote,
                    tool_counts=tool_counts,
                    winning_tool=winning_tool,
                    total_tool_calls=total_tool_calls,
                )

            trace = SelfConsistencyTrace(
                candidates=candidates,
                vote_counts=vote_counts,
                total_votes=sum(result.response_counts.values()),
                tool_voting=tool_voting_trace,
            )
            return trace.model_dump()

        elif isinstance(result, BestOfNResult):
            candidates = []
            for i, resp in enumerate(result.responses):
                content = extract_content_from_lm_response(resp)
                candidates.append(CandidateResponse(
                    index=i,
                    content=content,
                    is_selected=(i == result.selected_index),
                ))
            trace = BestOfNTrace(
                candidates=candidates,
                scores=[round(s, 4) for s in result.scores],
                max_score=round(max(result.scores), 4),
                min_score=round(min(result.scores), 4),
            )
            return trace.model_dump()

        elif isinstance(result, BeamSearchResult):
            candidates = []
            for i, resp in enumerate(result.responses):
                content = extract_content_from_lm_response(resp)
                candidates.append(CandidateResponse(
                    index=i,
                    content=content,
                    is_selected=(i == result.selected_index),
                ))
            trace = BeamSearchTrace(
                candidates=candidates,
                scores=[round(s, 4) for s in result.scores],
                steps_used=result.steps_used,
            )
            return trace.model_dump()

        elif isinstance(result, ParticleGibbsResult):
            iterations = []
            num_iterations = len(result.responses_lst)
            for it_idx in range(num_iterations):
                # Build a ParticleFilteringResult-like object for each iteration
                it_result = ParticleFilteringResult(
                    responses=result.responses_lst[it_idx],
                    log_weights_lst=result.log_weights_lst[it_idx],
                    selected_index=result.selected_index if it_idx == num_iterations - 1 else 0,
                    steps_used_lst=result.steps_used_lst[it_idx],
                )
                iterations.append(_build_pf_trace(it_result))
            trace = ParticleGibbsTrace(
                num_iterations=num_iterations,
                iterations=iterations,
            )
            return trace.model_dump()

        elif isinstance(result, ParticleFilteringResult):
            trace = _build_pf_trace(result)
            return trace.model_dump()

        else:
            logger.warning(f"Unknown result type for trace: {type(result)}")
            return None

    except Exception as e:
        logger.error(f"Failed to build trace for {algorithm}: {e}", exc_info=True)
        return None


async def run_its(
    lm: OpenAICompatibleLanguageModel,
    question: str,
    algorithm: str,
    budget: int,
    api_key: str,
    baseline_input_tokens: int = 0,
    baseline_output_tokens: int = 0,
    enable_tools: bool = False,
    tool_vote: str | None = None,
    exclude_args: list[str] | None = None,
    question_type: str = "general"
) -> tuple[str, int, int, int, dict | None, list[ToolCall] | None]:
    """
    Run ITS inference with the specified algorithm.

    Args:
        lm: Language model to use
        question: Question to answer
        algorithm: ITS algorithm name
        budget: Computational budget
        api_key: API key for judge/PRM
        baseline_input_tokens: Baseline input token count for estimation
        baseline_output_tokens: Baseline output token count for estimation
        enable_tools: Whether tools are enabled
        tool_vote: Tool voting strategy
        exclude_args: Arguments to exclude from tool voting
        question_type: Question type ("math", "tool_calling", or "general")

    Returns:
        (answer, latency_ms, input_tokens, output_tokens, trace, tool_calls)
    """
    start_time = time.time()

    # Prepare tools if enabled
    tools = get_tool_schemas() if enable_tools else None
    tool_choice = "auto" if enable_tools else None

    # Create algorithm instance
    if algorithm == "best_of_n":
        # Use LLM judge for Best-of-N
        judge = LLMJudgeRewardModel(
            model=DEFAULT_JUDGE_MODEL,
            criterion="overall_quality",
            judge_type="pointwise",
            api_key=api_key,
            enable_judge_logging=False,
        )
        alg = BestOfN(judge)

    elif algorithm == "self_consistency":
        # Configure Self-Consistency based on question type
        if question_type == "tool_calling":
            # Tool consensus: vote on tool selection
            alg = SelfConsistency(
                consistency_space_projection_func=None,
                tool_vote=tool_vote or "tool_name",
                exclude_args=exclude_args or []
            )
        elif question_type == "math":
            # Math: extract boxed answers for voting
            projection_func = create_math_projection_function()
            alg = SelfConsistency(
                consistency_space_projection_func=projection_func,
                tool_vote=None,
                exclude_args=[]
            )
        else:
            # General: exact text matching (current behavior)
            alg = SelfConsistency(
                consistency_space_projection_func=None,
                tool_vote=None,
                exclude_args=[]
            )

    elif algorithm in ["beam_search", "particle_filtering", "entropic_particle_filtering", "particle_gibbs"]:
        # Process-based algorithms need StepGeneration and Process Reward Model

        # Create StepGeneration with step-by-step reasoning
        # Using "\n\n" as step delimiter for reasoning steps
        step_gen = StepGeneration(
            max_steps=DEFAULT_STEP_GEN_MAX_STEPS,
            step_token=DEFAULT_STEP_GEN_TOKEN,
            stop_token=None,
            temperature=DEFAULT_STEP_GEN_TEMPERATURE,
            include_stop_str_in_output=False,
        )

        prm = LLMProcessRewardModel(
            model=DEFAULT_PRM_MODEL,
            api_key=api_key,
            temperature=DEFAULT_PRM_TEMPERATURE,
        )

        if algorithm == "beam_search":
            beam_width = max(BEAM_WIDTH_MIN, min(BEAM_WIDTH_MAX, budget // 2))
            adjusted_budget = max(beam_width, (budget // beam_width) * beam_width)
            alg = BeamSearch(sg=step_gen, prm=prm, beam_width=beam_width)
            budget = adjusted_budget

        elif algorithm == "particle_filtering":
            # Basic particle filtering
            alg = ParticleFiltering(
                sg=step_gen,
                prm=prm,
            )

        elif algorithm == "entropic_particle_filtering":
            # Entropic particle filtering with temperature annealing
            alg = EntropicParticleFiltering(
                sg=step_gen,
                prm=prm,
            )

        elif algorithm == "particle_gibbs":
            # Particle Gibbs with multiple iterations
            # Use smaller budget for iterations to avoid timeout
            num_iterations = max(GIBBS_ITERATIONS_MIN, min(GIBBS_ITERATIONS_MAX, budget // GIBBS_ITERATIONS_DIVISOR))
            alg = ParticleGibbs(
                sg=step_gen,
                prm=prm,
                num_iterations=num_iterations,
            )
    else:
        raise ValueError(f"Unknown algorithm: {algorithm}")

    # Run inference with full result to capture trace data
    result = await alg.ainfer(
        lm,
        question,
        budget=budget,
        return_response_only=False,
        tools=tools,
        tool_choice=tool_choice
    )
    answer = extract_content_from_lm_response(result.the_one)

    # Extract and execute tool calls if present
    tool_calls_list = None
    if enable_tools and "tool_calls" in result.the_one and result.the_one["tool_calls"]:
        tool_calls_list = []
        for tc in result.the_one["tool_calls"]:
            tool_name = tc.get("function", {}).get("name", "")
            tool_args = parse_tool_args(tc.get("function", {}).get("arguments", {}))
            tool_result = execute_tool(tool_name, tool_args)

            tool_calls_list.append(ToolCall(
                name=tool_name,
                arguments=tool_args,
                result=tool_result
            ))

            if tool_result:
                answer += f"\n\n**Tool Used:** {tool_name}\n**Result:** {tool_result}"

    latency_ms = int((time.time() - start_time) * 1000)

    # Build trace from the full result
    trace = build_trace(algorithm, result, tool_vote=tool_vote)

    # --- Estimated token usage for ITS ---
    #
    # WHY ESTIMATED: ITS algorithms call the LLM multiple times internally
    # (via ainfer()), but the algorithm classes do not accumulate or return
    # per-call token usage from litellm. Until the algorithm internals are
    # updated to surface aggregated usage data, we estimate based on:
    #
    #   1. The baseline's actual token counts (from a single litellm call)
    #   2. The number of candidates the algorithm produced
    #   3. Algorithm-specific overhead (judge/PRM scoring calls)
    #
    # These estimates are marked with tokens_estimated=True in the response
    # so the frontend can label them accordingly.
    #
    # WHAT IS ACTUAL vs ESTIMATED for ITS results:
    #   - latency_ms:    ACTUAL (wall-clock around the full ainfer() call)
    #   - input_tokens:  ESTIMATED (see formula below)
    #   - output_tokens: ESTIMATED (see formula below)
    #   - cost_usd:      ESTIMATED (derived from estimated tokens x pricing)
    #
    num_candidates = len(result.responses) if hasattr(result, 'responses') else budget

    if baseline_input_tokens > 0 and baseline_output_tokens > 0:
        # Base: assume each candidate uses roughly the same tokens as baseline
        estimated_input = baseline_input_tokens * num_candidates
        estimated_output = baseline_output_tokens * num_candidates

        if algorithm == "best_of_n":
            # Best-of-N also makes one LLM judge call per candidate to score
            # it. The judge sees the candidate response (~baseline_output_tokens)
            # plus a scoring prompt (~200 tokens), and produces a short score
            # output (~50 tokens).
            judge_input_per_call = baseline_output_tokens + 200
            judge_output_per_call = 50
            estimated_input += judge_input_per_call * num_candidates
            estimated_output += judge_output_per_call * num_candidates
        elif algorithm in ["beam_search", "particle_filtering", "entropic_particle_filtering", "particle_gibbs"]:
            # Process-based algorithms make PRM scoring calls at each
            # reasoning step. We use the actual step count from the result
            # when available, otherwise fall back to budget * 4 as a rough
            # average. Each PRM call uses ~150 input and ~30 output tokens.
            if hasattr(result, 'steps_used_lst'):
                prm_calls = sum(result.steps_used_lst)
            else:
                prm_calls = budget * 4
            estimated_input += 150 * prm_calls
            estimated_output += 30 * prm_calls
    else:
        estimated_input = 0
        estimated_output = 0

    return answer, latency_ms, estimated_input, estimated_output, trace, tool_calls_list


@app.post("/compare", response_model=CompareResponse)
async def compare(request: CompareRequest):
    """
    Compare baseline vs ITS inference.

    Input:
        - question: The question to answer
        - model_id: Model identifier from the registry
        - algorithm: ITS algorithm (best_of_n or self_consistency)
        - budget: Computation budget

    Output:
        - baseline: { answer, latency_ms, log_preview }
        - its: { answer, latency_ms, log_preview }
        - meta: { model_id, algorithm, budget, run_id }
    """
    run_id = str(uuid.uuid4())

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
                question_type=question_type
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
                question_type=question_type
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
                question_type=question_type
            )

        logger.info(
            f"[{run_id}] Comparison complete: "
            f"baseline_latency={baseline_latency}ms, its_latency={its_latency}ms"
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
                log_preview="",  # Placeholder for future
                model_size=frontier_model_size if request.use_case == "match_frontier" else model_size,
                cost_usd=baseline_cost if baseline_cost > 0 else None,
                input_tokens=baseline_input_tokens if baseline_input_tokens > 0 else None,
                output_tokens=baseline_output_tokens if baseline_output_tokens > 0 else None,
                tool_calls=baseline_tool_calls if baseline_tool_calls else None,
            ),
            "its": ResultDetail(
                answer=its_answer,
                latency_ms=its_latency,
                log_preview="",  # Placeholder for future
                model_size=model_size,
                cost_usd=its_cost if its_cost > 0 else None,
                input_tokens=its_input_tokens if its_input_tokens > 0 else None,
                output_tokens=its_output_tokens if its_output_tokens > 0 else None,
                tokens_estimated=True,
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
            }
        }

        # Add small baseline if match_frontier use case
        if request.use_case == "match_frontier" and small_baseline_answer is not None:
            response_data["small_baseline"] = ResultDetail(
                answer=small_baseline_answer,
                latency_ms=small_baseline_latency,
                log_preview="",
                model_size=model_size,
                cost_usd=small_baseline_cost if small_baseline_cost and small_baseline_cost > 0 else None,
                input_tokens=small_baseline_input_tokens if small_baseline_input_tokens and small_baseline_input_tokens > 0 else None,
                output_tokens=small_baseline_output_tokens if small_baseline_output_tokens and small_baseline_output_tokens > 0 else None,
                tool_calls=small_baseline_tool_calls if small_baseline_tool_calls else None,
            )
            response_data["meta"]["frontier_model_id"] = request.frontier_model_id

        response = CompareResponse(**response_data)

        return response

    except ValueError as e:
        logger.error(f"[{run_id}] Validation error: {e}")
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"[{run_id}] Error during comparison: {e}", exc_info=True)

        # Return generic error messages — details are logged server-side above
        error_msg = str(e)
        if "timeout" in error_msg.lower() or "payload" in error_msg.lower() or "ClientPayloadError" in error_msg:
            raise HTTPException(
                status_code=504,
                detail="The model is temporarily unavailable. Please try again or select a different model."
            )

        raise HTTPException(status_code=500, detail="Internal server error. Check server logs for details.")


# Mount frontend static files at root so relative paths in index.html resolve
# correctly. This must come after all API route definitions because FastAPI
# mounts are greedy — a "/" mount placed before routes would shadow them.
app.mount("/", StaticFiles(directory=str(frontend_dir)), name="frontend")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)

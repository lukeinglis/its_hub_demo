"""
Inference logic for the ITS demo.

Contains language model creation, baseline inference, ITS inference,
cost calculation, and question type detection.
"""

import asyncio
import json
import logging
import os
import re
import time
from typing import Dict

import litellm

from its_hub.lms import OpenAICompatibleLanguageModel, LiteLLMLanguageModel, StepGeneration
from its_hub.algorithms import (
    BestOfN,
    SelfConsistency,
    BeamSearch,
    ParticleFiltering,
    EntropicParticleFiltering,
    ParticleGibbs,
)
try:
    from its_hub.integration.reward_hub import LLMJudgeRewardModel
except ImportError:
    LLMJudgeRewardModel = None
from its_hub.types import ChatMessage
from its_hub.utils import extract_content_from_lm_response, QWEN_SYSTEM_PROMPT
from its_hub.base import AbstractLanguageModel
from its_hub.algorithms.self_consistency import create_regex_projection_function

from .config import get_model_config, get_api_key, ModelConfig
from .llm_prm import LLMProcessRewardModel
from .models import ToolCall
from .tools import get_tool_schemas, execute_tool
from .traces import build_trace

# Vertex AI models are imported lazily in create_language_model() to avoid
# requiring anthropic and google-cloud-aiplatform for basic usage (e.g.
# guided demo on a MacBook with only OpenAI configured).

logger = logging.getLogger(__name__)

# ── Algorithm / model defaults ────────────────────────────────────────
DEFAULT_JUDGE_MODEL = "gpt-4.1-mini"
DEFAULT_PRM_MODEL = "gpt-4.1-mini"
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

    # If tools are explicitly enabled, it's a tool-calling question
    if enable_tools:
        return "tool_calling"

    # Check for math patterns using weighted scoring to reduce false positives.
    # Strong indicators (LaTeX notation) are sufficient on their own.
    # Weak indicators require at least 2 matches to trigger math detection.
    strong_indicators = [
        r'\\frac', r'\\boxed', r'\\sum', r'\\int', r'\\sqrt',
        r'\\begin\{', r'\\mathbb', r'\\left', r'\\right',
        r'\$[^$]+\$',  # Paired dollar signs (LaTeX inline math)
    ]
    weak_indicators = [
        r'probability', r'find the value', r'what is the.*term',
        r'solve for [a-z]', r'evaluate the (integral|derivative|limit)',
        r'\bx\s*[\+\-\*\/\^]\s*\d', r'\d\s*[\+\-\*\/\^]\s*x',  # algebraic expressions
    ]

    for pattern in strong_indicators:
        if re.search(pattern, question):
            return "math"

    weak_count = sum(1 for p in weak_indicators if re.search(p, question, re.IGNORECASE))
    if weak_count >= 2:
        return "math"

    return "general"


def create_math_projection_function():
    """Create projection function for extracting boxed mathematical answers."""
    return create_regex_projection_function(r'\\boxed\{([^}]+)\}')


def extract_final_answer(text: str) -> str | None:
    """
    Extract the final answer from a model response, ignoring reasoning.

    Mirrors the frontend extractFinalAnswer() logic so that self-consistency
    voting groups responses by their conclusion, not their full text.

    Returns the extracted answer string, or None if no answer pattern matched.
    """
    if not text or not text.strip():
        return None

    # 1. Boxed answer (math) — handle nested braces
    boxed_idx = text.find('\\boxed{')
    if boxed_idx != -1:
        brace_count = 0
        start = boxed_idx + 7
        for i in range(start, len(text)):
            if text[i] == '{':
                brace_count += 1
            elif text[i] == '}':
                if brace_count == 0:
                    if i > start:
                        return text[start:i]
                    break
                brace_count -= 1

    # 2. Explicit answer patterns
    answer_patterns = [
        r'Final Answer:\s*(.+?)(?:\n\n|$)',
        r'Answer:\s*(.+?)(?:\n\n|$)',
        r'Therefore,?\s+the\s+(?:answer|value|result)\s+(?:is|equals?)\s+(.+?)(?:\.|$)',
        r'(?:^|\n)Therefore,?\s+(.+?)(?:\n\n|$)',
        r'(?:^|\n)Thus,?\s+(.+?)(?:\n\n|$)',
        r'(?:^|\n)So,?\s+the\s+(?:answer|value|result)\s+(?:is|equals?)\s+(.+?)(?:\.|$)',
        r'(?:^|\n)In conclusion,?\s+(.+?)(?:\n\n|$)',
    ]
    for pattern in answer_patterns:
        match = re.search(pattern, text, re.IGNORECASE | re.DOTALL)
        if match:
            answer = match.group(1).strip()
            if len(answer) < 200:
                return answer

    # 3. Short last line that looks like an answer
    lines = text.strip().split('\n')
    last_line = lines[-1].strip()
    if last_line and len(last_line) < 150 and re.search(
        r'^[A-Z]:|^[-+]?\d+|\$.*\$|^x\s*=|^y\s*=|=.*\d', last_line
    ):
        return last_line

    return None


def create_general_projection_function():
    """
    Create a projection function for general (non-math, non-tool) questions.

    Extracts the final answer/conclusion from each response so that
    self-consistency votes on answers, not full reasoning text.
    Falls back to the last paragraph if no explicit answer pattern is found.
    """
    def project(response: str) -> str:
        extracted = extract_final_answer(response)
        if extracted:
            return extracted.strip().lower()

        # Fallback: use the last non-empty paragraph as a rough "conclusion"
        paragraphs = [p.strip() for p in response.strip().split('\n\n') if p.strip()]
        if paragraphs:
            return paragraphs[-1].strip().lower()

        return response.strip().lower()

    return project


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
        # Lazy import to avoid requiring anthropic/google-cloud-aiplatform for basic usage
        try:
            from .vertex_lm import VertexAIClaudeModel, VertexAIGeminiModel
        except ImportError:
            raise ValueError(
                "Vertex AI models require additional packages. "
                "Install with: pip install anthropic[vertex] google-cloud-aiplatform"
            )

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
            # _prepare_request_data is designed for aiohttp (auth via headers),
            # but litellm needs api_key and api_base as explicit kwargs.
            # Without these, litellm can't authenticate with non-OpenAI
            # providers (e.g. OpenRouter) and the call fails silently.
            request_data["api_key"] = lm.api_key
            request_data["api_base"] = lm.endpoint
            # litellm needs a provider prefix to route non-OpenAI models correctly.
            # Without it, models like "meta-llama/llama-3.2-3b-instruct" fail with
            # "LLM Provider NOT provided". Detect OpenRouter by base_url.
            if "openrouter.ai" in lm.endpoint and not request_data.get("model", "").startswith("openrouter/"):
                request_data["model"] = "openrouter/" + request_data["model"]
            elif ("localhost:11434" in lm.endpoint or "127.0.0.1:11434" in lm.endpoint) and not request_data.get("model", "").startswith("openai/"):
                # Use openai/ prefix for Ollama — forces litellm to use the
                # OpenAI-compatible /v1 endpoint rather than native Ollama API
                request_data["model"] = "openai/" + request_data["model"]
            full_response = await asyncio.wait_for(
                litellm.acompletion(**request_data),
                timeout=120.0,
            )

            # Extract usage from full response
            if hasattr(full_response, 'usage') and full_response.usage:
                input_tokens = getattr(full_response.usage, 'prompt_tokens', 0)
                output_tokens = getattr(full_response.usage, 'completion_tokens', 0)

            # Extract message
            response = full_response.choices[0].message.dict()
        except Exception as e:
            logger.warning(f"Could not capture token usage: {type(e).__name__}")
            response = await lm.agenerate(messages)
    else:
        # For Vertex AI or other models, use standard interface
        response = await lm.agenerate(messages)
        # Extract usage if the model wrapper provided it (e.g. Vertex AI Claude)
        if isinstance(response, dict) and "usage" in response:
            input_tokens = response["usage"].get("input_tokens", 0)
            output_tokens = response["usage"].get("output_tokens", 0)

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
    question_type: str = "general",
    judge_criterion: str = "overall_quality",
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
        judge_criterion: Judge criterion for Best-of-N (built-in name or custom prompt)

    Returns:
        (answer, latency_ms, input_tokens, output_tokens, trace, tool_calls)
    """
    start_time = time.time()

    # Prepare tools if enabled
    tools = get_tool_schemas() if enable_tools else None
    tool_choice = "auto" if enable_tools else None

    # Create algorithm instance
    if algorithm == "best_of_n":
        if LLMJudgeRewardModel is None:
            raise ValueError(
                "Best-of-N algorithm requires the reward_hub library. "
                "Install with: pip install 'its_hub[prm]'"
            )

        # Resolve judge criterion: built-in or custom
        built_in_criteria = {"overall_quality", "multi_step_tool_judge"}
        if judge_criterion in built_in_criteria:
            criterion_to_use = judge_criterion
        else:
            # Custom criterion — register with CriterionRegistry
            try:
                from reward_hub.llm_judge.prompts import Criterion, CriterionRegistry
            except ImportError:
                raise ValueError(
                    "Custom judge criteria require the reward_hub library. "
                    "Install with: pip install 'its_hub[prm]'"
                )
            criterion_name = f"custom_{hash(judge_criterion) & 0xFFFFFFFF:08x}"
            logger.info(f"Registering custom judge criterion as: {criterion_name}")
            custom_criterion = Criterion(
                name=criterion_name,
                content=judge_criterion,
                description="Custom evaluation criterion",
            )
            CriterionRegistry.register(custom_criterion)
            criterion_to_use = criterion_name

        # Use LLM judge for Best-of-N
        judge = LLMJudgeRewardModel(
            model=DEFAULT_JUDGE_MODEL,
            criterion=criterion_to_use,
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
            # General: extract final answer/conclusion for voting
            projection_func = create_general_projection_function()
            alg = SelfConsistency(
                consistency_space_projection_func=projection_func,
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
            alg = ParticleFiltering(
                sg=step_gen,
                prm=prm,
            )

        elif algorithm == "entropic_particle_filtering":
            alg = EntropicParticleFiltering(
                sg=step_gen,
                prm=prm,
            )

        elif algorithm == "particle_gibbs":
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
    num_candidates = len(result.responses) if hasattr(result, 'responses') else budget

    if baseline_input_tokens > 0 and baseline_output_tokens > 0:
        estimated_input = baseline_input_tokens * num_candidates
        estimated_output = baseline_output_tokens * num_candidates

        if algorithm == "best_of_n":
            judge_input_per_call = baseline_output_tokens + 200
            judge_output_per_call = 50
            estimated_input += judge_input_per_call * num_candidates
            estimated_output += judge_output_per_call * num_candidates
        elif algorithm in ["beam_search", "particle_filtering", "entropic_particle_filtering", "particle_gibbs"]:
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

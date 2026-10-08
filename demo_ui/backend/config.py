"""
Configuration for the demo backend.

Model registry maps model_id to connection details.
"""

import os
from typing import Dict, TypedDict


class ModelConfig(TypedDict, total=False):
    """Configuration for a single model."""
    base_url: str
    api_key_env_var: str
    model_name: str
    description: str
    provider: str  # Optional: 'maas', 'openrouter', 'openai'. Defaults to 'maas'
    size: str  # Optional: Model size (e.g., "70B", "7B")
    input_cost_per_1m: float  # Optional: Cost per 1M input tokens in USD
    output_cost_per_1m: float  # Optional: Cost per 1M output tokens in USD
    supports_tools: bool  # Optional: Whether model supports function/tool calling (defaults to True for OpenAI)
    is_reasoning: bool  # Optional: Whether model is a reasoning/thinking model (defaults to False)
    # Infrastructure metadata for savings story
    self_hostable: bool  # Optional: Whether model can be self-hosted (defaults to False for API models)
    min_gpu: str  # Optional: Minimum GPU for self-hosting (e.g., "1x A10 24GB")
    gpu_cloud_cost_hr: float  # Optional: Approximate cloud GPU cost per hour in USD


# Model registry
# Maps model_id -> { base_url, api_key_env_var, model_name, ... }
#
# Providers mirror Red Hat's model delivery paths (not API vendors):
#   maas        — models served by your infrastructure: OpenShift AI MaaS route,
#                 self-hosted vLLM, or Red Hat AI Inference Server
#   openrouter  — no-GPU fallback to the same Red Hat-validated open models
#   openai      — frontier models for "Match Frontier" comparisons
#
# Tier legend:
#   🏢 MaaS / Validated — models IT serves through MaaS (headline)
#   ⚡ Small/Fast        — cost-effective, good ITS candidates
#   🏆 Frontier          — ceiling for "Match Frontier" use case
#
# NOTE: OpenRouter models are available when the user has OPENROUTER_API_KEY set.
# OpenRouter model availability changes over time. If a model fails with
# "No endpoints found", check https://openrouter.ai/models for current list.
# Pricing last verified: October 2026.

MODEL_REGISTRY: Dict[str, ModelConfig] = {

    # ========================================================================
    # RED HAT MaaS / VALIDATED MODELS (served by your infrastructure)
    # ========================================================================
    # Point these at an OpenShift AI MaaS route, a self-hosted vLLM server, or
    # Red Hat AI Inference Server via MAAS_BASE_URL (falls back to VLLM_BASE_URL).
    # The same validated open models are available without a GPU via OpenRouter.

    # === Self-hosted MaaS endpoint ===
    "maas-granite-4-small": {
        "base_url": os.getenv("MAAS_BASE_URL", os.getenv("VLLM_BASE_URL", "http://localhost:8100/v1")),
        "api_key_env_var": "MAAS_API_KEY",
        "model_name": os.getenv("MAAS_MODEL_NAME", "ibm-granite/granite-4-h-small"),
        "description": "🏢 IBM Granite 4 Small (Red Hat MaaS / self-hosted)",
        "provider": "maas",
        "size": "Small",
        "input_cost_per_1m": 0.0,  # zero per-token cost when self-hosted
        "output_cost_per_1m": 0.0,
        "self_hostable": True,
    },
    "maas-custom": {
        "base_url": os.getenv("MAAS_BASE_URL", os.getenv("VLLM_BASE_URL", "http://localhost:8100/v1")),
        "api_key_env_var": "MAAS_API_KEY",
        "model_name": os.getenv("MAAS_CUSTOM_MODEL_NAME", "your-model-name"),
        "description": "🔧 Your MaaS model (any validated open model)",
        "provider": "maas",
        "size": os.getenv("MAAS_MODEL_SIZE", "Custom"),
    },

    # === Ollama-backed quick-start variants (handy for laptop demos) ===
    "qwen3-0.6b": {
        "base_url": os.getenv("OLLAMA_BASE_URL", "http://localhost:11434/v1"),
        "api_key_env_var": "OLLAMA_API_KEY",
        "model_name": "qwen3:0.6b",
        "description": "🎯 Qwen3 0.6B (Tiny — ideal for ITS demos)",
        "provider": "maas",
        "size": "0.6B",
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "self_hostable": True,
    },
    "granite-4-3b": {
        "base_url": os.getenv("OLLAMA_BASE_URL", "http://localhost:11434/v1"),
        "api_key_env_var": "OLLAMA_API_KEY",
        "model_name": "granite4:3b",
        "description": "🏢🎯 IBM Granite 4 3B (Self-hosted via Ollama)",
        "provider": "maas",
        "size": "Small",
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "self_hostable": True,
    },

    # ========================================================================
    # OPENROUTER — no-GPU fallback to Red Hat-validated open models
    # ========================================================================
    # Setup: Get API key from https://openrouter.ai/keys
    # Set OPENROUTER_API_KEY in your .env file

    # === MaaS / Validated Open Models 🏢 ===
    "granite-4.0-micro": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "ibm-granite/granite-4.0-h-micro",
        "description": "🏢🎯 IBM Granite 4.0 Micro (Red Hat validated)",
        "provider": "openrouter",
        "size": "3B",
        "input_cost_per_1m": 0.017,
        "output_cost_per_1m": 0.11,
        "self_hostable": True,
        "min_gpu": "1x RTX 4090 / A10 (24GB)",
        "gpu_cloud_cost_hr": 0.50,
    },
    "gpt-oss-20b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "openai/gpt-oss-20b",
        "description": "🏢 GPT-OSS 20B MoE (Apache 2.0, open weights)",
        "provider": "openrouter",
        "size": "20B MoE",
        "input_cost_per_1m": 0.018,
        "output_cost_per_1m": 0.09,
        "self_hostable": True,
        "min_gpu": "1x RTX 4090 (16GB VRAM, MXFP4)",
        "gpu_cloud_cost_hr": 0.50,
    },
    "gpt-oss-120b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "openai/gpt-oss-120b",
        "description": "🏢🏆 GPT-OSS 120B MoE (Apache 2.0, open weights)",
        "provider": "openrouter",
        "size": "120B MoE",
        "input_cost_per_1m": 0.03,
        "output_cost_per_1m": 0.17,
        "self_hostable": True,
        "min_gpu": "1x H100 (80GB, MXFP4)",
        "gpu_cloud_cost_hr": 2.50,
    },
    "llama-4-scout": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "meta-llama/llama-4-scout",
        "description": "🏢⚖️ Llama 4 Scout (Red Hat validated)",
        "provider": "openrouter",
        "size": "17B active / 109B MoE",
        "input_cost_per_1m": 0.08,
        "output_cost_per_1m": 0.30,
        "self_hostable": True,
        "min_gpu": "1x H100 80GB",
        "gpu_cloud_cost_hr": 2.50,
    },
    "qwen3-1.7b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "qwen/qwen3-1.7b",
        "description": "🎯 Qwen3 1.7B (Very small)",
        "provider": "openrouter",
        "size": "1.7B",
        "input_cost_per_1m": 0.05,
        "output_cost_per_1m": 0.05,
    },

    # === Frontier open models 🏆 ===
    "llama-4-maverick": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "meta-llama/llama-4-maverick",
        "description": "🏢🏆 Llama 4 Maverick (Frontier open model)",
        "provider": "openrouter",
        "size": "17B active / 400B MoE",
        "input_cost_per_1m": 0.15,
        "output_cost_per_1m": 0.60,
    },

    # === Claude via OpenRouter (replaces the removed Vertex AI path) ===
    "claude-sonnet-4.6": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "anthropic/claude-sonnet-4.6",
        "description": "🏆 Claude Sonnet 4.6 (Frontier)",
        "provider": "openrouter",
        "size": "Large",
        "input_cost_per_1m": 3.00,
        "output_cost_per_1m": 15.00,
        "supports_tools": True,
    },
    "claude-haiku-4.5": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "anthropic/claude-haiku-4.5",
        "description": "⚡ Claude Haiku 4.5 (Small, fast)",
        "provider": "openrouter",
        "size": "Small",
        "input_cost_per_1m": 1.00,
        "output_cost_per_1m": 5.00,
        "supports_tools": True,
    },

    # ========================================================================
    # OPENAI — frontier models for "Match Frontier" comparisons
    # ========================================================================

    # === Small/Fast Models ⚡ ===
    "gpt-6-luna": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-6-luna",
        "description": "⚡ GPT-6 Luna (Small, cost-efficient)",
        "provider": "openai",
        "size": "Small",
        "input_cost_per_1m": 0.10,
        "output_cost_per_1m": 0.50,
        "supports_tools": True,
        "self_hostable": False,
    },
    "gpt-5-mini": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-5-mini",
        "description": "⚡ GPT-5 Mini (Previous-gen small)",
        "provider": "openai",
        "size": "Small",
        "input_cost_per_1m": 0.25,
        "output_cost_per_1m": 2.00,
        "supports_tools": True,
        "self_hostable": False,
    },

    # === Frontier Models 🏆 ===
    "gpt-6-sol": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-6-sol",
        "description": "🏆 GPT-6 Sol (Frontier)",
        "provider": "openai",
        "size": "Large",
        "input_cost_per_1m": 2.00,
        "output_cost_per_1m": 10.00,
        "supports_tools": True,
        "self_hostable": False,
    },
    "gpt-5.5": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-5.5",
        "description": "🏆 GPT-5.5 (Previous-gen frontier)",
        "provider": "openai",
        "size": "Large",
        "input_cost_per_1m": 5.00,
        "output_cost_per_1m": 30.00,
        "supports_tools": True,
        "self_hostable": False,
    },
}


def get_model_config(model_id: str) -> ModelConfig:
    """Get configuration for a model by ID."""
    if model_id not in MODEL_REGISTRY:
        raise ValueError(f"Model '{model_id}' not found in registry. Available models: {list(MODEL_REGISTRY.keys())}")
    return MODEL_REGISTRY[model_id]


def get_api_key(model_id: str) -> str:
    """Get API key for a model from environment variables."""
    config = get_model_config(model_id)
    api_key = os.getenv(config["api_key_env_var"])
    if not api_key:
        # Self-hosted models (Ollama, local vLLM) don't require API keys
        base_url = config.get("base_url", "")
        if "localhost" in base_url or "127.0.0.1" in base_url:
            return "ollama"
        # MaaS routes (e.g. OpenShift AI) often use unauthenticated routes or
        # network-level auth — a placeholder key keeps OpenAI-compatible clients happy
        if config["api_key_env_var"] == "MAAS_API_KEY":
            return "maas"
        raise ValueError(
            f"API key not found for model '{model_id}'. "
            f"Please set environment variable '{config['api_key_env_var']}'"
        )
    return api_key


def get_judge_config() -> dict:
    """Resolve judge/PRM connection settings.

    Defaults to the MaaS endpoint when configured, so the whole demo —
    including the correctness judge, Best-of-N judge, and the process reward
    model — runs on your own infrastructure with no OpenAI key. Falls back to
    the OpenAI endpoint. JUDGE_BASE_URL / JUDGE_MODEL / JUDGE_API_KEY
    override everything.
    """
    base_url = (
        os.getenv("JUDGE_BASE_URL")
        or os.getenv("MAAS_BASE_URL")
        or os.getenv("VLLM_BASE_URL")
        or "https://api.openai.com/v1"
    )
    custom = base_url != "https://api.openai.com/v1"

    if custom:
        model = os.getenv("JUDGE_MODEL") or os.getenv("MAAS_MODEL_NAME") or "gpt-5-mini"
        api_key = (
            os.getenv("JUDGE_API_KEY")
            or os.getenv("OPENAI_API_KEY")
            or os.getenv("MAAS_API_KEY")
            or "maas"  # placeholder for unauthenticated MaaS routes
        )
        # litellm needs the openai/ prefix to route to a custom OpenAI-compatible endpoint
        litellm_model = model if model.startswith(("openai/", "openrouter/")) else f"openai/{model}"
    else:
        model = os.getenv("JUDGE_MODEL", "gpt-5-mini")
        api_key = os.getenv("JUDGE_API_KEY") or os.getenv("OPENAI_API_KEY")
        litellm_model = model

    return {
        "base_url": base_url,
        "api_key": api_key,
        "model": model,                    # for OpenAICompatibleLanguageModel
        "litellm_model": litellm_model,    # for litellm.acompletion calls
    }

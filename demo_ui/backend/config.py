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
    provider: str  # Optional: 'openai', 'vertex_ai', etc. Defaults to 'openai'
    vertex_project: str  # Optional: for Vertex AI
    vertex_location: str  # Optional: for Vertex AI
    size: str  # Optional: Model size (e.g., "175B", "70B", "7B")
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
# Tier legend:
#   🎯 Weak/Very Small  — best for demonstrating ITS gains
#   ⚡ Small/Fast        — cost-effective, good ITS candidates
#   ⚖️ Medium            — comparison baselines
#   🏆 Frontier          — ceiling for "Match Frontier" use case
#   🧠 Reasoning         — chain-of-thought / thinking models (is_reasoning=True)
#   🏢 IBM Granite       — enterprise open-source
#
# NOTE: OpenRouter models are available when the user has OPENROUTER_API_KEY set.
# OpenRouter model availability changes over time. If a model fails with
# "No endpoints found", check https://openrouter.ai/models for current list.
# Pricing last verified: March 2026.

MODEL_REGISTRY: Dict[str, ModelConfig] = {

    # ========================================================================
    # OPENAI MODELS (Native OpenAI API)
    # ========================================================================

    # === Frontier Models 🏆 ===
    "gpt-4o": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-4o",
        "description": "🏆 GPT-4o (Frontier)",
        "provider": "openai",
        "size": "Large",
        "input_cost_per_1m": 2.50,
        "output_cost_per_1m": 10.00,
        "supports_tools": True,
        "self_hostable": False,
    },
    "gpt-4.1": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-4.1",
        "description": "🏆 GPT-4.1 (Frontier)",
        "provider": "openai",
        "size": "Large",
        "input_cost_per_1m": 2.00,
        "output_cost_per_1m": 8.00,
        "supports_tools": True,
    },

    # === Small/Fast Models ⚡ ===
    "gpt-4o-mini": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-4o-mini",
        "description": "⚡ GPT-4o Mini (Small, fast)",
        "provider": "openai",
        "size": "Small",
        "input_cost_per_1m": 0.15,
        "output_cost_per_1m": 0.60,
        "supports_tools": True,
        "self_hostable": False,
    },
    "gpt-4.1-mini": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-4.1-mini",
        "description": "⚡ GPT-4.1 Mini (Small, fast)",
        "provider": "openai",
        "size": "Small",
        "input_cost_per_1m": 0.40,
        "output_cost_per_1m": 1.60,
        "supports_tools": True,
    },
    "gpt-3.5-turbo": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-3.5-turbo",
        "description": "⚡ GPT-3.5 Turbo (Small)",
        "provider": "openai",
        "size": "Small",
        "input_cost_per_1m": 0.50,
        "output_cost_per_1m": 1.50,
        "supports_tools": True,
    },

    # === Weak Models (for ITS demonstration) 🎯 ===
    "gpt-4.1-nano": {
        "base_url": "https://api.openai.com/v1",
        "api_key_env_var": "OPENAI_API_KEY",
        "model_name": "gpt-4.1-nano",
        "description": "🎯 GPT-4.1 Nano (Very small - great for ITS demo)",
        "provider": "openai",
        "size": "Small",
        "input_cost_per_1m": 0.10,
        "output_cost_per_1m": 0.40,
        "supports_tools": True,
    },

    # ========================================================================
    # OPENROUTER MODELS (Access to multiple providers)
    # ========================================================================
    # Setup: Get API key from https://openrouter.ai/keys
    # Set OPENROUTER_API_KEY in your .env file

    # === Weak Models (Great for demonstrating ITS) 🎯 ===
    "qwen3-1.7b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "qwen/qwen3-1.7b",
        "description": "🎯 Qwen3 1.7B (Very weak - dramatic ITS demo)",
        "provider": "openai",
        "size": "1.7B",
        "input_cost_per_1m": 0.05,
        "output_cost_per_1m": 0.05,
    },
    "llama-3.2-3b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "meta-llama/llama-3.2-3b-instruct",
        "description": "🎯 Llama 3.2 3B (Very weak - dramatic ITS demo)",
        "provider": "openai",
        "size": "3B",
        "input_cost_per_1m": 0.06,
        "output_cost_per_1m": 0.06,
        "self_hostable": True,
        "min_gpu": "1x RTX 4090 / A10 (24GB)",
        "gpu_cloud_cost_hr": 0.50,
    },
    "granite-4.0-micro": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "ibm-granite/granite-4.0-h-micro",
        "description": "🏢🎯 IBM Granite 4.0 Micro 3B (Very weak - excellent for ITS demo)",
        "provider": "openai",
        "size": "3B",
        "input_cost_per_1m": 0.017,
        "output_cost_per_1m": 0.11,
        "self_hostable": True,
        "min_gpu": "1x RTX 4090 / A10 (24GB)",
        "gpu_cloud_cost_hr": 0.50,
    },
    "gemma-3-4b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "google/gemma-3-4b-it",
        "description": "🎯 Gemma 3 4B (Weak - great for ITS demo)",
        "provider": "openai",
        "size": "4B",
        "input_cost_per_1m": 0.04,
        "output_cost_per_1m": 0.08,
    },
    "qwen-2.5-7b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "qwen/qwen-2.5-7b-instruct",
        "description": "🎯 Qwen 2.5 7B (Weak - great for ITS demo)",
        "provider": "openai",
        "size": "7B",
        "input_cost_per_1m": 0.15,
        "output_cost_per_1m": 0.15,
        "self_hostable": True,
        "min_gpu": "1x A100 40GB",
        "gpu_cloud_cost_hr": 1.50,
    },
    "deepseek-r1-distill-qwen-7b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "deepseek/deepseek-r1-distill-qwen-7b",
        "description": "🧠🎯 DeepSeek R1 Distill 7B (Reasoning, weak - ITS on reasoning)",
        "provider": "openai",
        "size": "7B",
        "input_cost_per_1m": 0.15,
        "output_cost_per_1m": 0.15,
        "is_reasoning": True,
    },

    # === Medium Models (Good balance) ⚖️ ===
    "llama-4-scout": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "meta-llama/llama-4-scout",
        "description": "⚖️ Llama 4 Scout 17B/109B MoE (Medium - latest Llama)",
        "provider": "openai",
        "size": "17B active",
        "input_cost_per_1m": 0.08,
        "output_cost_per_1m": 0.30,
    },
    "llama-3.3-70b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "meta-llama/llama-3.3-70b-instruct",
        "description": "⚖️ Llama 3.3 70B (Medium)",
        "provider": "openai",
        "size": "70B",
        "input_cost_per_1m": 0.35,
        "output_cost_per_1m": 0.40,
        "self_hostable": True,
        "min_gpu": "4x A100 80GB",
        "gpu_cloud_cost_hr": 12.00,
    },
    "qwen-2.5-72b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "qwen/qwen-2.5-72b-instruct",
        "description": "⚖️ Qwen 2.5 72B (Medium - strong reasoning)",
        "provider": "openai",
        "size": "72B",
        "input_cost_per_1m": 0.35,
        "output_cost_per_1m": 0.40,
    },
    "qwq-32b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "qwen/qwq-32b",
        "description": "🧠⚖️ QwQ 32B (Reasoning specialist)",
        "provider": "openai",
        "size": "32B",
        "input_cost_per_1m": 0.15,
        "output_cost_per_1m": 0.40,
        "is_reasoning": True,
    },
    "gemma-3-27b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "google/gemma-3-27b-it",
        "description": "⚖️ Gemma 3 27B (Medium - good balance)",
        "provider": "openai",
        "size": "27B",
        "input_cost_per_1m": 0.04,
        "output_cost_per_1m": 0.15,
    },

    # === Weak/Small Models (OpenAI Open Source) 🎯 ===
    "gpt-oss-20b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "openai/gpt-oss-20b",
        "description": "🧠🎯 GPT-OSS 20B MoE (Reasoning, 3.6B active - great for ITS demo)",
        "provider": "openai",
        "size": "3.6B active",
        "input_cost_per_1m": 0.02,
        "output_cost_per_1m": 0.10,
        "is_reasoning": True,
    },

    # === Frontier Models 🏆 ===
    "gpt-oss-120b": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "openai/gpt-oss-120b",
        "description": "🧠🏆 GPT-OSS 120B MoE (Frontier - reasoning, 5.1B active)",
        "provider": "openai",
        "size": "5.1B active",
        "input_cost_per_1m": 0.50,
        "output_cost_per_1m": 2.00,
        "is_reasoning": True,
    },
    "deepseek-r1": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "deepseek/deepseek-r1",
        "description": "🧠🏆 DeepSeek R1 671B MoE (Frontier - reasoning specialist)",
        "provider": "openai",
        "size": "671B MoE",
        "input_cost_per_1m": 0.55,
        "output_cost_per_1m": 2.19,
        "is_reasoning": True,
    },
    "llama-4-maverick": {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key_env_var": "OPENROUTER_API_KEY",
        "model_name": "meta-llama/llama-4-maverick",
        "description": "🏆 Llama 4 Maverick 17B/400B MoE (Frontier)",
        "provider": "openai",
        "size": "17B active",
        "input_cost_per_1m": 0.15,
        "output_cost_per_1m": 0.60,
    },

    # ========================================================================
    # VERTEX AI MODELS (Google Cloud)
    # ========================================================================
    # Setup: https://console.cloud.google.com/vertex-ai
    # Authentication: gcloud auth application-default login
    # Or set GOOGLE_APPLICATION_CREDENTIALS to service account JSON path

    # === Gemini Models (via Vertex AI) ===
    "gemini-2.0-flash-vertex": {
        "base_url": "",  # Not used for Vertex AI
        "api_key_env_var": "GOOGLE_APPLICATION_CREDENTIALS",
        "model_name": "gemini-2.0-flash",
        "description": "⚡ Gemini 2.0 Flash (Vertex AI, Fast)",
        "provider": "vertex_ai",
        "vertex_project": os.getenv("VERTEX_PROJECT", "your-gcp-project-id"),
        "vertex_location": os.getenv("VERTEX_LOCATION", "us-east5"),
        "size": "Large",
        "input_cost_per_1m": 0.10,
        "output_cost_per_1m": 0.40,
        "supports_tools": True,
    },
    "gemini-2.5-flash-vertex": {
        "base_url": "",  # Not used for Vertex AI
        "api_key_env_var": "GOOGLE_APPLICATION_CREDENTIALS",
        "model_name": "gemini-2.5-flash-preview-05-20",
        "description": "🧠 Gemini 2.5 Flash (Vertex AI, Reasoning)",
        "provider": "vertex_ai",
        "vertex_project": os.getenv("VERTEX_PROJECT", "your-gcp-project-id"),
        "vertex_location": os.getenv("VERTEX_LOCATION", "us-east5"),
        "size": "Large",
        "input_cost_per_1m": 0.15,
        "output_cost_per_1m": 0.60,
        "supports_tools": True,
        "is_reasoning": True,
    },

    # === Claude Models (via Vertex AI) ===
    "claude-sonnet-vertex": {
        "base_url": "",  # Not used for Vertex AI
        "api_key_env_var": "GOOGLE_APPLICATION_CREDENTIALS",
        "model_name": "claude-sonnet-4-6",
        "description": "🏆 Claude Sonnet 4.6 (Vertex AI, Frontier)",
        "provider": "vertex_ai",
        "vertex_project": os.getenv("VERTEX_PROJECT", "your-gcp-project-id"),
        "vertex_location": os.getenv("VERTEX_LOCATION", "us-east5"),
        "size": "Large",
        "input_cost_per_1m": 3.00,
        "output_cost_per_1m": 15.00,
        "supports_tools": True,
    },
    "claude-haiku-vertex": {
        "base_url": "",  # Not used for Vertex AI
        "api_key_env_var": "GOOGLE_APPLICATION_CREDENTIALS",
        "model_name": "claude-haiku-4-5",
        "description": "⚡ Claude Haiku 4.5 (Vertex AI, Small)",
        "provider": "vertex_ai",
        "vertex_project": os.getenv("VERTEX_PROJECT", "your-gcp-project-id"),
        "vertex_location": os.getenv("VERTEX_LOCATION", "us-east5"),
        "size": "Small",
        "input_cost_per_1m": 1.00,
        "output_cost_per_1m": 5.00,
        "supports_tools": True,
    },

    # ========================================================================
    # IBM GRANITE MODELS (Self-hosted)
    # ========================================================================
    # IBM's open-source Granite models for enterprise AI
    # Note: Requires vLLM server running on GRANITE_BASE_URL
    # Start with: python -m vllm.entrypoints.openai.api_server \
    #   --model ibm-granite/granite-3.3-8b-instruct --port 8100 --max-model-len 8192
    "granite-3.3-8b": {
        "base_url": os.getenv("GRANITE_BASE_URL", "http://localhost:8100/v1"),
        "api_key_env_var": "GRANITE_API_KEY",
        "model_name": "ibm-granite/granite-3.3-8b-instruct",
        "description": "🏢 IBM Granite 3.3 8B Instruct (Self-hosted)",
        "provider": "openai",
        "size": "8B",
        "input_cost_per_1m": 0.0,  # Free if self-hosted
        "output_cost_per_1m": 0.0,
        "self_hostable": True,
        "min_gpu": "1x A10 24GB / RTX 4090",
        "gpu_cloud_cost_hr": 0.50,
    },

    # ========================================================================
    # LOCAL / CUSTOM MODELS
    # ========================================================================
    # For running your own vLLM server with any open-source model
    "local-vllm": {
        "base_url": os.getenv("VLLM_BASE_URL", "http://localhost:8100/v1"),
        "api_key_env_var": "VLLM_API_KEY",
        "model_name": os.getenv("VLLM_MODEL_NAME", "your-model-name"),
        "description": "🔧 Local vLLM (Your own model)",
        "size": os.getenv("VLLM_MODEL_SIZE", "Custom"),
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
        raise ValueError(
            f"API key not found for model '{model_id}'. "
            f"Please set environment variable '{config['api_key_env_var']}'"
        )
    return api_key

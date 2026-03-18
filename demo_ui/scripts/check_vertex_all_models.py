#!/usr/bin/env python3
"""
Check which Vertex AI model families are available in your GCP project.

Tests: Gemini (native), Gemma (native), Llama (Model Garden MaaS), and Claude.
Uses minimal requests to probe availability without burning tokens.

Usage:
    python demo_ui/scripts/check_vertex_all_models.py

    # Override project/location:
    VERTEX_PROJECT=my-project VERTEX_LOCATION=us-central1 python demo_ui/scripts/check_vertex_all_models.py
"""

import os
import sys

from dotenv import load_dotenv

load_dotenv()

PROJECT = os.getenv("VERTEX_PROJECT", "your-gcp-project-id")
LOCATION = os.getenv("VERTEX_LOCATION", "us-east5")

# Regions to try if the primary location fails for a model family
FALLBACK_LOCATIONS = ["us-central1", "us-east4", "europe-west1"]

print(f"Project:  {PROJECT}")
print(f"Location: {LOCATION}")
print("=" * 70)


# ── 1. Gemini (native Vertex AI) ────────────────────────────────────────

def check_gemini(project: str, location: str) -> bool:
    """Test Gemini availability using the Vertex AI SDK."""
    try:
        import vertexai
        from vertexai.generative_models import GenerativeModel

        vertexai.init(project=project, location=location)
        model = GenerativeModel("gemini-2.0-flash-lite")
        response = model.generate_content("Say hi", generation_config={"max_output_tokens": 5})
        _ = response.text
        return True
    except Exception as e:
        print(f"    Error: {e}")
        return False


GEMINI_MODELS = [
    ("gemini-2.0-flash-lite", "Gemini 2.0 Flash Lite (Weak/cheap)"),
    ("gemini-2.0-flash", "Gemini 2.0 Flash (Small/fast)"),
    ("gemini-2.5-flash-preview-05-20", "Gemini 2.5 Flash Preview (Medium)"),
    ("gemini-2.5-pro-preview-05-06", "Gemini 2.5 Pro Preview (Frontier)"),
]


def check_gemini_models(project: str, location: str):
    """Test individual Gemini model availability."""
    try:
        import vertexai
        from vertexai.generative_models import GenerativeModel

        vertexai.init(project=project, location=location)
    except ImportError:
        print("  SKIP: google-cloud-aiplatform not installed")
        print("        pip install google-cloud-aiplatform")
        return

    for model_id, description in GEMINI_MODELS:
        try:
            model = GenerativeModel(model_id)
            response = model.generate_content(
                "Say hi", generation_config={"max_output_tokens": 5}
            )
            _ = response.text
            print(f"  OK    {model_id:<45} {description}")
        except Exception as e:
            err = str(e)[:80]
            print(f"  FAIL  {model_id:<45} {err}")


# ── 2. Gemma (Vertex AI — may need Model Garden deployment) ─────────────

GEMMA_MODELS = [
    ("gemma-3-4b-it", "Gemma 3 4B (Weak)"),
    ("gemma-3-12b-it", "Gemma 3 12B (Small)"),
    ("gemma-3-27b-it", "Gemma 3 27B (Medium)"),
]


def check_gemma_models(project: str, location: str):
    """Test Gemma model availability via Vertex AI native API."""
    try:
        import vertexai
        from vertexai.generative_models import GenerativeModel

        vertexai.init(project=project, location=location)
    except ImportError:
        print("  SKIP: google-cloud-aiplatform not installed")
        return

    for model_id, description in GEMMA_MODELS:
        try:
            model = GenerativeModel(model_id)
            response = model.generate_content(
                "Say hi", generation_config={"max_output_tokens": 5}
            )
            _ = response.text
            print(f"  OK    {model_id:<45} {description}")
        except Exception as e:
            err = str(e)[:80]
            print(f"  FAIL  {model_id:<45} {err}")


# ── 3. Llama (Model Garden MaaS via OpenAI-compatible endpoint) ─────────

LLAMA_MODELS = [
    ("meta/llama-3.2-3b-instruct-maas", "Llama 3.2 3B (Weak)"),
    ("meta/llama-3.2-1b-instruct-maas", "Llama 3.2 1B (Very weak)"),
    ("meta/llama-3.3-70b-instruct-maas", "Llama 3.3 70B (Medium)"),
    ("meta/llama-4-scout-17b-16e-instruct-maas", "Llama 4 Scout (Medium MoE)"),
    ("meta/llama-4-maverick-17b-128e-instruct-maas", "Llama 4 Maverick (Frontier MoE)"),
]


def check_llama_models(project: str, location: str):
    """
    Test Llama availability via Vertex AI Model Garden (MaaS).

    Uses litellm with vertex_ai/ prefix which handles Google ADC auth.
    """
    try:
        import litellm
    except ImportError:
        print("  SKIP: litellm not installed")
        return

    for model_id, description in LLAMA_MODELS:
        try:
            response = litellm.completion(
                model=f"vertex_ai/{model_id}",
                messages=[{"role": "user", "content": "Say hi"}],
                max_tokens=5,
                vertex_project=project,
                vertex_location=location,
            )
            _ = response.choices[0].message.content
            print(f"  OK    {model_id:<45} {description}")
        except Exception as e:
            err = str(e)[:120]
            if "404" in err or "NOT_FOUND" in err:
                print(f"  FAIL  {model_id:<45} NOT FOUND in {location}")
            elif "403" in err or "PERMISSION" in err:
                print(f"  FAIL  {model_id:<45} NO PERMISSION")
            elif "not available" in err.lower() or "region" in err.lower():
                print(f"  FAIL  {model_id:<45} NOT IN REGION {location}")
            else:
                print(f"  FAIL  {model_id:<45} {err}")


# ── 4. Claude (Anthropic on Vertex AI — existing check) ─────────────────

CLAUDE_MODELS = [
    ("claude-sonnet-4-6", "Claude Sonnet 4.6 (Frontier)"),
    ("claude-haiku-4-5", "Claude Haiku 4.5 (Small)"),
    ("claude-3-5-haiku@20241022", "Claude 3.5 Haiku (Previous gen)"),
]


def check_claude_models(project: str, location: str):
    """Test Claude availability via Anthropic Vertex SDK."""
    try:
        from anthropic import AnthropicVertex
    except ImportError:
        print("  SKIP: anthropic[vertex] not installed")
        return

    for model_id, description in CLAUDE_MODELS:
        try:
            client = AnthropicVertex(project_id=project, region=location)
            response = client.messages.create(
                model=model_id,
                max_tokens=5,
                messages=[{"role": "user", "content": "Say hi"}],
            )
            _ = response.content[0].text
            print(f"  OK    {model_id:<45} {description}")
        except Exception as e:
            err = str(e)[:80]
            if "404" in err or "NOT_FOUND" in err:
                print(f"  FAIL  {model_id:<45} NOT FOUND")
            elif "403" in err or "PERMISSION" in err:
                print(f"  FAIL  {model_id:<45} NO PERMISSION")
            else:
                print(f"  FAIL  {model_id:<45} {err}")


# ── Run all checks ──────────────────────────────────────────────────────

def main():
    print()
    print("--- Claude (Anthropic on Vertex AI) ---")
    check_claude_models(PROJECT, LOCATION)

    print()
    print("--- Gemini (Native Vertex AI) ---")
    check_gemini_models(PROJECT, LOCATION)

    print()
    print("--- Gemma (Vertex AI) ---")
    check_gemma_models(PROJECT, LOCATION)

    print()
    print("--- Llama (Vertex AI Model Garden MaaS) ---")
    check_llama_models(PROJECT, LOCATION)

    # Summary
    print()
    print("=" * 70)
    print("Notes:")
    print(f"  - All tests ran against project={PROJECT}, location={LOCATION}")
    print("  - Gemini/Gemma may need a different region (try us-central1)")
    print("  - Llama Model Garden requires accepting terms at:")
    print("    https://console.cloud.google.com/vertex-ai/publishers/meta")
    print("  - Auth: gcloud auth application-default login")


if __name__ == "__main__":
    main()

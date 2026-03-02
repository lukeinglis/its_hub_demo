# ITS Demo UI

A simple web interface for comparing baseline LLM inference vs Inference-Time Scaling (ITS) side by side.

## Overview

This demo provides two entry points from the landing page:

1. **Guided Demo** — A step-by-step walkthrough that walks the user through the ITS experience with pre-populated scenarios and mock data (no backend required)
2. **Interactive Demo** — Full live experience with real API calls, model selection, and custom questions (requires backend)

### Guided Demo Flow

The Guided Demo follows a 6-step progressive disclosure:

| Step | Screen | What the user does |
|------|--------|--------------------|
| 1 | **Goal** | Choose "Improve Model Performance" or "Improve Small Model to Match Frontier" |
| 2 | **Method** | Choose ITS algorithm: Self-Consistency or Best-of-N |
| 3 | **Scenario** | Pick a model scenario (options depend on goal selected in step 1) |
| 4 | **Run** | Review pre-populated question and click Submit |
| 5 | **Trace** | View responses side-by-side, then trigger a staged trace animation |
| 6 | **Performance** | Bar charts comparing Cost, Quality, Latency, and Tokens |

**Branching logic in Step 3:**
- If goal = "Improve Model Performance" → Frontier Model or Open Source Model
- If goal = "Match Frontier" → Same Family (e.g. GPT-4o mini → GPT-4o) or Cross-Family (e.g. Llama 3 8B → GPT-4o)

All guided demo data uses placeholders. See [Plugging in Real Data](#plugging-in-real-data-guided-demo) below for where to replace them.

### Interactive Demo Flow

The Interactive Demo follows a 5-step credential-aware flow:

| Step | Screen | What happens |
|------|--------|--------------|
| 1 | **Access** | Provider info page — clickable cards copy env snippets to clipboard; active providers auto-highlighted green |
| 2 | **Status** | Calls `/providers` and `/models` to detect which credentials are active and list available models |
| 3 | **Scenario** | Choose "Improve Model Performance" or "Match Frontier" |
| 4 | **Configure & Run** | Select model(s), algorithm, budget, choose a curated question or write your own, then submit |
| 5 | **Results** | Live response panes with Cheapest/Fastest badges, expandable reasoning/trace, and performance charts |

The model dropdowns in step 4 are dynamically populated based on which providers were detected in step 2. See [Plugging in Real Data (Interactive Demo)](#plugging-in-real-data-interactive-demo) below.

### Interactive Demo Use Cases

- **Two Scenarios** (selected in step 3):
  1. **Improve Model Performance**: Compare same model with/without ITS (2-column results)
  2. **Match Frontier**: Show small model + ITS matching large frontier model (3-column results)
- **Backend also supports**: Tool Consensus use case (agent tool voting via API — available when running comparisons directly through the `/compare` endpoint)
- **Smart Features**:
  - **Answer Extraction**: Automatically extracts `\boxed{}` answers from math responses for proper consensus voting
  - **Auto-Detection**: Recognizes question types (math, tool_calling, general) and applies optimal configuration
  - **System Prompts**: Applies QWEN system prompt for math questions to ensure consistent answer formatting
- **Backend**: FastAPI server with comprehensive API endpoints
- **Frontend**: Modern HTML/JS interface with expandable sections, algorithm traces, and real-time metrics
- **Models**: OpenAI, OpenRouter, and Vertex AI models
  - **OpenAI**: GPT-4o, GPT-4o Mini, GPT-4 Turbo, GPT-3.5 Turbo
  - **OpenRouter**:
    - Weak models (3B-9B): Llama 3.1/3.2, Mistral 7B, Qwen 2.5, Gemma 2 - **great for ITS demos**
    - Medium models (27B-70B): Llama 3.1/3.3 70B, Qwen 2.5 72B, Gemma 2 27B
    - Frontier models: DeepSeek R1 (reasoning specialist)
    - Code specialists: Qwen 2.5 Coder, DeepSeek Coder V2
  - **Vertex AI Claude**: Sonnet, Opus, Haiku
  - **Vertex AI Gemini**: Pro, Flash
  - **Local**: vLLM server (any self-hosted model)
- **Algorithms**:
  - Outcome-based: Best-of-N, Self-Consistency (with answer extraction and tool voting)
  - Process-based: Beam Search, Particle Filtering, Entropic Particle Filtering, Particle Gibbs
- **Cost Tracking**: Real-time token usage and cost calculation
- **Security**: API keys stored server-side only

## Key Features

### 🎯 Three Powerful Use Cases

1. **Improve Any Model's Performance**
   - See how ITS enhances any model's capabilities
   - 2-column comparison: Baseline vs ITS
   - **NEW**: Answer extraction for math questions ensures proper consensus voting
   - Example: GPT-3.5 achieves 30-60% accuracy improvement on probability questions

2. **Match Frontier at Lower Cost**
   - Show small model + ITS matching large frontier model
   - 3-column comparison: Small, Small+ITS, Frontier
   - **Cost savings: 64-73%** while maintaining quality
   - Example: GPT-4o Mini + ITS saves 64% vs GPT-4o with same accuracy

3. **Tool Consensus for Agent Reliability** 🆕
   - Demonstrate reliable agent decision-making through tool voting
   - 2-column comparison: Single tool call vs Consensus voting
   - Shows distribution of tool selections (e.g., `{'calculate': 6}`)
   - Perfect for showcasing agent reliability in production scenarios

### 🧠 Intelligent Answer Extraction & Auto-Detection 🆕

**Answer Extraction for Math Questions:**
- Automatically extracts `\boxed{}` answers from mathematical responses
- Enables proper consensus voting (votes on "3/4" not "Therefore, the answer is 3/4...")
- Fixes Self-Consistency voting on full text (which was previously broken for math)
- Shows vote counts in algorithm trace: e.g., `{"('\\frac{3',)": 8}` (8/8 consensus)

**Auto-Detection:**
- **Math questions**: Detects LaTeX symbols (`$`, `\frac`, `\boxed`), math keywords → applies QWEN system prompt + answer extraction
- **Tool calling**: Detects `enable_tools=True` → applies tool voting configuration
- **General**: Default full-text matching for other questions
- No manual configuration needed - just paste your question!

**System Prompts:**
- Math questions automatically get: "Please reason step by step, and put your final answer within \boxed{}."
- Ensures consistent answer formatting across all models
- Improves consensus accuracy by 30-60% on mathematical reasoning tasks

### 💰 Real-Time Cost Tracking

- Accurate token counting (input/output)
- Per-request cost calculation in USD
- Cost comparison across models and configurations
- Demonstrates ROI of ITS approach

### 🎨 Modern, Clean UI

- **Response Section**: Complete chatbot answer (always visible)
- **Expandable Reasoning**: Step-by-step trace (click to reveal)
- **Expandable Performance**: All metrics in one place
- **LaTeX Support**: Beautiful math rendering
- **Responsive Design**: Works on desktop and mobile

### 🧮 Comprehensive Algorithm Support

All 6 ITS algorithms tested and working:
- **Outcome-based**: Best-of-N, Self-Consistency
- **Process-based**: Beam Search, Particle Filtering, Entropic Particle Filtering, Particle Gibbs

### 📚 Example Questions Library

16 curated questions inspired by MATH500 and AIME-2024:
- Easy, Medium, and Hard difficulty levels
- Organized by category (Algebra, Calculus, Logic, etc.)
- Each includes expected answer and algorithm recommendations
- Auto-filter by selected algorithm

## Project Structure

```
demo_ui/
├── README.md                                    # This file
├── DEMO_GUIDE.md                               # Presenter's guide with configurations and talking points
├── .env.example                                # Environment variables template
├── backend/
│   ├── __init__.py
│   ├── main.py                                 # FastAPI app (/compare, /models, /providers, /examples)
│   ├── config.py                               # Model registry (28 models across 4 providers)
│   ├── models.py                               # Pydantic request/response models
│   ├── tools.py                                # Tool definitions for agent demos
│   ├── example_questions.py                    # Curated example questions library
│   ├── vertex_lm.py                            # Vertex AI model implementations
│   ├── llm_prm.py                              # LLM-based process reward model
│   └── requirements.txt                        # Backend dependencies
├── tests/
│   ├── conftest.py                             # Shared test fixtures
│   ├── test_config.py                          # Model registry and API key tests
│   ├── test_models.py                          # Pydantic model validation tests
│   ├── test_main_functions.py                  # detect_question_type, calculate_cost tests
│   ├── test_example_questions.py               # Example question query tests
│   └── test_tools.py                           # Safe expression evaluator and tool tests
└── frontend/
    ├── index.html                              # Landing page, wizard HTML shells, inline shared utilities
    ├── guided-demo.js                          # Guided Demo: 6-step flow, mock data, trace animation
    ├── guided-demo.css                         # Guided Demo styles
    ├── interactive-demo.js                     # Interactive Demo: 5-step flow, provider detection, live execution
    ├── interactive-demo.css                    # Interactive Demo styles
    ├── performance-viz-v2.js                   # Performance visualization bar charts
    └── performance-viz-v2.css                  # Performance visualization styles
```

## Setup

### Prerequisites

- Python 3.10+
- `its_hub` library installed (from parent directory)
- API keys for your chosen model provider

### 1. Install its_hub

From the repository root:

```bash
cd ..  # Go to repo root if you're in demo_ui/
uv sync --extra dev
# OR using pip:
# pip install -e .
```

### 2. Install backend dependencies

```bash
cd demo_ui
pip install -r backend/requirements.txt
```

### 3. Configure environment variables

```bash
cp .env.example .env
```

Edit `.env` and add your credentials:

```bash
# Required: OpenAI API (for GPT models + LLM judge)
OPENAI_API_KEY=your-openai-api-key-here

# Optional: OpenRouter (for Llama, Mistral, Qwen, Gemma, DeepSeek models)
# Get your key from: https://openrouter.ai/keys
# Provides access to 15+ open-source and specialized models
OPENROUTER_API_KEY=your-openrouter-api-key-here

# Optional: Google Cloud Vertex AI (for Claude and Gemini)
# Setup: gcloud auth application-default login
VERTEX_PROJECT=your-gcp-project-id
VERTEX_LOCATION=us-central1

# Optional: Local vLLM server
# VLLM_BASE_URL=http://localhost:8100/v1
# VLLM_API_KEY=NO_API_KEY
# VLLM_MODEL_NAME=your-model-name
```

> **Quick Start:** The **Guided Demo** works immediately with no API keys. The **Interactive Demo** requires at least one provider key configured in `demo_ui/.env`. Add a key, restart the backend, and the Interactive Demo will auto-detect it.

**Provider Notes**:
- **OpenAI**: Native OpenAI API - most reliable, best for production demos
- **OpenRouter**: Unified API for open-source models - great for cost-effective demos and model diversity
  - **Note**: OpenRouter does NOT support function/tool calling - use OpenAI models for Tool Consensus demos
- **Vertex AI**: Claude and Gemini models via Google Cloud - requires `gcloud auth application-default login`

## Running the Demo

### Start the backend server

From the `demo_ui` directory:

```bash
# Using uvicorn directly
uvicorn backend.main:app --host 0.0.0.0 --port 8000 --reload

# OR using python
python -m uvicorn backend.main:app --host 0.0.0.0 --port 8000 --reload
```

The backend will be available at `http://localhost:8000`

### Open the frontend

The frontend is served automatically by the backend. Open your browser to:

```
http://localhost:8000
```

You will see the landing page with two options: **Guided Demo** and **Interactive Demo**.

## Demo Guide

For recommended configurations, talking points, and a 5-minute demo flow, see **[DEMO_GUIDE.md](DEMO_GUIDE.md)**.

## Using the Demo

### Landing Page

Open `http://localhost:8000` to see the landing page with two options:

- **Guided Demo** — Walks through pre-built scenarios with mock data. No backend API keys required. Good for presentations.
- **Interactive Demo** — Runs live API calls against real models. Requires at least one provider configured.

### Guided Demo Walkthrough

1. Select a **goal** (Improve Performance or Match Frontier)
2. Choose an **ITS method** (Self-Consistency or Best-of-N)
3. Pick a **model scenario** (options depend on goal)
4. Review the pre-populated **question** and click Submit
5. View side-by-side **responses**, then click "Show ITS Trace" to see the staged animation
6. Review the **Performance** page with Cost, Quality, Latency, and Token charts

All data is placeholder/mock — see [Plugging in Real Data (Guided Demo)](#plugging-in-real-data-guided-demo) to replace with real captured results.

### Interactive Demo Walkthrough

> **Note:** Step 1 auto-detects which providers have valid API keys and highlights their cards green. You need at least one active provider to proceed. If no providers are detected, an inline error will appear on Step 1 with setup instructions.

1. **Access** — Review which providers are available. Click any card to copy its env variable. Active providers are highlighted green automatically.
2. **Status** — Click "Check Access & Continue" to detect configured credentials and see available models.
3. **Scenario** — Choose "Improve Model Performance" (2-column) or "Match Frontier" (3-column).
4. **Configure & Run** — Select model(s), algorithm, budget, and enter your question (from curated list or custom). Click "Run Comparison".
5. **Results** — View response panes with Cheapest/Fastest badges, expandable reasoning and algorithm traces, and performance comparison charts.

### Understanding the Results

Each result pane shows:

- **Response** (always visible): The model's chatbot-style answer with LaTeX math support
- **Latency / Cost / Tokens**: Summary badges below the response
- **View Full Reasoning** (expandable): Step-by-step reasoning if the response contains structured steps
- **Performance Details** (expandable): Latency, cost, input/output tokens, model size
- **Algorithm Trace** (expandable, ITS pane only): Vote counts (Self-Consistency) or score leaderboard (Best-of-N)

Visual indicator badges:
- **Cheapest** — pane with the lowest cost
- **Fastest** — pane with the lowest latency

For curated questions with a known answer, the **Expected Answer** is shown above the results.

## Plugging in Real Data (Guided Demo)

The Guided Demo uses placeholder/mock data throughout. All data lives in `frontend/guided-demo.js` in clearly marked sections:

| What to replace | Where in `guided-demo.js` | Format |
|-----------------|---------------------------|--------|
| **Scenario definitions** | `GUIDED_SCENARIOS` object | Add/remove/edit scenario cards (title, icon, model names) |
| **Demo questions** | `GUIDED_MOCK_QUESTIONS` object | Key = `${scenarioId}_${method}`, value = question string |
| **Model responses** | `getMockResponse()` function | Return `{ baseline: {...}, its: {...}, frontier?: {...}, trace: {...} }` |
| **Performance metrics** | `getMockPerformance()` function | Return `{ columns, cost, latency, tokens, quality }` arrays |
| **Trace data** | Inside `getMockResponse()` return | Self-Consistency: `vote_counts` object; Best-of-N: `scores` array |

### Example: replacing a question

```javascript
// In GUIDED_MOCK_QUESTIONS, change:
'improve_frontier_self_consistency': 'What is 144 / 12 + 7 * 3 - 5?',
// To your real demo question:
'improve_frontier_self_consistency': 'Your real question here',
```

### Example: replacing mock responses with captured data

```javascript
// In getMockResponse(), add a specific key check at the top:
function getMockResponse(scenarioId, method) {
    const key = `${scenarioId}_${method}`;
    // Add your real captured data here:
    if (key === 'improve_frontier_self_consistency') {
        return {
            baseline: { response: '...', latency_ms: 523, input_tokens: 24, output_tokens: 42, cost_usd: 0.000021 },
            its:      { response: '...', latency_ms: 1022, input_tokens: 192, output_tokens: 48, cost_usd: 0.000168 },
            trace:    { algorithm: 'self_consistency', candidates: [...], vote_counts: {...}, total_votes: 8 },
        };
    }
    // ... fallback to placeholder data
}
```

### Adding a new scenario

1. Add an entry to `GUIDED_SCENARIOS` with a unique `id`
2. Add a question to `GUIDED_MOCK_QUESTIONS` for each method (`${id}_self_consistency`, `${id}_best_of_n`)
3. Optionally add specific response data in `getMockResponse()`
4. The scenario will automatically appear in Step 3 based on its `goal` field

## Plugging in Real Data (Interactive Demo)

The Interactive Demo logic lives in `frontend/interactive-demo.js`. Key extension points:

| What to customize | Where in `interactive-demo.js` | Notes |
|-------------------|-------------------------------|-------|
| **Curated prompts** | `IW_CURATED_PROMPTS` object | Key = `${scenario}_${algorithm}`, value = array of `{q, a}` |
| **Result presentation** | `iwBuildResultPane()` function | Customize badges, indicators, answer formatting |
| **Judge integration** | `iwBuildResultPane()` badges section | Add `correct` badge by comparing against expected answer or LLM judge |
| **Provider docs** | Step 1 HTML in `index.html` | Update setup instructions per provider |
| **Model grouping** | `iwPopulateConfig()` function | Customize how models are grouped in dropdowns |

### Adding curated prompts

```javascript
// In IW_CURATED_PROMPTS, add questions per scenario+algorithm:
'improve_performance_self_consistency': [
    { q: 'Your math question here', a: 'Expected answer' },
    { q: 'Open-ended question', a: null },  // null = no expected answer
],
```

### Adding a judge/evaluation mechanism

The result panes already support a `correct` badge class. To wire up evaluation:

1. After receiving results, compare `data.its.answer` against `iwState.expectedAnswer`
2. Or call an LLM judge endpoint to score response quality
3. Add badge: `<span class="iw-pane-badge correct">Correct</span>`

## API Reference

### GET /health

Health check endpoint.

**Response:**
```json
{
  "status": "ok",
  "message": "ITS Demo API is running"
}
```

### GET /providers

Check which model providers have credentials configured.

**Response:**
```json
{
  "providers": {
    "openai": { "enabled": true, "name": "OpenAI", "description": "GPT-4o, ...", "env_var": "OPENAI_API_KEY", "setup": "export OPENAI_API_KEY=sk-..." },
    "openrouter": { "enabled": false, "name": "OpenRouter", "..." : "..." },
    "vertex_ai": { "enabled": false, "..." : "..." },
    "local": { "enabled": false, "..." : "..." }
  },
  "any_enabled": true
}
```

### GET /models

List available models. Now includes `provider` field for each model.

**Response:**
```json
{
  "models": [
    {
      "id": "gpt-4o-mini",
      "description": "OpenAI GPT-4o Mini",
      "model_name": "gpt-4o-mini",
      "size": "Small",
      "supports_tools": true,
      "provider": "openai"
    }
  ]
}
```

### GET /examples

Get example questions, optionally filtered by algorithm.

**Query Parameters:**
- `algorithm` (optional): Filter questions by algorithm (e.g., `beam_search`)

**Response:**
```json
{
  "examples": [
    {
      "question": "Solve the quadratic equation: x^2 + 5x + 6 = 0",
      "category": "Algebra",
      "difficulty": "Medium",
      "expected_answer": "x = -2 or x = -3 (factoring: (x+2)(x+3) = 0)",
      "best_for": ["beam_search", "particle_filtering"],
      "why": "Multi-step solution with verification"
    }
  ]
}
```

### POST /compare

Compare baseline vs ITS inference.

**Request (Improve Model Use Case):**
```json
{
  "question": "What is the derivative of x^3 + 2x^2?",
  "model_id": "gpt-4o-mini",
  "algorithm": "best_of_n",
  "budget": 4,
  "use_case": "improve_model"
}
```

**Request (Match Frontier Use Case):**
```json
{
  "question": "What is the derivative of x^3 + 2x^2?",
  "model_id": "gpt-4o-mini",
  "frontier_model_id": "gpt-4o",
  "algorithm": "best_of_n",
  "budget": 4,
  "use_case": "match_frontier"
}
```

**Request (Tool Consensus Use Case):** 🆕
```json
{
  "question": "What's the compound annual growth rate if I invest $1000 and it grows to $2000 in 5 years?",
  "model_id": "gpt-4o-mini",
  "algorithm": "self_consistency",
  "budget": 6,
  "use_case": "tool_consensus",
  "enable_tools": true,
  "tool_vote": "tool_name"
}
```

**Parameters:**
- `question` (string, required): The question to answer
- `model_id` (string, required): Model to use for ITS
- `algorithm` (string, required): ITS algorithm (best_of_n, self_consistency, beam_search, particle_filtering, entropic_particle_filtering, particle_gibbs)
- `budget` (integer, required): Computation budget (1-32)
- `use_case` (string, optional): "improve_model" (default), "match_frontier", or "tool_consensus"
- `frontier_model_id` (string, optional): Large model to compare against (required for match_frontier)
- `question_type` (string, optional): "auto" (default), "math", "tool_calling", or "general" - auto-detects if not specified
- `enable_tools` (boolean, optional): Enable tool calling for agent scenarios (default: false)
- `tool_vote` (string, optional): Tool voting strategy - "tool_name", "tool_args", or "tool_hierarchical"
- `exclude_args` (array, optional): Argument names to exclude from tool voting (e.g., `["timestamp", "id"]`)

**Response (Improve Model):**
```json
{
  "baseline": {
    "answer": "...",
    "latency_ms": 1234,
    "log_preview": "",
    "model_size": "Small",
    "cost_usd": 0.000007,
    "input_tokens": 15,
    "output_tokens": 50
  },
  "its": {
    "answer": "...",
    "latency_ms": 2345,
    "log_preview": "",
    "model_size": "Small",
    "cost_usd": 0.000021,
    "input_tokens": 45,
    "output_tokens": 150
  },
  "meta": {
    "model_id": "gpt-4o-mini",
    "algorithm": "best_of_n",
    "budget": 4,
    "run_id": "uuid-here",
    "use_case": "improve_model"
  },
  "small_baseline": null
}
```

**Response (Match Frontier):**
```json
{
  "baseline": {
    "answer": "...",
    "latency_ms": 1000,
    "model_size": "Large",
    "cost_usd": 0.000117,
    "input_tokens": 15,
    "output_tokens": 50
  },
  "its": {
    "answer": "...",
    "latency_ms": 3500,
    "model_size": "Small",
    "cost_usd": 0.000021,
    "input_tokens": 45,
    "output_tokens": 150
  },
  "small_baseline": {
    "answer": "...",
    "latency_ms": 500,
    "model_size": "Small",
    "cost_usd": 0.000007,
    "input_tokens": 15,
    "output_tokens": 50
  },
  "meta": {
    "model_id": "gpt-4o-mini",
    "frontier_model_id": "gpt-4o",
    "algorithm": "best_of_n",
    "budget": 4,
    "run_id": "uuid-here",
    "use_case": "match_frontier"
  }
}
```

## Adding Custom Models

Edit `backend/config.py` and add your model to the `MODEL_REGISTRY`:

```python
MODEL_REGISTRY = {
    "my-custom-model": {
        "base_url": "https://api.example.com/v1",
        "api_key_env_var": "MY_CUSTOM_API_KEY",
        "model_name": "my-model-name",
        "description": "My Custom Model",
    },
    # ... existing models
}
```

Then add the corresponding API key to your `.env` file.

## OpenRouter Models

OpenRouter provides access to 15+ open-source and specialized models through a unified API. Get your API key at https://openrouter.ai/keys.

### Available Model Categories

**🎯 Weak Models (3B-9B) - Great for ITS Demos:**
- **Llama 3.1 8B** - Good balance of performance and cost
- **Llama 3.2 3B** - Very weak, shows dramatic ITS improvement
- **Mistral 7B** - Popular open-source model
- **Qwen 2.5 7B** - Strong reasoning for its size
- **Gemma 2 9B** - Google's open model

**Why use weak models?** They make mistakes on baseline but ITS corrects them, creating impressive before/after demos.

**⚖️ Medium Models (27B-70B) - Good Balance:**
- **Llama 3.1/3.3 70B** - Latest Meta models
- **Qwen 2.5 72B** - Strong reasoning capabilities
- **Gemma 2 27B** - Good mid-size option

**🏆 Frontier Models:**
- **DeepSeek R1** - Reasoning specialist

**💻 Code Specialists:**
- **Qwen 2.5 Coder 32B** - Code-focused model
- **DeepSeek Coder V2** - Advanced coding model

### OpenRouter Best Practices

**✅ Use OpenRouter for:**
- **Improve Model** demos - weak models show dramatic improvement
- **Match Frontier** demos - Mistral 7B + ITS vs GPT-4o saves 73%
- **Cost-effective testing** - open-source models are much cheaper
- **Model diversity** - test across different architectures

**❌ Don't use OpenRouter for:**
- **Tool Consensus** demos - OpenRouter doesn't support function calling
- Production tool calling scenarios - use OpenAI models instead

**⚠️ Important Notes:**
- Model availability changes - check https://openrouter.ai/models for current list
- Some models may have rate limits or timeouts
- For live demos, test your chosen model first
- If you get "No endpoints found", the model may no longer be available

### Recommended Demo Configurations

**Best cost/impact ratio:**
```
Use Case: Match Frontier
Small Model: mistral-7b (via OpenRouter - $0.06/1M)
Frontier Model: gpt-4o (via OpenAI - $10/1M)
Cost Savings: 73%
```

**Most dramatic improvement:**
```
Use Case: Improve Model
Model: llama-3.2-3b (very weak - makes many mistakes)
Algorithm: self_consistency
Budget: 8
Expected: Baseline often wrong, ITS corrects through consensus
```

## Algorithm Details

### Outcome-Based vs Process-Based

- **Outcome-based algorithms** generate complete responses and evaluate only the final output
- **Process-based algorithms** generate responses step-by-step and evaluate intermediate reasoning

The demo uses an **LLM-based Process Reward Model** for process-based algorithms, which:
- Scores partial responses during generation
- Enables step-by-step reasoning evaluation
- Works without requiring a separate reward model server
- Uses GPT-4o Mini as the judge (configurable in code)

For production use with process-based algorithms, consider using a dedicated process reward model server (e.g., via vLLM) for better performance and cost efficiency.

## Features

### ✅ Implemented

- ✅ **Guided Demo Flow**: 6-step progressive disclosure with goal → method → scenario → submit → trace → performance
- ✅ **Interactive Demo Flow**: 5-step credential-aware live experience with provider detection and dynamic model availability
- ✅ **Provider Detection**: `/providers` endpoint checks configured API keys; frontend shows status and available models
- ✅ **Visual Indicators**: Cheapest/Fastest badges on result panes, extensible for correctness judge
- ✅ **Trace Animation**: Staged 3-phase visualization showing Generate → Evaluate → Select
- ✅ **Performance Page**: Bar charts for Cost, Quality, Latency, and Tokens with best-value highlighting
- ✅ **Branching Scenarios**: Model options change based on selected goal (Improve vs Match Frontier)
- ✅ **Three Use Cases**: Improve model performance, match frontier at lower cost, or demonstrate tool consensus
- ✅ **Answer Extraction**: Extracts `\boxed{}` answers from math responses for proper consensus voting
- ✅ **Auto-Detection**: Recognizes math, tool_calling, and general question types automatically
- ✅ **Tool Consensus**: Agent reliability through democratic tool voting
- ✅ **Algorithm Traces**: Expandable visualization of vote counts, tool distributions, and candidate responses
- ✅ **6 ITS Algorithms**: Outcome-based and process-based algorithms
- ✅ **Cost Tracking**: Real-time token counting and cost calculation
- ✅ **Performance Metrics**: Latency, model size, tokens, and cost per request
- ✅ **Expandable UI**: Clean response view with collapsible reasoning and metrics
- ✅ **Example Questions**: Curated problems across difficulty levels (math and tool calling)
- ✅ **Multi-Provider Support**: OpenAI and Vertex AI (Claude, Gemini)
- ✅ **LaTeX Math Rendering**: Proper formatting for mathematical content

### 🔧 Potential Enhancements

- **Step-by-step visualization**: Show intermediate reasoning steps during generation
- **Dedicated PRM server**: Use vLLM with specialized process reward models for better performance
- **Quality scoring**: Automatic evaluation of answer correctness
- **Batch testing**: Evaluate multiple questions for systematic benchmarking
- **Response comparison**: Side-by-side diff view for answer comparison
- **Export results**: Download comparison results as JSON/CSV

## Troubleshooting

### "API key not found" error

Make sure:
1. You created a `.env` file from `.env.example`
2. You added your API key to the `.env` file
3. The backend server was restarted after adding the key

### CORS errors in browser console

The backend is configured to allow all origins for local development. If you're running in production, update the CORS configuration in `backend/main.py`.

### Module import errors

Make sure `its_hub` is installed:
```bash
cd ..  # Go to repo root
pip install -e .
```

## Development

The backend uses:
- **FastAPI**: Modern async web framework
- **Pydantic**: Data validation
- **its_hub**: ITS algorithms library

The frontend uses:
- Vanilla HTML/CSS/JavaScript (no build step required)
- Fetch API for HTTP requests

Frontend file responsibilities:
- `index.html` — Landing page, wizard HTML shells, inline styles/scripts, shared utility functions
- `guided-demo.js` — Guided Demo flow: state management, step navigation, mock data, trace animation, performance charts
- `guided-demo.css` — Guided Demo styles
- `interactive-demo.js` — Interactive Demo flow: provider detection, live execution, results rendering, performance visualization
- `interactive-demo.css` — Interactive Demo styles
- `performance-viz-v2.js` / `.css` — Performance visualization component (used by both modes)

To modify the backend, edit files in `backend/` and the server will auto-reload (if started with `--reload` flag).

To modify the frontend, edit the relevant file and refresh your browser. Both demo flows are self-contained in their respective JS files — they override entry-point functions from `index.html` at load time (`initGuidedWizard` for guided, `selectExperience` for interactive).

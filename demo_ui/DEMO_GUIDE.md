# ITS Demo Guide

Quick reference for presenting the ITS demo. For setup and architecture details, see `README.md`.

---

## Pre-Demo Checklist

1. Backend running: `cd demo_ui && uvicorn backend.main:app --host 0.0.0.0 --port 8000` (venv active)
2. Open `http://localhost:8000`
3. Hard refresh: `Cmd+Shift+R` (Mac) or `Ctrl+Shift+R` (Windows/Linux)
4. Choose **Guided Demo** (pre-captured data, no API keys) or **Interactive Demo** (live API calls, requires keys)

---

## Guided Demo — Recommended Flow

The guided demo uses pre-captured real API responses. No API keys required. Each scenario walks through a 6-step flow: Goal → Method → Scenario → Question → Responses/Trace → Performance.

### Demo 1: Tool Consensus — Agent Reliability (show first)

**Why first:** Most unique and differentiated feature. Shows how ITS improves agent tool selection through democratic voting.

| Step | Selection |
|------|-----------|
| Goal | Tool Consensus |
| Scenario | **Stock Price Lookup** (BFCL multiple_56) |

**What happens:**
- GPT-4.1 Nano is given 4 tools: `web_search`, `calculate`, `get_data`, `code_executor`
- Question: "What is the current stock price of Apple (AAPL)?"
- Baseline picks `web_search` (unstructured, unreliable)
- ITS generates 8 candidates — **6/8 vote for `get_data`** (structured API, correct tool)
- BFCL ground truth confirms `get_data` is correct

**Key talking point:** "ITS acts like a team of agents voting on the best approach — 6 out of 8 agreed on the structured API rather than a generic web search. This is how you build reliable agentic systems."

---

### Demo 2: Match Frontier — Cost Savings (show second)

**Why second:** Clear ROI story. Shows a tiny model matching an expensive frontier model at a fraction of the cost.

| Step | Selection |
|------|-----------|
| Goal | Match Frontier |
| Method | Self-Consistency |
| Scenario | **Cross-Family Match** (Llama 3.2 3B → GPT-4o) |

**What happens:**
- Question: derangement problem (counting letter arrangements)
- Expected answer: 44
- Llama 3.2 3B + ITS: **5/8 candidates vote for 44** (correct) at $0.0002
- GPT-4o baseline: correct at $0.0077
- **97% cost savings** while matching frontier quality

**Key talking point:** "A 3-billion parameter open-source model on a single GPU matches GPT-4o quality at 97% lower cost. And unlike GPT-4o, you can self-host this model on-premise or in air-gapped environments."

---

### Demo 3: Improve Model — Accuracy Gains (show third)

**Why third:** Shows the broadest use case — same model, better results through inference-time scaling.

| Step | Selection |
|------|-----------|
| Goal | Improve Performance |
| Method | Self-Consistency |
| Scenario | **Small Commercial Model** (GPT-4.1 Nano) |

**What happens:**
- Question: 5-digit palindrome counting problem
- Expected answer: 300
- Baseline: may get wrong answer in a single pass
- ITS: generates 8 candidates, **4/8 vote for 300** (correct) through majority voting

**Key talking point:** "The model is unchanged — same weights, same API. We just call it multiple times and vote on the answer. A small cost increase for a significant accuracy improvement."

---

## Quick Reference Table

| Demo | Scenario | Model | Method | Budget | Key Metric |
|------|----------|-------|--------|--------|------------|
| Tool Consensus | Stock Price Lookup | GPT-4.1 Nano | Self-Consistency | 8 | 6/8 vote for correct tool |
| Match Frontier | Cross-Family (Llama→GPT-4o) | Llama 3.2 3B | Self-Consistency | 8 | 97% cost savings |
| Improve Model | Small Commercial Model | GPT-4.1 Nano | Self-Consistency | 8 | Corrects wrong baseline |

---

## 5-Minute Demo Script

### Tool Consensus (1.5 min)
1. Select **Tool Consensus** goal → **Stock Price Lookup** scenario
2. Review the question and available tools, click Submit
3. Compare: baseline chose `web_search`, ITS chose `get_data`
4. Expand the trace — show the 6/8 vote distribution
5. Point out the BFCL ground truth confirmation

### Match Frontier (2 min)
1. Select **Match Frontier** goal → **Self-Consistency** method → **Cross-Family Match**
2. Click Submit, review all three columns (small baseline, small+ITS, frontier)
3. Highlight: ITS matches frontier answer at 97% lower cost
4. Click through to Performance — show the savings summary card
5. Mention: "This model runs on a single GPU — you can self-host it"

### Improve Model (1.5 min)
1. Select **Improve Performance** goal → **Self-Consistency** → **Small Commercial Model**
2. Click Submit, compare baseline vs ITS answers
3. Expand the trace — show vote distribution
4. Click through to Performance — show cost multiplier vs accuracy gain

---

## The One Demo (if time-limited)

Use the **Cross-Family Match Frontier** scenario. It demonstrates cost savings, quality matching, self-hosting potential, and answer extraction in a single run.

---

## Audience-Specific Sequences

| Audience | Order | Emphasis |
|----------|-------|----------|
| **Technical** (engineers) | Tool Consensus → Improve Model → Match Frontier | Algorithm traces, vote distributions, tool voting, answer extraction |
| **Business** (executives) | Match Frontier → Tool Consensus → Improve Model | 97% cost savings, self-hosting, agent reliability, no retraining needed |
| **Research** (ML/academic) | Improve Model → Match Frontier → Tool Consensus | Self-consistency methodology, BFCL benchmark, scaling laws at inference |

---

## Best-of-N Demos

The guided demo also includes Best-of-N variants for Improve Model scenarios. To show Best-of-N instead of Self-Consistency:

- Select **Best-of-N** as the method in Step 2
- The trace shows LLM judge scoring (not voting) — a different ITS approach
- Good for showing that ITS isn't a single technique but a family of methods

---

## Pro Tips

- **Budget 6-8** is ideal — clear consensus, fast enough for live demos
- **Medium difficulty** math questions work best (easy = no improvement to show; hard = risk of timeout)
- The guided demo badge shows "Using captured results" (green) — this confirms you're seeing real API responses
- If you see "Using example data" (amber), the captured data file may be missing — recapture with `python scripts/capture_guided_scenarios.py`
- **Avoid GPT-4o** for "Improve Model" — it's too good, no improvement to demonstrate

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| Backend not reachable | Run `uvicorn backend.main:app --port 8000` from `demo_ui/` with venv active |
| Empty dropdowns / broken UI | Hard refresh (`Cmd+Shift+R`) |
| "Using example data" badge | Recapture: `python scripts/capture_guided_scenarios.py` |
| Want to try Interactive Demo | Need at least `OPENAI_API_KEY` in `.env` — see README Scenario 2 |

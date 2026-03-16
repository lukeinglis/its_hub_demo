# ITS Demo Guide

Quick reference for presenting the ITS demo. For setup, architecture, and API details, see `README.md`.

---

## Pre-Demo Checklist

1. Backend running: `cd demo_ui && uv run uvicorn backend.main:app --host 0.0.0.0 --port 8000`
2. Open `http://localhost:8000`
3. Hard refresh: `Cmd+Shift+R` (Mac) or `Ctrl+Shift+R` (Windows)
4. Quick test: Improve Model tab → GPT-3.5 Turbo → "What is 2+2?" → Run

---

## Recommended Configurations

### Use Case 1: Agent Tool Consensus (show first)

**Why first:** Most unique feature — shows agent reliability through democratic voting.

| Setting | Value |
|---------|-------|
| Tab | Agent Tool Consensus |
| Model | GPT-4.1 Nano |
| Algorithm | Self-Consistency |
| Budget | 8 |

**Question:**
> What's the compound annual growth rate if I invest $1000 and it grows to $2000 in 5 years?

**Expected:** Auto-detected as `tool_calling`. Baseline makes 1 tool call; ITS shows consensus like `{'calculate': 6}`. Result: CAGR = 14.87%.

**Talking point:** "ITS prevents inconsistent tool choices through democratic voting — like having multiple agents vote on the best approach."

---

### Use Case 2: Match Frontier (show second)

**Why second:** Clear ROI story with cost savings.

| Setting | Value |
|---------|-------|
| Tab | Match Frontier with Smaller Model |
| Small Model | GPT-3.5 Turbo |
| Frontier Model | GPT-4o |
| Algorithm | Self-Consistency |
| Budget | 6 |

**Question:**
> Let $x$ and $y$ be positive real numbers such that $x + y = 10$ and $x^2 + y^2 = 60$. Find the value of $x^3 + y^3$.

**Expected:** Auto-detected as `math`. All three outputs use `\boxed{400}`. Small + ITS costs less than frontier.

**Talking point:** "The small model with ITS matches the frontier at lower cost — same quality, structured answer extraction ensures fair comparison."

---

### Use Case 3: Improve Model Performance (show third)

**Why third:** Shows general applicability and answer extraction in action.

| Setting | Value |
|---------|-------|
| Tab | Improve Model Performance |
| Model | GPT-3.5 Turbo |
| Algorithm | Self-Consistency |
| Budget | 8 |

**Question:**
> Alice and Bob each independently roll a standard six-sided die. What is the probability that the product of their rolls is even? Express your answer as a common fraction.

**Expected:** Auto-detected as `math`. Baseline may get wrong answer. ITS achieves `\boxed{\frac{3}{4}}` with 8/8 vote consensus on extracted answer.

**Talking point:** "ITS votes on the extracted answer, not full text — 8 out of 8 responses agree on 3/4 through consensus."

---

## Quick Reference Table

| Use Case | Model | Algorithm | Budget | Expected Improvement |
|----------|-------|-----------|--------|---------------------|
| Tool Consensus | GPT-4.1 Nano | Self-Consistency | 8 | Auto-detect + reliable consensus |
| Match Frontier | GPT-3.5 Turbo → GPT-4o | Self-Consistency | 6 | 14-44% cost savings |
| Improve Model | GPT-3.5 Turbo | Self-Consistency | 8 | 30-60% accuracy gain |

---

## 5-Minute Demo Flow

### Demo 1: Tool Consensus (1.5 min)
1. Click "Agent Tool Consensus" tab
2. Select GPT-4.1 Nano, Self-Consistency, Budget 8
3. Paste the CAGR question → Run
4. Click "Algorithm Trace" to show tool voting
5. Highlight: auto-detection, consensus distribution, correct result

### Demo 2: Match Frontier (2 min)
1. Click "Match Frontier" tab
2. Select GPT-3.5 Turbo / GPT-4o, Self-Consistency, Budget 6
3. Paste the algebra question → Run
4. Highlight: all three use `\boxed{400}`, cost savings vs frontier

### Demo 3: Improve Model (1.5 min)
1. Click "Improve Model Performance" tab
2. Select GPT-3.5 Turbo, Self-Consistency, Budget 8
3. Paste the probability question → Run
4. Click "Algorithm Trace"
5. Highlight: 8/8 consensus on extracted answer, baseline may be wrong

---

## Alternative Questions

If the primary questions fail or you want variety:

**Tool Consensus alternative:**
> Calculate the monthly payment on a $300,000 mortgage at 6.5% interest over 30 years.

**Match Frontier alternative (73% savings with Mistral 7B → GPT-4o, Budget 8):**
> In an arithmetic sequence, the 5th term is 23 and the 12th term is 58. What is the 20th term?

**Improve Model alternative (Mistral 7B, Budget 8):**
> A box contains 3 red balls, 4 blue balls, and 5 green balls. Two balls are drawn at random without replacement. What is the probability that both balls are the same color? Express your answer as a common fraction.

---

## The One Demo (if time-limited)

Use **Match Frontier** with the algebra question above. It shows cost savings + quality matching + answer extraction + auto-detection in a single run.

---

## Audience-Specific Sequences

| Audience | Order | Focus |
|----------|-------|-------|
| **Technical** (engineers) | Tool Consensus → Improve Model → Match Frontier | Auto-detection, answer extraction, algorithm traces |
| **Business** (executives) | Match Frontier → Improve Model → Tool Consensus | ROI (cost savings), quality gains, reliability |
| **Research** (ML/academic) | Improve Model → Tool Consensus → Match Frontier | Voting methodology, hierarchical tool voting, fair evaluation |

---

## Pro Tips

- **Budget 6-8** is ideal for demos — clear consensus, fast enough
- **Medium difficulty** math questions work best (easy = no improvement to show; hard = risk of timeout)
- **Avoid GPT-4o** for "Improve Model" — it's too good, no improvement to demonstrate
- **Avoid OpenRouter models** for Tool Consensus — they don't support function calling
- Self-Consistency is best for demos; Best-of-N requires the LLM judge and is slower

---

## Troubleshooting

| Error | Fix |
|-------|-----|
| "No endpoints found for tool use" | Wrong model for Tool Consensus — use GPT-4.1 Nano or GPT-3.5 Turbo |
| Model timeout / connection error | Switch to an OpenAI model (GPT-4.1 Nano recommended) |
| Empty dropdowns | Hard refresh (`Cmd+Shift+R`) |
| Backend not reachable | Run `uvicorn backend.main:app --port 8000` from `demo_ui/` |

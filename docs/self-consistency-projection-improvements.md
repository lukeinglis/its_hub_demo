# Self-Consistency Projection Improvements

This document describes improvements to the self-consistency algorithm's answer
extraction and voting that would make it work correctly out-of-the-box for common
question types. These changes would live in `its_hub/algorithms/self_consistency.py`.

## Background: How Self-Consistency Should Work

Per [Wang et al., 2022](https://arxiv.org/abs/2205.11822), self-consistency works by:

1. Sampling N diverse reasoning chains from the model
2. **Extracting only the final answer** from each chain
3. Taking a majority vote over the extracted answers

The key insight is that different reasoning paths converge on the same answer.
You vote on the *answer*, never the full reasoning text.

## Current Behavior

The `_default_projection_func` in `self_consistency.py` simply calls `.strip()`
on the full response text. This means:

- Two responses that both conclude "45" but with different reasoning get
  separate vote entries, because their full text differs.
- With 8 candidates that all arrive at the same answer, you get 8 entries
  with 1 vote each instead of 1 entry with 8 votes.
- The `selected_index` may point to an arbitrary candidate rather than one
  from the majority-answer group.

The `create_regex_projection_function` helper exists but:
- Returns tuples even for single patterns (e.g., `('45',)` instead of `'45'`),
  which creates awkward keys in `response_counts`.
- Must be explicitly provided by every caller — there's no smart default.

## Proposed Changes

### 1. Smarter `_default_projection_func`

Replace the current `.strip()` default with answer extraction:

```python
def _default_projection_func(response: str) -> str:
    """Default projection that extracts the final answer for voting.

    Tries, in order:
    1. \\boxed{...} extraction (math)
    2. Explicit answer patterns ("Final Answer:", "Therefore, the answer is...")
    3. Last short line that looks like an answer
    4. Last paragraph (as a rough conclusion)
    5. Full text stripped (current fallback)
    """
    if not response:
        return ""

    # 1. Boxed answer
    boxed_idx = response.find('\\boxed{')
    if boxed_idx != -1:
        brace_count = 0
        start = boxed_idx + 7
        for i in range(start, len(response)):
            if response[i] == '{':
                brace_count += 1
            elif response[i] == '}':
                if brace_count == 0:
                    if i > start:
                        return response[start:i].strip()
                    break
                brace_count -= 1

    # 2. Explicit patterns
    import re
    patterns = [
        r'Final Answer:\s*(.+?)(?:\n\n|$)',
        r'Answer:\s*(.+?)(?:\n\n|$)',
        r'Therefore,?\s+the\s+(?:answer|value|result)\s+(?:is|equals?)\s+(.+?)(?:\.|$)',
        r'Therefore,?\s+(.+?)(?:\n\n|$)',
        r'Thus,?\s+(.+?)(?:\n\n|$)',
        r'So,?\s+(.+?)(?:\n\n|$)',
        r'In conclusion,?\s+(.+?)(?:\n\n|$)',
    ]
    for pattern in patterns:
        match = re.search(pattern, response, re.IGNORECASE | re.DOTALL)
        if match:
            answer = match.group(1).strip()
            if len(answer) < 200:
                return answer.lower()

    # 3. Short last line
    lines = response.strip().split('\n')
    last_line = lines[-1].strip()
    if last_line and len(last_line) < 150:
        return last_line.lower()

    # 4. Last paragraph
    paragraphs = [p.strip() for p in response.strip().split('\n\n') if p.strip()]
    if paragraphs:
        return paragraphs[-1].lower()

    # 5. Fallback
    return response.strip()
```

This makes self-consistency work correctly for math, general Q&A, and reasoning
tasks without any caller configuration.

### 2. Fix `create_regex_projection_function` for single patterns

When a single pattern is provided, return a plain string instead of a 1-tuple:

```python
def create_regex_projection_function(patterns):
    if isinstance(patterns, str):
        patterns = [patterns]

    compiled = [re.compile(p, re.DOTALL | re.IGNORECASE) for p in patterns]
    single_pattern = len(compiled) == 1

    def projection_function(response: str):
        results = []
        if response is None:
            response = ""
        for pattern in compiled:
            match = pattern.search(response)
            if match and match.groups():
                results.append(match.group(1).strip())
            elif match:
                results.append(match.group(0).strip())
            else:
                results.append(None)

        # For single patterns, return the string directly (not a tuple)
        # so response_counts keys are clean strings
        if single_pattern:
            return results[0]
        return tuple(results)

    return projection_function
```

This eliminates the `('45',)` tuple-string problem at the source.

### 3. Built-in projection presets

Add convenience presets so callers don't need to construct their own:

```python
class ProjectionPreset:
    MATH = "math"        # \\boxed{} extraction
    GENERAL = "general"  # Answer pattern extraction
    EXACT = "exact"      # Full text matching (current default)

class SelfConsistency:
    def __init__(
        self,
        consistency_space_projection_func=None,
        projection_preset=None,  # NEW: "math", "general", or "exact"
        ...
    ):
        if projection_preset == "math":
            self.consistency_space_projection_func = create_regex_projection_function(
                r'\\boxed\{([^}]+)\}'
            )
        elif projection_preset == "general":
            self.consistency_space_projection_func = _default_projection_func
        elif consistency_space_projection_func:
            self.consistency_space_projection_func = consistency_space_projection_func
        else:
            self.consistency_space_projection_func = _default_projection_func
```

### 4. Case normalization

The projection function should normalize case so "The answer is 45" and
"the answer is 45" don't get separate votes. Apply `.lower()` to extracted
answers (already shown in the examples above).

## Impact

- **No breaking changes** if the new default projection is adopted — callers
  that already pass a custom `consistency_space_projection_func` are unaffected.
- `response_counts` keys become clean, human-readable strings.
- The algorithm works correctly out-of-the-box for the most common use cases.
- Downstream consumers (demo UIs, IaaS) no longer need to post-process vote keys.

## Workaround (Current Demo Approach)

Until these changes land in `its_hub`, the demo works around the issue by:
1. Passing a custom projection function from `demo_ui/backend/inference.py` that
   extracts final answers for both math and general questions.
2. Post-processing `vote_counts` in `demo_ui/backend/traces.py` to unwrap tuples
   and re-aggregate by extracted answers.

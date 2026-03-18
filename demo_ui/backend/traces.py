"""
Trace building for ITS algorithm results.

Converts algorithm Result objects into serializable trace dicts
for the frontend to visualize.
"""

import logging
import re

import numpy as np

from its_hub.utils import extract_content_from_lm_response
from its_hub.algorithms.self_consistency import SelfConsistencyResult
from its_hub.algorithms.bon import BestOfNResult
from its_hub.algorithms.beam_search import BeamSearchResult
from its_hub.algorithms.particle_gibbs import ParticleFilteringResult, ParticleGibbsResult

from .models import (
    CandidateResponse,
    SelfConsistencyTrace,
    BestOfNTrace,
    BeamSearchTrace,
    ParticleFilteringTrace,
    ParticleGibbsTrace,
    ToolCall,
    ToolVotingTrace,
)

logger = logging.getLogger(__name__)


def _unwrap_vote_key(key) -> str:
    """Convert a vote key to a clean display string.

    Handles:
    - Tuples like ('45',) -> '45'
    - Multi-element tuples like ('algebra', '42') -> '42' (last non-None element)
    - Plain strings -> returned as-is
    - None -> '(no answer)'
    """
    if isinstance(key, tuple):
        # Filter out None values and take the last meaningful element
        non_none = [str(v) for v in key if v is not None]
        if non_none:
            return non_none[-1]
        return "(no answer)"
    if key is None:
        return "(no answer)"
    return str(key)


def _extract_answer_from_key(key_str: str) -> str:
    """Extract a concise answer from a vote key that may contain full response text.

    Uses the same patterns as the frontend extractFinalAnswer() and the backend
    extract_final_answer() to ensure consistency.
    """
    # Already short enough — no extraction needed
    if len(key_str) <= 80:
        return key_str

    # Try boxed answer
    boxed_idx = key_str.find('\\boxed{')
    if boxed_idx != -1:
        brace_count = 0
        start = boxed_idx + 7
        for i in range(start, len(key_str)):
            if key_str[i] == '{':
                brace_count += 1
            elif key_str[i] == '}':
                if brace_count == 0:
                    if i > start:
                        return key_str[start:i]
                    break
                brace_count -= 1

    # Try explicit answer patterns
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
        match = re.search(pattern, key_str, re.IGNORECASE | re.DOTALL)
        if match:
            answer = match.group(1).strip()
            if len(answer) < 200:
                return answer

    # Fallback: last non-empty paragraph
    paragraphs = [p.strip() for p in key_str.strip().split('\n\n') if p.strip()]
    if paragraphs:
        last = paragraphs[-1]
        if len(last) <= 200:
            return last

    # Nothing worked — truncate
    return key_str[:80]


def _clean_vote_counts(response_counts) -> dict[str, int]:
    """Clean and re-aggregate vote counts for frontend display.

    The self-consistency algorithm's response_counts may be keyed by:
    - Full response text (when no projection function extracts answers)
    - Python tuples like ('45',) from regex projection functions
    - Clean strings (ideal case)

    This function:
    1. Unwraps tuple keys to clean strings
    2. Extracts concise answers from long full-text keys
    3. Re-aggregates counts when multiple keys resolve to the same answer
    """
    cleaned: dict[str, int] = {}
    for key, count in response_counts.items():
        display_key = _unwrap_vote_key(key)
        display_key = _extract_answer_from_key(display_key)
        cleaned[display_key] = cleaned.get(display_key, 0) + count
    return cleaned


def _parse_tool_args(tool_args):
    """Parse tool arguments, handling JSON string encoding."""
    import json
    if isinstance(tool_args, str):
        try:
            return json.loads(tool_args)
        except json.JSONDecodeError:
            return {}
    return tool_args


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
                        tool_args = _parse_tool_args(tc.get("function", {}).get("arguments", {}))
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

            vote_counts = _clean_vote_counts(result.response_counts)

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

"""
Pydantic models for request/response validation.
"""

from typing import Literal, Optional
from pydantic import BaseModel, Field


class CompareRequest(BaseModel):
    """Request to compare baseline vs ITS."""
    question: str = Field(..., min_length=1, description="The question to answer")
    model_id: str = Field(..., description="Model identifier from the registry (for ITS)")
    algorithm: Literal[
        "best_of_n",
        "self_consistency",
        "beam_search",
        "particle_filtering",
        "entropic_particle_filtering",
        "particle_gibbs"
    ] = Field(
        ..., description="ITS algorithm to use"
    )
    budget: int = Field(..., ge=1, le=32, description="Computation budget (1-32)")
    use_case: Literal["improve_model", "match_frontier", "tool_consensus"] = Field(
        default="improve_model",
        description="Use case: improve_model (same model +ITS), match_frontier (small+ITS vs large), or tool_consensus (agent tool selection)"
    )
    frontier_model_id: Optional[str] = Field(
        default=None,
        description="Frontier model to compare against (only for match_frontier use case)"
    )
    # Tool calling parameters
    enable_tools: bool = Field(
        default=False,
        description="Enable tool calling for agent scenarios"
    )
    tool_vote: Optional[Literal["tool_name", "tool_args", "tool_hierarchical"]] = Field(
        default=None,
        description="Tool voting strategy for self-consistency algorithm"
    )
    exclude_args: Optional[list[str]] = Field(
        default=None,
        description="Argument names to exclude from tool voting (e.g., ['timestamp', 'id'])"
    )
    question_type: Optional[Literal["auto", "math", "tool_calling", "general"]] = Field(
        default="auto",
        description="Question type for configuration. 'auto' detects automatically."
    )


# --- Algorithm Trace Models ---

class ToolCall(BaseModel):
    """A tool call made by the model."""
    name: str = Field(..., description="Tool function name")
    arguments: dict = Field(..., description="Tool arguments")
    result: Optional[str] = Field(default=None, description="Tool execution result")


class CandidateResponse(BaseModel):
    """A single candidate response from an algorithm."""
    index: int = Field(..., description="Candidate index")
    content: str = Field(..., description="Response text content")
    is_selected: bool = Field(default=False, description="Whether this candidate was selected as the winner")
    tool_calls: Optional[list[ToolCall]] = Field(default=None, description="Tool calls made in this response")


class ToolVotingTrace(BaseModel):
    """Trace data for tool voting in self-consistency."""
    tool_vote_type: str = Field(..., description="Type of tool voting: tool_name, tool_args, or tool_hierarchical")
    tool_counts: dict[str, int] = Field(..., description="Distribution of tool selections")
    winning_tool: str = Field(..., description="Tool/parameters that won the vote")
    total_tool_calls: int = Field(..., description="Total number of tool calls made")


class SelfConsistencyTrace(BaseModel):
    """Trace data for Self-Consistency algorithm."""
    algorithm: Literal["self_consistency"] = "self_consistency"
    candidates: list[CandidateResponse]
    vote_counts: dict[str, int] = Field(..., description="Vote counts per unique answer")
    total_votes: int = Field(..., description="Total number of votes cast")
    tool_voting: Optional[ToolVotingTrace] = Field(default=None, description="Tool voting statistics if enabled")


class BestOfNTrace(BaseModel):
    """Trace data for Best-of-N algorithm."""
    algorithm: Literal["best_of_n"] = "best_of_n"
    candidates: list[CandidateResponse]
    scores: list[float] = Field(..., description="LLM judge scores for each candidate")
    max_score: float
    min_score: float


class BeamSearchTrace(BaseModel):
    """Trace data for Beam Search algorithm."""
    algorithm: Literal["beam_search"] = "beam_search"
    candidates: list[CandidateResponse]
    scores: list[float] = Field(..., description="PRM scores for each beam")
    steps_used: list[int] = Field(..., description="Number of steps used per beam")


class ParticleFilteringTrace(BaseModel):
    """Trace data for Particle Filtering algorithm."""
    algorithm: Literal["particle_filtering"] = "particle_filtering"
    candidates: list[CandidateResponse]
    log_weights: list[float] = Field(..., description="Log-weights for each particle")
    normalized_weights: list[float] = Field(..., description="Normalized weights (sum to 1)")
    steps_used: list[int] = Field(..., description="Number of steps used per particle")


class ParticleGibbsTrace(BaseModel):
    """Trace data for Particle Gibbs algorithm."""
    algorithm: Literal["particle_gibbs"] = "particle_gibbs"
    num_iterations: int = Field(..., description="Number of Gibbs iterations")
    iterations: list[ParticleFilteringTrace] = Field(..., description="Trace per iteration")


class ResultDetail(BaseModel):
    """Details of a single result (baseline or ITS).

    Metric accuracy by result type:
      Baseline — latency, input_tokens, output_tokens, and cost_usd are all
        actual values. Latency is wall-clock measured. Token counts come from
        the litellm response usage object. Cost is calculated from actual
        tokens and the model's configured pricing.
      ITS — latency is actual (wall-clock). Token counts (input_tokens,
        output_tokens) are *estimates* derived from the baseline token counts
        multiplied by the number of candidates, plus algorithm-specific
        overhead (judge calls for Best-of-N, PRM calls for process-based
        algorithms). cost_usd is calculated from these estimated tokens and
        is therefore also an estimate. The underlying ITS algorithm classes
        do not currently surface per-call token usage; to get actual ITS
        token counts, the algorithm internals would need to accumulate
        litellm usage data across all internal LLM calls.

    When tokens_estimated is True, the frontend prefixes token/cost values
    with "~" and labels them "(est.)" to communicate this to users.
    """
    answer: str = Field(..., description="The final answer text")
    latency_ms: int = Field(..., description="Latency in milliseconds (actual wall-clock)")
    log_preview: str = Field(default="", description="Placeholder for step logs (future)")
    model_size: Optional[str] = Field(default=None, description="Model size (e.g., 'Large', 'Small')")
    cost_usd: Optional[float] = Field(default=None, description="Cost in USD (estimated when tokens_estimated=True)")
    input_tokens: Optional[int] = Field(default=None, description="Number of input tokens (estimated for ITS results)")
    output_tokens: Optional[int] = Field(default=None, description="Number of output tokens (estimated for ITS results)")
    tokens_estimated: bool = Field(
        default=False,
        description="Whether token counts are estimates. True for ITS results "
        "(algorithms don't expose per-call usage), False for baseline results "
        "(actual counts from litellm)."
    )
    trace: Optional[dict] = Field(default=None, description="Algorithm trace data for visualization")
    tool_calls: Optional[list[ToolCall]] = Field(default=None, description="Tool calls made during execution")


class CompareResponse(BaseModel):
    """Response from comparison endpoint."""
    baseline: ResultDetail
    its: ResultDetail
    meta: dict = Field(..., description="Metadata about the comparison run")
    small_baseline: Optional[ResultDetail] = Field(
        default=None,
        description="Small model baseline (only for match_frontier use case)"
    )


class HealthResponse(BaseModel):
    """Health check response."""
    status: str
    message: str

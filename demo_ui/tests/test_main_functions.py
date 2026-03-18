"""Tests for pure functions in backend/inference.py."""

import pytest

from backend.inference import calculate_cost, detect_question_type


# ── detect_question_type ──────────────────────────────────────────────


class TestDetectQuestionType:
    """Tests for detect_question_type()."""

    # -- Math detection via regex patterns --

    def test_latex_dollar_sign(self):
        assert detect_question_type("Find $x^2 + y^2 = 10$") == "math"

    def test_latex_frac(self):
        assert detect_question_type("What is \\frac{3}{4}?") == "math"

    def test_latex_boxed(self):
        assert detect_question_type("The answer is \\boxed{42}") == "math"

    def test_single_weak_indicator_is_general(self):
        """A single weak indicator is not enough — need 2+ for math."""
        assert detect_question_type("What is the probability of rolling a 6?") == "general"
        assert detect_question_type("Find the value of sin(30)") == "general"
        assert detect_question_type("What is the 10th term of the sequence?") == "general"

    def test_two_weak_indicators_is_math(self):
        """Two or more weak indicators trigger math detection."""
        assert detect_question_type("What is the probability of the 5th term being prime?") == "math"
        assert detect_question_type("Solve for x: find the value of x + 3 = 10") == "math"
        assert detect_question_type("Find the value and evaluate the integral of f(x)") == "math"

    # -- Tool calling detection --

    def test_enable_tools_flag(self):
        assert detect_question_type("What's the weather?", enable_tools=True) == "tool_calling"

    def test_metadata_expected_tools(self):
        metadata = {"expected_tools": ["calculate", "web_search"]}
        assert detect_question_type("Any question", question_metadata=metadata) == "tool_calling"

    def test_metadata_source_tool_calling(self):
        metadata = {"source": "tool_calling"}
        assert detect_question_type("Any question", question_metadata=metadata) == "tool_calling"

    def test_metadata_takes_precedence_over_math(self):
        """Metadata should be checked before math patterns."""
        metadata = {"source": "tool_calling"}
        assert detect_question_type("Calculate 2+2", question_metadata=metadata) == "tool_calling"

    def test_enable_tools_takes_precedence_over_math(self):
        assert detect_question_type("Calculate 2+2", enable_tools=True) == "tool_calling"

    # -- General fallthrough --

    def test_general_question(self):
        assert detect_question_type("Tell me about the history of France") == "general"

    def test_general_no_math_patterns(self):
        assert detect_question_type("What is the capital of Germany?") == "general"

    def test_empty_string(self):
        assert detect_question_type("") == "general"

    # -- Edge cases --

    def test_none_metadata_ignored(self):
        assert detect_question_type("Hello", question_metadata=None) == "general"

    def test_empty_metadata_ignored(self):
        assert detect_question_type("Hello", question_metadata={}) == "general"


# ── calculate_cost ────────────────────────────────────────────────────


class TestCalculateCost:
    """Tests for calculate_cost()."""

    def test_zero_pricing(self, zero_cost_config):
        assert calculate_cost(zero_cost_config, 1000, 1000) == 0.0

    def test_input_cost_only(self, sample_model_config):
        config = {**sample_model_config, "output_cost_per_1m": 0.0}
        cost = calculate_cost(config, 1_000_000, 0)
        assert cost == 0.40

    def test_output_cost_only(self, sample_model_config):
        config = {**sample_model_config, "input_cost_per_1m": 0.0}
        cost = calculate_cost(config, 0, 1_000_000)
        assert cost == 1.60

    def test_combined_cost(self, sample_model_config):
        # 1M input tokens at $0.40/1M + 500K output tokens at $1.60/1M
        cost = calculate_cost(sample_model_config, 1_000_000, 500_000)
        assert cost == pytest.approx(1.20)

    def test_small_token_count(self, sample_model_config):
        # 100 input + 200 output
        cost = calculate_cost(sample_model_config, 100, 200)
        expected = (100 / 1_000_000) * 0.40 + (200 / 1_000_000) * 1.60
        assert cost == pytest.approx(round(expected, 6))

    def test_zero_tokens(self, sample_model_config):
        assert calculate_cost(sample_model_config, 0, 0) == 0.0

    def test_rounding_precision(self):
        config = {"input_cost_per_1m": 1.0, "output_cost_per_1m": 1.0}
        cost = calculate_cost(config, 1, 1)
        # Should be 0.000002, rounded to 6 decimal places
        assert cost == 0.000002

    def test_missing_pricing_keys_default_to_zero(self):
        config = {"model_name": "test"}
        assert calculate_cost(config, 1000, 1000) == 0.0

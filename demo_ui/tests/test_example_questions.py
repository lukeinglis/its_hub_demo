"""Tests for backend/example_questions.py."""

import pytest

from backend.example_questions import (
    get_all_questions,
    get_questions_by_algorithm,
    get_tool_calling_questions,
    get_tool_calling_questions_by_algorithm,
)

REQUIRED_QUESTION_KEYS = {"question", "expected_answer", "best_for", "category", "difficulty"}


class TestGetAllQuestions:
    """Tests for get_all_questions()."""

    def test_returns_nonempty_list(self):
        questions = get_all_questions()
        assert len(questions) >= 9

    def test_all_questions_have_required_keys(self):
        for q in get_all_questions():
            missing = REQUIRED_QUESTION_KEYS - set(q.keys())
            assert not missing, f"Question missing keys {missing}: {q.get('question', '?')[:50]}"

    def test_all_questions_have_nonempty_question_text(self):
        for q in get_all_questions():
            assert len(q["question"]) > 0

    def test_best_for_is_list(self):
        for q in get_all_questions():
            assert isinstance(q["best_for"], list), (
                f"best_for should be a list: {q.get('question', '?')[:50]}"
            )


class TestGetQuestionsByAlgorithm:
    """Tests for get_questions_by_algorithm()."""

    def test_returns_results_for_known_algorithm(self):
        questions = get_questions_by_algorithm("self_consistency")
        assert len(questions) > 0

    def test_returns_results_for_unknown_algorithm(self):
        """Unknown algorithms should still return all questions (tier 3)."""
        questions = get_questions_by_algorithm("nonexistent_algo")
        assert len(questions) > 0

    def test_limit_parameter(self):
        questions = get_questions_by_algorithm("self_consistency", limit=3)
        assert len(questions) <= 3

    def test_tiering_order(self):
        """Questions where algo is first in best_for should appear before others."""
        questions = get_questions_by_algorithm("self_consistency", limit=100)
        saw_non_tier1 = False
        for q in questions:
            is_tier1 = q["best_for"] and q["best_for"][0] == "self_consistency"
            if not is_tier1:
                saw_non_tier1 = True
            elif saw_non_tier1:
                pytest.fail("Tier 1 question appeared after non-tier-1 question")


class TestGetToolCallingQuestions:
    """Tests for get_tool_calling_questions()."""

    def test_returns_nonempty_list(self):
        questions = get_tool_calling_questions()
        assert len(questions) >= 8

    def test_all_have_expected_tools(self):
        for q in get_tool_calling_questions():
            assert "expected_tools" in q, f"Missing expected_tools: {q.get('question', '?')[:50]}"
            assert isinstance(q["expected_tools"], list)

    def test_all_have_source_tool_calling(self):
        for q in get_tool_calling_questions():
            assert q.get("source") == "tool_calling"


class TestGetToolCallingQuestionsByAlgorithm:
    """Tests for get_tool_calling_questions_by_algorithm()."""

    def test_returns_results(self):
        questions = get_tool_calling_questions_by_algorithm("self_consistency")
        assert len(questions) > 0

    def test_limit_parameter(self):
        questions = get_tool_calling_questions_by_algorithm("self_consistency", limit=2)
        assert len(questions) <= 2

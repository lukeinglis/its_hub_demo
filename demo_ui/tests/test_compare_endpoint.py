"""Integration tests for the POST /compare endpoint.

Uses FastAPI's TestClient with mocked LLM calls so no real API keys
or model endpoints are required.
"""

import pytest
from unittest.mock import patch, AsyncMock, MagicMock

from fastapi.testclient import TestClient

from backend.main import app


client = TestClient(app)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture(autouse=True)
def mock_openai_key(monkeypatch):
    """Ensure OPENAI_API_KEY is set for all tests."""
    monkeypatch.setenv("OPENAI_API_KEY", "sk-test-fake-key")


def _make_sc_result(responses, selected_index=0):
    """Build a mock SelfConsistencyResult."""
    from its_hub.algorithms.self_consistency import SelfConsistencyResult
    from its_hub.utils import extract_content_from_lm_response

    # Build response_counts from responses
    counts = {}
    for r in responses:
        content = extract_content_from_lm_response(r)
        key = (content,)
        counts[key] = counts.get(key, 0) + 1

    result = MagicMock(spec=SelfConsistencyResult)
    result.responses = responses
    result.selected_index = selected_index
    result.response_counts = counts
    result.the_one = responses[selected_index]
    return result


def _mock_lm_response(content="The answer is 42."):
    """Create a mock LLM response dict."""
    return {"role": "assistant", "content": content}


# ---------------------------------------------------------------------------
# Request validation tests
# ---------------------------------------------------------------------------

class TestCompareValidation:
    """Tests for request validation on /compare."""

    def test_empty_question_rejected(self):
        resp = client.post("/compare", json={
            "question": "",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
        })
        assert resp.status_code == 422

    def test_budget_too_low(self):
        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 0,
        })
        assert resp.status_code == 422

    def test_budget_too_high(self):
        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 100,
        })
        assert resp.status_code == 422

    def test_invalid_algorithm_rejected(self):
        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "not_an_algorithm",
            "budget": 4,
        })
        assert resp.status_code == 422

    def test_invalid_use_case_rejected(self):
        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
            "use_case": "invalid_use_case",
        })
        assert resp.status_code == 422

    def test_match_frontier_requires_frontier_model(self):
        """match_frontier without frontier_model_id should fail."""
        with patch("backend.main.create_language_model") as mock_create:
            mock_create.return_value = MagicMock()
            resp = client.post("/compare", json={
                "question": "What is 2+2?",
                "model_id": "gpt-4.1-nano",
                "algorithm": "self_consistency",
                "budget": 4,
                "use_case": "match_frontier",
            })
            assert resp.status_code == 400


# ---------------------------------------------------------------------------
# Use case routing tests (improve_model)
# ---------------------------------------------------------------------------

class TestCompareImproveModel:
    """Tests for the improve_model use case."""

    @patch("backend.main.evaluate_correctness", new_callable=AsyncMock)
    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_improve_model_success(self, mock_create_lm, mock_baseline, mock_its, mock_eval):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("Baseline answer", 100, 50, 80, None)
        mock_its.return_value = ("ITS answer", 200, 200, 320, {
            "algorithm": "self_consistency",
            "candidates": [],
            "vote_counts": {"ITS answer": 3, "other": 1},
            "total_votes": 4,
        }, None)
        mock_eval.return_value = (True, "exact_match")

        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
            "expected_answer": "4",
        })

        assert resp.status_code == 200
        data = resp.json()

        # Check top-level structure
        assert "baseline" in data
        assert "its" in data
        assert "meta" in data

        # Check baseline fields
        assert data["baseline"]["answer"] == "Baseline answer"
        assert data["baseline"]["latency_ms"] == 100
        assert data["baseline"]["tokens_estimated"] is False

        # Check ITS fields
        assert data["its"]["answer"] == "ITS answer"
        assert data["its"]["latency_ms"] == 200
        assert data["its"]["tokens_estimated"] is True
        assert data["its"]["trace"] is not None

        # Check meta
        assert data["meta"]["model_id"] == "gpt-4.1-nano"
        assert data["meta"]["algorithm"] == "self_consistency"
        assert data["meta"]["budget"] == 4
        assert data["meta"]["use_case"] == "improve_model"
        assert "run_id" in data["meta"]

        # No small_baseline for improve_model
        assert data.get("small_baseline") is None

    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_improve_model_no_expected_answer(self, mock_create_lm, mock_baseline, mock_its):
        """Without expected_answer, is_correct should be None."""
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("answer", 50, 10, 20, None)
        mock_its.return_value = ("answer", 80, 40, 80, None, None)

        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
        })

        assert resp.status_code == 200
        data = resp.json()
        assert data["baseline"]["is_correct"] is None
        assert data["its"]["is_correct"] is None

    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_invalid_model_id_returns_error(self, mock_create_lm, mock_baseline, mock_its):
        """Unknown model_id should return 400 with error message."""
        mock_create_lm.side_effect = ValueError("Model 'fake-model' not found in registry")

        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "fake-model",
            "algorithm": "self_consistency",
            "budget": 4,
        })

        assert resp.status_code == 400
        data = resp.json()
        assert "not found" in data["detail"].lower()


# ---------------------------------------------------------------------------
# Use case routing tests (match_frontier)
# ---------------------------------------------------------------------------

class TestCompareMatchFrontier:
    """Tests for the match_frontier use case."""

    @patch("backend.main.evaluate_correctness", new_callable=AsyncMock)
    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_match_frontier_success(self, mock_create_lm, mock_baseline, mock_its, mock_eval):
        mock_create_lm.return_value = MagicMock()
        # run_baseline is called twice: once for small baseline, once for frontier
        mock_baseline.side_effect = [
            ("Small baseline", 50, 10, 20, None),   # small model baseline
            ("Frontier answer", 150, 30, 60, None),  # frontier baseline
        ]
        mock_its.return_value = ("ITS answer", 200, 40, 80, None, None)
        mock_eval.return_value = (True, "exact_match")

        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "frontier_model_id": "gpt-4o",
            "algorithm": "self_consistency",
            "budget": 4,
            "use_case": "match_frontier",
            "expected_answer": "4",
        })

        assert resp.status_code == 200
        data = resp.json()

        # Should have all three result columns
        assert data["baseline"]["answer"] == "Frontier answer"
        assert data["its"]["answer"] == "ITS answer"
        assert data["small_baseline"] is not None
        assert data["small_baseline"]["answer"] == "Small baseline"

        # Meta should include frontier_model_id
        assert data["meta"]["frontier_model_id"] == "gpt-4o"
        assert data["meta"]["use_case"] == "match_frontier"


# ---------------------------------------------------------------------------
# Use case routing tests (tool_consensus)
# ---------------------------------------------------------------------------

class TestCompareToolConsensus:
    """Tests for the tool_consensus use case."""

    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_tool_consensus_success(self, mock_create_lm, mock_baseline, mock_its):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("Used web_search", 50, 10, 20, [
            {"name": "web_search", "arguments": {"query": "AAPL stock"}, "result": "..."}
        ])
        mock_its.return_value = ("Used get_data", 200, 40, 80, {
            "algorithm": "self_consistency",
            "candidates": [],
            "vote_counts": {"get_data": 6, "web_search": 2},
            "total_votes": 8,
        }, [
            {"name": "get_data", "arguments": {"data_type": "stock_price"}, "result": "..."}
        ])

        resp = client.post("/compare", json={
            "question": "What is the current stock price of AAPL?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 8,
            "use_case": "tool_consensus",
            "enable_tools": True,
            "tool_vote": "tool_name",
        })

        assert resp.status_code == 200
        data = resp.json()
        assert data["meta"]["use_case"] == "tool_consensus"


# ---------------------------------------------------------------------------
# Question type detection tests
# ---------------------------------------------------------------------------

class TestCompareQuestionType:
    """Tests that question type auto-detection routes correctly."""

    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_math_question_detected(self, mock_create_lm, mock_baseline, mock_its):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("42", 50, 10, 20, None)
        mock_its.return_value = ("42", 80, 40, 80, None, None)

        resp = client.post("/compare", json={
            "question": "Find the value of $\\frac{3}{4} + \\frac{1}{2}$.",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
        })

        assert resp.status_code == 200
        data = resp.json()
        assert data["meta"]["question_type"] == "math"

    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_general_question_detected(self, mock_create_lm, mock_baseline, mock_its):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("Blue sky", 50, 10, 20, None)
        mock_its.return_value = ("Blue sky", 80, 40, 80, None, None)

        resp = client.post("/compare", json={
            "question": "Why is the sky blue?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
        })

        assert resp.status_code == 200
        data = resp.json()
        assert data["meta"]["question_type"] == "general"

    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_explicit_question_type_overrides_auto(self, mock_create_lm, mock_baseline, mock_its):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("answer", 50, 10, 20, None)
        mock_its.return_value = ("answer", 80, 40, 80, None, None)

        resp = client.post("/compare", json={
            "question": "What is $x + 1$?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
            "question_type": "general",
        })

        assert resp.status_code == 200
        data = resp.json()
        assert data["meta"]["question_type"] == "general"


# ---------------------------------------------------------------------------
# Response structure tests
# ---------------------------------------------------------------------------

class TestCompareResponseStructure:
    """Tests that the response has the correct fields and types."""

    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_response_has_all_required_fields(self, mock_create_lm, mock_baseline, mock_its):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("answer", 100, 50, 80, None)
        mock_its.return_value = ("its answer", 200, 200, 320, None, None)

        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
        })

        assert resp.status_code == 200
        data = resp.json()

        # ResultDetail required fields
        for key in ["baseline", "its"]:
            result = data[key]
            assert "answer" in result
            assert "latency_ms" in result
            assert isinstance(result["latency_ms"], int)
            assert "tokens_estimated" in result
            assert isinstance(result["tokens_estimated"], bool)

        # Meta required fields
        meta = data["meta"]
        assert "model_id" in meta
        assert "algorithm" in meta
        assert "budget" in meta
        assert "run_id" in meta
        assert "use_case" in meta
        assert "question_type" in meta

    @patch("backend.main.evaluate_correctness", new_callable=AsyncMock)
    @patch("backend.main.run_its", new_callable=AsyncMock)
    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_cost_is_calculated(self, mock_create_lm, mock_baseline, mock_its, mock_eval):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.return_value = ("answer", 100, 500, 200, None)
        mock_its.return_value = ("its answer", 200, 2000, 800, None, None)
        mock_eval.return_value = (None, None)

        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
        })

        assert resp.status_code == 200
        data = resp.json()

        # gpt-4.1-nano has pricing configured, so costs should be present
        assert data["baseline"]["cost_usd"] is not None
        assert data["baseline"]["cost_usd"] > 0
        assert data["its"]["cost_usd"] is not None
        assert data["its"]["cost_usd"] > 0
        # ITS should cost more than baseline (more tokens)
        assert data["its"]["cost_usd"] > data["baseline"]["cost_usd"]


# ---------------------------------------------------------------------------
# Error handling tests
# ---------------------------------------------------------------------------

class TestCompareErrorHandling:
    """Tests for error handling in /compare."""

    @patch("backend.main.run_baseline", new_callable=AsyncMock)
    @patch("backend.main.create_language_model")
    def test_lm_failure_returns_500(self, mock_create_lm, mock_baseline):
        mock_create_lm.return_value = MagicMock()
        mock_baseline.side_effect = RuntimeError("Connection refused")

        resp = client.post("/compare", json={
            "question": "What is 2+2?",
            "model_id": "gpt-4.1-nano",
            "algorithm": "self_consistency",
            "budget": 4,
        })

        assert resp.status_code == 500
        data = resp.json()
        assert "detail" in data

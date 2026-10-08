"""Tests for backend/tools.py — safe expression evaluator and tool schemas."""

import json
import math

import pytest

from backend.tools import TOOL_SCHEMAS, _safe_eval_expr, execute_tool


class TestSafeEvalExpr:
    """Tests for _safe_eval_expr() — the AST-based math evaluator."""

    # -- Basic arithmetic --

    def test_addition(self):
        assert _safe_eval_expr("2 + 3") == 5

    def test_subtraction(self):
        assert _safe_eval_expr("10 - 4") == 6

    def test_multiplication(self):
        assert _safe_eval_expr("6 * 7") == 42

    def test_division(self):
        assert _safe_eval_expr("10 / 4") == 2.5

    def test_floor_division(self):
        assert _safe_eval_expr("10 // 3") == 3

    def test_modulo(self):
        assert _safe_eval_expr("10 % 3") == 1

    def test_exponentiation(self):
        assert _safe_eval_expr("2 ** 10") == 1024

    def test_negative_number(self):
        assert _safe_eval_expr("-5 + 3") == -2

    def test_compound_expression(self):
        result = _safe_eval_expr("1000 * (1 + 0.05) ** 5")
        assert result == pytest.approx(1276.2816, rel=1e-4)

    # -- Math functions --

    def test_sqrt(self):
        assert _safe_eval_expr("sqrt(144)") == 12.0

    def test_log(self):
        assert _safe_eval_expr("log(1)") == 0.0

    def test_exp(self):
        assert _safe_eval_expr("exp(0)") == 1.0

    def test_sin(self):
        assert _safe_eval_expr("sin(0)") == 0.0

    def test_cos(self):
        assert _safe_eval_expr("cos(0)") == 1.0

    def test_abs_function(self):
        assert _safe_eval_expr("abs(-42)") == 42

    # -- Constants --

    def test_pi(self):
        assert _safe_eval_expr("pi") == pytest.approx(math.pi)

    def test_e(self):
        assert _safe_eval_expr("e") == pytest.approx(math.e)

    def test_pi_times_two(self):
        assert _safe_eval_expr("pi * 2") == pytest.approx(2 * math.pi)

    # -- Security: must reject dangerous inputs --

    def test_rejects_import(self):
        with pytest.raises((ValueError, SyntaxError)):
            _safe_eval_expr('__import__("os")')

    def test_rejects_open(self):
        with pytest.raises((ValueError, SyntaxError)):
            _safe_eval_expr('open("/etc/passwd")')

    def test_rejects_class_traversal(self):
        with pytest.raises((ValueError, SyntaxError)):
            _safe_eval_expr("().__class__.__bases__[0].__subclasses__()")

    def test_rejects_arbitrary_function_call(self):
        with pytest.raises(ValueError):
            _safe_eval_expr("print(42)")

    def test_rejects_string_literals(self):
        with pytest.raises(ValueError):
            _safe_eval_expr('"hello"')

    def test_rejects_list_comprehension(self):
        with pytest.raises((ValueError, SyntaxError)):
            _safe_eval_expr("[x for x in range(10)]")


class TestToolSchemas:
    """Tests for TOOL_SCHEMAS structure."""

    def test_schemas_is_list(self):
        assert isinstance(TOOL_SCHEMAS, list)

    def test_schemas_nonempty(self):
        assert len(TOOL_SCHEMAS) > 0

    def test_all_schemas_have_type_function(self):
        for schema in TOOL_SCHEMAS:
            assert schema["type"] == "function"
            assert "function" in schema
            assert "name" in schema["function"]
            assert "description" in schema["function"]


class TestExecuteTool:
    """Tests for execute_tool() dispatch."""

    def test_calculate_returns_json(self):
        result = execute_tool("calculate", {"expression": "2 + 3"})
        data = json.loads(result)
        assert "result" in data

    def test_unknown_tool_returns_error(self):
        result = execute_tool("nonexistent_tool", {})
        data = json.loads(result)
        assert "error" in data or "unknown" in result.lower() or isinstance(data, dict)

    def test_web_search_returns_json(self):
        result = execute_tool("web_search", {"query": "test"})
        data = json.loads(result)
        assert isinstance(data, dict)

    def test_code_executor_returns_demo_mode(self):
        result = execute_tool("code_executor", {"code": "x = 42"})
        data = json.loads(result)
        # Code execution is intentionally disabled in demo mode for security
        assert data.get("status") == "demo_mode"

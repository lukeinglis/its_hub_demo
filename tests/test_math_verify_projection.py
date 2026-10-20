import pytest

from its_hub.core.algorithms.self_consistency import create_math_verify_projection


class TestMathVerifyProjectionEquivalence:
    def test_equivalent_fractions_map_to_same_key(self):
        pytest.importorskip("math_verify")
        proj = create_math_verify_projection()
        assert proj("1/2") == proj("0.5")

    def test_equivalent_integers(self):
        pytest.importorskip("math_verify")
        proj = create_math_verify_projection()
        assert proj("2+3") == proj("5")


class TestMathVerifyProjectionFallback:
    def test_malformed_latex_falls_back_to_strip(self):
        pytest.importorskip("math_verify")
        proj = create_math_verify_projection()
        result = proj("\\boxed{")
        assert result == "\\boxed{"

    def test_plain_text_falls_back_to_strip(self):
        pytest.importorskip("math_verify")
        proj = create_math_verify_projection()
        result = proj("  hello world  ")
        assert result == "hello world"


class TestMathVerifyProjectionNonStrInput:
    def test_none_input_returns_str_without_raising(self):
        pytest.importorskip("math_verify")
        proj = create_math_verify_projection()
        result = proj(None)
        assert isinstance(result, str)

    def test_int_input_returns_str_without_raising(self):
        pytest.importorskip("math_verify")
        proj = create_math_verify_projection()
        result = proj(42)
        assert isinstance(result, str)


class TestMathVerifyProjectionComplexityThreshold:
    def test_simplify_skipped_when_count_ops_exceeds_threshold(self, monkeypatch):
        pytest.importorskip("math_verify")
        import math_verify
        import sympy

        from its_hub.core.algorithms import self_consistency as sc_module

        x = sympy.symbols("x")
        complex_expr = sum(x**i for i in range(120))
        assert sympy.count_ops(complex_expr) > sc_module.MATH_VERIFY_SIMPLIFY_MAX_OPS

        monkeypatch.setattr(math_verify, "parse", lambda s: [complex_expr])

        simplify_calls = []
        original_simplify = sympy.simplify

        def spy_simplify(*args, **kwargs):
            simplify_calls.append(args)
            return original_simplify(*args, **kwargs)

        monkeypatch.setattr(sympy, "simplify", spy_simplify)

        proj = create_math_verify_projection()
        result = proj("ignored")

        assert simplify_calls == []
        assert isinstance(result, str)

    def test_simplify_runs_below_threshold(self, monkeypatch):
        pytest.importorskip("math_verify")
        import math_verify
        import sympy

        from its_hub.core.algorithms import self_consistency as sc_module

        x = sympy.symbols("x")
        simple_expr = x + 1
        assert sympy.count_ops(simple_expr) <= sc_module.MATH_VERIFY_SIMPLIFY_MAX_OPS

        monkeypatch.setattr(math_verify, "parse", lambda s: [simple_expr])

        simplify_calls = []
        original_simplify = sympy.simplify

        def spy_simplify(*args, **kwargs):
            simplify_calls.append(args)
            return original_simplify(*args, **kwargs)

        monkeypatch.setattr(sympy, "simplify", spy_simplify)

        proj = create_math_verify_projection()
        result = proj("ignored")

        assert len(simplify_calls) == 1
        assert isinstance(result, str)


class TestMathVerifyProjectionPathologicalInput:
    def test_deep_continued_fraction_does_not_hang(self):
        pytest.importorskip("math_verify")
        import time

        proj = create_math_verify_projection()
        depth = 25
        continued_fraction = "1/" + "(1+1/" * depth + "1" + ")" * depth

        start = time.monotonic()
        result = proj(continued_fraction)
        elapsed = time.monotonic() - start

        assert elapsed < 2.0
        assert isinstance(result, str)

    def test_deep_continued_fraction_skips_parse_entirely(self, monkeypatch):
        # math-verify's extraction regex has catastrophic backtracking on this
        # shape of input regardless of any sympy-side guard, so parse() itself
        # must never be called for a sufficiently nested expression.
        pytest.importorskip("math_verify")
        import math_verify

        parse_calls = []
        monkeypatch.setattr(
            math_verify, "parse", lambda s: parse_calls.append(s) or []
        )

        proj = create_math_verify_projection()
        depth = 25
        continued_fraction = "1/" + "(1+1/" * depth + "1" + ")" * depth
        result = proj(continued_fraction)

        assert parse_calls == []
        assert result == continued_fraction


class TestMaxParenDepth:
    def test_flat_expression_has_zero_depth(self):
        from its_hub.core.algorithms.self_consistency import _max_paren_depth

        assert _max_paren_depth("1 + 2 + 3") == 0

    def test_nested_parens_counted_correctly(self):
        from its_hub.core.algorithms.self_consistency import _max_paren_depth

        assert _max_paren_depth("(1+(2+(3)))") == 3

    def test_sequential_parens_not_summed(self):
        from its_hub.core.algorithms.self_consistency import _max_paren_depth

        assert _max_paren_depth("(1)(2)(3)") == 1


class TestMathVerifyProjectionImportError:
    def test_missing_math_verify_raises_with_install_hint(self, monkeypatch):
        import builtins

        real_import = builtins.__import__

        def mock_import(name, *args, **kwargs):
            if name == "math_verify":
                raise ImportError("No module named 'math_verify'")
            return real_import(name, *args, **kwargs)

        monkeypatch.setattr(builtins, "__import__", mock_import)

        with pytest.raises(ImportError, match="pip install"):
            create_math_verify_projection()

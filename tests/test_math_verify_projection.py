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

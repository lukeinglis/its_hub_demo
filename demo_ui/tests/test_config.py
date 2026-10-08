"""Tests for backend/config.py."""

import pytest

from backend.config import MODEL_REGISTRY, get_api_key, get_judge_config, get_model_config


class TestGetModelConfig:
    """Tests for get_model_config()."""

    def test_valid_model_returns_config(self):
        config = get_model_config("gpt-6-sol")
        assert config["model_name"] == "gpt-6-sol"
        assert config["provider"] == "openai"

    def test_valid_model_has_required_fields(self):
        config = get_model_config("gpt-5-mini")
        assert "base_url" in config
        assert "api_key_env_var" in config
        assert "model_name" in config

    def test_invalid_model_raises_valueerror(self):
        with pytest.raises(ValueError, match="not found in registry"):
            get_model_config("nonexistent-model-xyz")

    def test_all_registry_models_retrievable(self):
        for model_id in MODEL_REGISTRY:
            config = get_model_config(model_id)
            assert config is not None
            assert "model_name" in config


class TestGetApiKey:
    """Tests for get_api_key()."""

    def test_returns_key_when_set(self, mock_env_api_key):
        key = get_api_key("gpt-6-sol")
        assert key == "sk-test-key-12345"

    def test_raises_when_key_missing(self, monkeypatch):
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        with pytest.raises(ValueError, match="API key not found"):
            get_api_key("gpt-6-sol")

    def test_error_message_includes_env_var_name(self, monkeypatch):
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        with pytest.raises(ValueError, match="OPENAI_API_KEY"):
            get_api_key("gpt-6-sol")

    def test_maas_models_get_placeholder_key_without_env(self, monkeypatch):
        """MaaS routes often use unauthenticated/network-level auth."""
        monkeypatch.delenv("MAAS_API_KEY", raising=False)
        from backend import config
        monkeypatch.setitem(
            config.MODEL_REGISTRY["maas-custom"],
            "base_url",
            "https://maas.example.com/v1",
        )
        key = get_api_key("maas-custom")
        assert key == "maas"


class TestGetJudgeConfig:
    """Tests for get_judge_config()."""

    JUDGE_ENV_VARS = (
        "JUDGE_BASE_URL", "JUDGE_MODEL", "JUDGE_API_KEY",
        "MAAS_BASE_URL", "MAAS_MODEL_NAME", "MAAS_API_KEY",
        "VLLM_BASE_URL", "OPENAI_API_KEY",
    )

    def _clear_env(self, monkeypatch):
        for var in self.JUDGE_ENV_VARS:
            monkeypatch.delenv(var, raising=False)

    def test_defaults_to_openai(self, monkeypatch):
        self._clear_env(monkeypatch)
        cfg = get_judge_config()
        assert cfg["base_url"] == "https://api.openai.com/v1"
        assert cfg["model"] == "gpt-5-mini"
        assert cfg["litellm_model"] == "gpt-5-mini"

    def test_maas_route_used_by_default(self, monkeypatch):
        """With a MaaS route configured, judge/PRM run on it — no OpenAI key."""
        self._clear_env(monkeypatch)
        monkeypatch.setenv("MAAS_BASE_URL", "https://maas.example.com/v1")
        monkeypatch.setenv("MAAS_MODEL_NAME", "ibm-granite/granite-4-h-small")
        cfg = get_judge_config()
        assert cfg["base_url"] == "https://maas.example.com/v1"
        assert cfg["model"] == "ibm-granite/granite-4-h-small"
        assert cfg["litellm_model"] == "openai/ibm-granite/granite-4-h-small"
        assert cfg["api_key"] == "maas"

    def test_judge_overrides_win(self, monkeypatch):
        self._clear_env(monkeypatch)
        monkeypatch.setenv("JUDGE_BASE_URL", "https://judge.example.com/v1")
        monkeypatch.setenv("JUDGE_MODEL", "my-judge")
        monkeypatch.setenv("JUDGE_API_KEY", "sk-judge")
        cfg = get_judge_config()
        assert cfg["base_url"] == "https://judge.example.com/v1"
        assert cfg["model"] == "my-judge"
        assert cfg["litellm_model"] == "openai/my-judge"
        assert cfg["api_key"] == "sk-judge"

    def test_openai_fallback_key(self, monkeypatch):
        self._clear_env(monkeypatch)
        monkeypatch.setenv("OPENAI_API_KEY", "sk-test")
        cfg = get_judge_config()
        assert cfg["api_key"] == "sk-test"


class TestModelRegistry:
    """Tests for MODEL_REGISTRY structure and completeness."""

    def test_registry_has_models(self):
        assert len(MODEL_REGISTRY) > 0

    def test_all_models_have_api_key_env_var(self):
        for model_id, config in MODEL_REGISTRY.items():
            assert "api_key_env_var" in config, f"Model '{model_id}' missing 'api_key_env_var'"

    def test_all_models_have_description(self):
        for model_id, config in MODEL_REGISTRY.items():
            assert "description" in config, f"Model '{model_id}' missing 'description'"

    def test_known_providers(self):
        expected_providers = {"openai", "openrouter", "maas"}
        actual_providers = {c.get("provider") for c in MODEL_REGISTRY.values() if "provider" in c}
        assert actual_providers <= expected_providers, (
            f"Unexpected providers: {actual_providers - expected_providers}"
        )

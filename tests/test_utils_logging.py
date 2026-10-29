"""Tests for logging added to the remaining uninstrumented modules (H3)."""

import importlib
import logging

import pytest

from its_hub.core.utils import extract_content_from_lm_response


def test_extract_content_logs_warning_on_invalid_content_type(caplog):
    """The invalid-content-type branch emits a warning before raising."""
    with (
        caplog.at_level(logging.WARNING, logger="its_hub.core.utils"),
        pytest.raises(ValueError, match="Invalid content type"),
    ):
        extract_content_from_lm_response({"content": 123})

    assert "Invalid content type" in caplog.text


def test_extract_content_logs_warning_on_invalid_tool_call(caplog):
    """The invalid-tool-call branch emits a warning before raising."""
    message = {"content": "hi", "tool_calls": [{"not_function": True}]}

    with (
        caplog.at_level(logging.WARNING, logger="its_hub.core.utils"),
        pytest.raises(ValueError, match="Invalid tool call"),
    ):
        extract_content_from_lm_response(message)

    assert "Invalid tool call" in caplog.text


def test_extract_content_no_log_on_string_content_success(caplog):
    """The plain string success path does not emit any log record."""
    with caplog.at_level(logging.DEBUG, logger="its_hub.core.utils"):
        content = extract_content_from_lm_response({"content": "hello"})

    assert content == "hello"
    assert caplog.records == []


def test_extract_content_no_log_on_multimodal_content_success(caplog):
    """The list[dict] success path does not emit any log record."""
    message = {"content": [{"type": "text", "text": "hello"}]}

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.utils"):
        content = extract_content_from_lm_response(message)

    assert content == "hello"
    assert caplog.records == []


def test_extract_content_no_log_on_tool_calls_success(caplog):
    """The well-formed tool-calls success path does not emit any log record."""
    message = {
        "content": "hi",
        "tool_calls": [{"function": {"name": "foo", "arguments": {}}}],
    }

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.utils"):
        extract_content_from_lm_response(message)

    assert caplog.records == []


def test_module_declares_module_level_logger():
    """its_hub.core.utils exposes a stdlib logger bound to its __name__."""
    module = importlib.import_module("its_hub.core.utils")
    assert isinstance(module.logger, logging.Logger)
    assert module.logger.name == "its_hub.core.utils"

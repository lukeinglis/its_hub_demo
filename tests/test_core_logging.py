"""Tests for structured logging added to the orchestration/generation hot path (H3)."""

import asyncio
import logging

import pytest

from its_hub.api.algorithm import AbstractScalingAlgorithm
from its_hub.api.lm import AbstractLanguageModel
from its_hub.api.types import ChatMessage
from its_hub.core.lms.step_generation import StepGeneration
from its_hub.core.orchestrator import LMOrchestrator
from tests.mocks.language_models import SimpleMockLanguageModel


def test_orchestrator_logs_same_start_and_completed_messages(caplog):
    """The pre-existing start/completed debug lines keep their exact text."""
    lm = SimpleMockLanguageModel(["resp1", "resp2"])
    orchestrator = LMOrchestrator(max_concurrency=4)
    messages_lst = [
        [ChatMessage(role="user", content="prompt1")],
        [ChatMessage(role="user", content="prompt2")],
    ]

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.orchestrator"):
        asyncio.run(orchestrator.agenerate(lm, messages_lst))

    assert "LMOrchestrator: Processing batch of 2 messages" in caplog.text
    assert "LMOrchestrator: Completed batch generation" in caplog.text


def test_orchestrator_logs_new_batch_start_debug_record(caplog):
    """A new debug record fires at batch-generation start with counts and concurrency."""
    lm = SimpleMockLanguageModel(["resp1", "resp2"])
    orchestrator = LMOrchestrator(max_concurrency=4)
    messages_lst = [
        [ChatMessage(role="user", content="prompt1")],
        [ChatMessage(role="user", content="prompt2")],
    ]

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.orchestrator"):
        asyncio.run(orchestrator.agenerate(lm, messages_lst))

    assert "LMOrchestrator.agenerate start" in caplog.text
    assert "2 prompt(s)" in caplog.text
    assert "2 expected response(s)" in caplog.text
    assert "max_concurrency=4" in caplog.text


def test_orchestrator_no_log_for_empty_batch(caplog):
    """The empty-batch early return does not emit the batch-start logs."""
    lm = SimpleMockLanguageModel([])
    orchestrator = LMOrchestrator(max_concurrency=4)

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.orchestrator"):
        result = asyncio.run(orchestrator.agenerate(lm, []))

    assert result == []
    assert "LMOrchestrator.agenerate start" not in caplog.text


def test_orchestrator_results_unchanged_by_logging(caplog):
    """Generation results are identical regardless of logging level/side effects."""
    lm = SimpleMockLanguageModel(["resp1", "resp2"])
    orchestrator = LMOrchestrator(max_concurrency=4)
    messages_lst = [
        [ChatMessage(role="user", content="prompt1")],
        [ChatMessage(role="user", content="prompt2")],
    ]

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.orchestrator"):
        responses = asyncio.run(orchestrator.agenerate(lm, messages_lst))

    assert responses == [
        {"role": "assistant", "content": "resp1"},
        {"role": "assistant", "content": "resp2"},
    ]


def test_step_generation_logs_same_single_prompt_message(caplog):
    """The pre-existing single-prompt info log keeps its exact text/level."""
    lm = SimpleMockLanguageModel(["step1"])
    sg = StepGeneration(max_steps=3, step_token="\n")

    with caplog.at_level(logging.INFO, logger="its_hub.core.lms.step_generation"):
        sg.forward(lm, "prompt")

    assert "Generating step 1/3" in caplog.text


def test_step_generation_logs_same_batch_message(caplog):
    """The pre-existing batch info log keeps its exact text/level."""
    lm = SimpleMockLanguageModel(["step1", "step2"])
    sg = StepGeneration(max_steps=3, step_token="\n")

    with caplog.at_level(logging.INFO, logger="its_hub.core.lms.step_generation"):
        sg.forward(lm, ["prompt1", "prompt2"], steps_so_far=[[], []])

    assert "Generating steps (batch): [1, 1] / 3" in caplog.text


def test_step_generation_results_unchanged_by_logging(caplog):
    """Step generation results are unaffected by the logger refactor."""
    lm = SimpleMockLanguageModel(["step1"])
    sg = StepGeneration(max_steps=3, step_token="\n")

    with caplog.at_level(logging.INFO, logger="its_hub.core.lms.step_generation"):
        next_step, is_stopped = sg.forward(lm, "prompt")

    assert next_step == "step1"
    assert is_stopped is False


def test_agenerate_single_default_path_logs_debug_before_raising(caplog):
    """A subclass that forgets to override agenerate_single leaves a log trail."""

    class IncompleteLanguageModel(AbstractLanguageModel):
        async def agenerate(self, messages, stop=None, **kwargs):
            return {"role": "assistant", "content": "unused"}

    lm = IncompleteLanguageModel()

    with (
        caplog.at_level(logging.DEBUG, logger="its_hub.api.lm"),
        pytest.raises(NotImplementedError),
    ):
        asyncio.run(lm.agenerate_single([ChatMessage(role="user", content="hi")]))

    assert "IncompleteLanguageModel" in caplog.text
    assert "does not implement agenerate_single" in caplog.text


def test_infer_logs_debug_with_algorithm_name_and_budget(caplog):
    """The synchronous infer() wrapper logs the algorithm class name and budget."""

    class EchoAlgorithm(AbstractScalingAlgorithm):
        async def ainfer(
            self,
            lm,
            prompt_or_messages,
            budget,
            return_response_only=True,
            tools=None,
            tool_choice=None,
        ):
            return {"role": "assistant", "content": "ok"}

    algorithm = EchoAlgorithm()
    lm = SimpleMockLanguageModel(["unused"])

    with caplog.at_level(logging.DEBUG, logger="its_hub.api.algorithm"):
        result = algorithm.infer(lm, "prompt", budget=7)

    assert result == {"role": "assistant", "content": "ok"}
    assert "EchoAlgorithm.infer start" in caplog.text
    assert "budget=7" in caplog.text


@pytest.mark.parametrize(
    "module_path",
    [
        "its_hub.core.orchestrator",
        "its_hub.core.lms.step_generation",
        "its_hub.api.lm",
        "its_hub.api.algorithm",
    ],
)
def test_module_declares_module_level_logger(module_path):
    """Each target module exposes a stdlib logger bound to its __name__."""
    import importlib

    module = importlib.import_module(module_path)
    assert isinstance(module.logger, logging.Logger)
    assert module.logger.name == module_path

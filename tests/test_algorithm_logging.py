"""Tests for structured logging added to the core algorithm modules (H2)."""

import logging

import pytest

from its_hub import StepGeneration
from its_hub.core.algorithms.beam_search import BeamSearch
from its_hub.core.algorithms.bon import BestOfN
from its_hub.core.algorithms.particle_gibbs import ParticleGibbs
from tests.mocks.language_models import StepMockLanguageModel
from tests.mocks.reward_models import MockOutcomeRewardModel, MockProcessRewardModel


def test_base_infer_logs_entry_and_exit_with_algorithm_and_budget(caplog):
    """AbstractScalingAlgorithm.infer() (the shared base) logs start/complete."""
    mock_lm = StepMockLanguageModel(["resp1", "resp2"])
    mock_orm = MockOutcomeRewardModel([0.3, 0.7])
    bon = BestOfN(mock_orm)

    with caplog.at_level(logging.INFO, logger="its_hub.api.algorithm"):
        bon.infer(mock_lm, "prompt", budget=2, return_response_only=True)

    assert "infer start" in caplog.text
    assert "infer complete" in caplog.text
    assert "BestOfN" in caplog.text
    assert "budget=2" in caplog.text


def test_best_of_n_logs_selected_score(caplog):
    mock_lm = StepMockLanguageModel(["resp1", "resp2"])
    mock_orm = MockOutcomeRewardModel([0.3, 0.7])
    bon = BestOfN(mock_orm)

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.algorithms.bon"):
        bon.infer(mock_lm, "prompt", budget=2, return_response_only=True)

    assert "BestOfN.ainfer start" in caplog.text
    assert "budget=2" in caplog.text
    assert "BestOfN selected response" in caplog.text
    assert "selected_score=0.7000" in caplog.text


def test_best_of_n_logs_identical_candidates_path(caplog):
    """The early-return path for identical candidates also emits a summary log."""
    mock_lm = StepMockLanguageModel(["same_response"])
    mock_orm = MockOutcomeRewardModel([1.0])
    bon = BestOfN(mock_orm)

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.algorithms.bon"):
        bon.infer(mock_lm, "prompt", budget=2, return_response_only=True)

    assert "unique_candidates=1" in caplog.text


def test_beam_search_logs_step_scores_and_selection(caplog):
    mock_lm = StepMockLanguageModel(["good_step", "bad_step"])
    mock_prm = MockProcessRewardModel([0.9, 0.1])
    sg = StepGeneration(step_token="\n", max_steps=1)
    beam_search = BeamSearch(sg, mock_prm, beam_width=2)

    with caplog.at_level(logging.DEBUG, logger="its_hub.core.algorithms.beam_search"):
        beam_search.infer(mock_lm, "prompt", budget=4, return_response_only=True)

    assert "BeamSearch.ainfer start" in caplog.text
    assert "budget=4" in caplog.text
    assert "BeamSearch step 1 scores" in caplog.text
    assert "BeamSearch selected response" in caplog.text


def test_particle_gibbs_logs_resample_and_ess(caplog):
    mock_lm = StepMockLanguageModel(["stepA", "stepB"])
    mock_prm = MockProcessRewardModel([0.9, 0.1])
    sg = StepGeneration(step_token="\n", max_steps=1)
    pg = ParticleGibbs(sg, mock_prm, num_iterations=1)

    with caplog.at_level(
        logging.DEBUG, logger="its_hub.core.algorithms.particle_gibbs"
    ):
        pg.infer(mock_lm, "prompt", budget=2, return_response_only=True)

    assert "ParticleGibbs.ainfer start" in caplog.text
    assert "num_particles=2" in caplog.text
    assert "ParticleGibbs resample trigger" in caplog.text
    assert "ess=" in caplog.text
    assert "ParticleGibbs selected response" in caplog.text


@pytest.mark.parametrize(
    "module_path",
    [
        "its_hub.api.algorithm",
        "its_hub.core.algorithms.bon",
        "its_hub.core.algorithms.beam_search",
        "its_hub.core.algorithms.particle_gibbs",
    ],
)
def test_module_declares_module_level_logger(module_path):
    """Each target module exposes a stdlib logger bound to its __name__."""
    import importlib

    module = importlib.import_module(module_path)
    assert isinstance(module.logger, logging.Logger)
    assert module.logger.name == module_path

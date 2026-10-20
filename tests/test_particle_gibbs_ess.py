"""Tests for lazy ESS computation in ParticleGibbs.ainfer (H1 hardening)."""

import logging

from its_hub import StepGeneration
from its_hub.core.algorithms.particle_gibbs import ParticleGibbs
from tests.mocks.language_models import StepMockLanguageModel
from tests.mocks.reward_models import MockProcessRewardModel


def _make_particle_gibbs():
    mock_lm = StepMockLanguageModel(["stepA", "stepB"])
    mock_prm = MockProcessRewardModel([0.9, 0.1])
    sg = StepGeneration(step_token="\n", max_steps=1)
    pg = ParticleGibbs(sg, mock_prm, num_iterations=1)
    return pg, mock_lm


def test_ess_not_computed_when_debug_disabled(monkeypatch):
    """The eager ESS computation must be skipped when DEBUG logging is off."""
    pg, mock_lm = _make_particle_gibbs()

    called = []
    original_ess = pg._effective_sample_size

    def spy_ess(probabilities):
        called.append(probabilities)
        return original_ess(probabilities)

    monkeypatch.setattr(pg, "_effective_sample_size", spy_ess)

    logger = logging.getLogger("its_hub.core.algorithms.particle_gibbs")
    previous_level = logger.level
    logger.setLevel(logging.INFO)
    try:
        pg.infer(mock_lm, "prompt", budget=2, return_response_only=True)
    finally:
        logger.setLevel(previous_level)

    assert called == []


def test_ess_computed_and_logged_when_debug_enabled(monkeypatch, caplog):
    """When DEBUG is enabled, ESS is still computed and logged (no regression)."""
    pg, mock_lm = _make_particle_gibbs()

    called = []
    original_ess = pg._effective_sample_size

    def spy_ess(probabilities):
        called.append(probabilities)
        return original_ess(probabilities)

    monkeypatch.setattr(pg, "_effective_sample_size", spy_ess)

    with caplog.at_level(
        logging.DEBUG, logger="its_hub.core.algorithms.particle_gibbs"
    ):
        pg.infer(mock_lm, "prompt", budget=2, return_response_only=True)

    assert called != []
    assert "ess=" in caplog.text

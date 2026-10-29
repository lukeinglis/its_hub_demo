"""Benchmark test validating equivalence_matching's consensus-quality claim.

Compares SelfConsistency voting with and without math-verify equivalence
matching on a fixed set of responses: three equivalent forms of one-half
("1/2", "0.5", "2/4") plus one distinct outlier ("3"). This is a deterministic,
offline test that requires no network access or API keys.
"""

import pytest

from its_hub.core.algorithms.self_consistency import (
    SelfConsistency,
    SelfConsistencyResult,
)
from tests.mocks.language_models import StepMockLanguageModel

RESPONSES = ["1/2", "0.5", "2/4", "3"]


class TestEquivalenceMatchingConsensusQuality:
    def test_exact_match_projection_splits_the_vote(self):
        """Without equivalence matching, the three equivalent forms are treated
        as distinct strings, so no response achieves a majority."""
        mock_lm = StepMockLanguageModel(RESPONSES)

        sc = SelfConsistency(equivalence_matching=False)
        result = sc.infer(mock_lm, "what is one half?", budget=4, return_response_only=False)

        assert isinstance(result, SelfConsistencyResult)
        assert max(result.response_counts.values()) == 1

    def test_math_verify_projection_unifies_equivalent_forms(self):
        """With equivalence matching, the three equivalent forms of one-half
        collapse into a single winning key with a 3-vs-1 majority over the
        distinct outlier."""
        pytest.importorskip("math_verify")
        mock_lm = StepMockLanguageModel(RESPONSES)

        sc = SelfConsistency(equivalence_matching=True)
        result = sc.infer(mock_lm, "what is one half?", budget=4, return_response_only=False)

        assert isinstance(result, SelfConsistencyResult)
        assert max(result.response_counts.values()) == 3

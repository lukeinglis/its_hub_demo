"""
LLM-based Process Reward Model for step-by-step reasoning.

This provides a simplified process reward model that uses an LLM judge
to score partial/intermediate responses, enabling algorithms like BeamSearch
and ParticleFiltering without requiring a separate vLLM server.
"""

import logging
from typing import List

from its_hub import AbstractProcessRewardModel
from its_hub.api import ChatMessage, ChatMessages

logger = logging.getLogger(__name__)


class LLMProcessRewardModel(AbstractProcessRewardModel):
    """
    LLM-based process reward model that evaluates partial responses.

    Uses an LLM to judge the quality of intermediate reasoning steps,
    providing scores between 0 and 1 for each partial response.
    """

    def __init__(
        self,
        model: str = "gpt-5-mini",
        api_key: str | None = None,
        base_url: str | None = None,
        temperature: float = 0.3,
    ):
        """
        Initialize LLM-based process reward model.

        Args:
            model: LiteLLM model name (e.g., "gpt-5-mini")
            api_key: API key for the model provider
            base_url: Base URL for custom endpoints
            temperature: Temperature for judge generation (lower = more deterministic)
        """
        import litellm

        self.model = model
        self.api_key = api_key
        self.base_url = base_url
        self.temperature = temperature

    def _create_judge_prompt(self, question: str, partial_response: str) -> str:
        """Create a prompt for judging a partial response."""
        return f"""You are evaluating a partial response to a question. Score the quality and correctness of the reasoning so far.

Question: {question}

Partial Response:
{partial_response}

Evaluate this partial response on:
1. Logical correctness of reasoning steps taken so far
2. Progress toward solving the problem
3. Clarity and coherence of explanation
4. Absence of errors or incorrect assumptions

Provide a score between 0.0 and 1.0, where:
- 1.0 = Excellent reasoning, clearly on the right track
- 0.7-0.9 = Good reasoning with minor issues
- 0.4-0.6 = Mediocre, some correct elements but also problems
- 0.0-0.3 = Poor reasoning, significant errors or off-track

Respond with ONLY a single number between 0.0 and 1.0, nothing else."""

    async def ascore(
        self,
        prompt_or_messages: str | list[ChatMessage] | ChatMessages,
        steps: str | list[str],
    ) -> float | list[float]:
        """Score response(s) asynchronously."""
        import litellm

        chat_messages = ChatMessages.from_prompt_or_messages(prompt_or_messages)
        question = chat_messages.to_prompt()

        is_single_response = isinstance(steps, str)
        responses = (
            [steps] if is_single_response else steps
        )

        # Score each response
        scores = []
        for response in responses:
            judge_prompt = self._create_judge_prompt(question, response)

            try:
                # Call LLM to get score
                result = await litellm.acompletion(
                    model=self.model,
                    messages=[{"role": "user", "content": judge_prompt}],
                    temperature=self.temperature,
                    max_tokens=10,
                    api_key=self.api_key,
                    base_url=self.base_url,
                )

                # Extract score from response
                score_text = result.choices[0].message.content.strip()
                try:
                    score = float(score_text)
                    # Clamp to [0, 1]
                    score = max(0.0, min(1.0, score))
                except ValueError:
                    logger.warning(f"Failed to parse score from: {score_text}, using 0.5")
                    score = 0.5

                scores.append(score)
                logger.debug(f"Scored partial response (length {len(response)}): {score:.3f}")

            except Exception as e:
                logger.error(f"Error scoring response: {e}")
                scores.append(0.5)  # Default to neutral score on error

        return scores[0] if is_single_response else scores

    def score(
        self,
        prompt_or_messages: str | list[ChatMessage] | ChatMessages,
        steps: str | list[str],
    ) -> float | list[float]:
        """Score response(s) synchronously."""
        import asyncio
        import concurrent.futures

        coro = self.ascore(prompt_or_messages, steps)
        try:
            asyncio.get_running_loop()
            with concurrent.futures.ThreadPoolExecutor() as executor:
                return executor.submit(asyncio.run, coro).result()
        except RuntimeError:
            return asyncio.run(coro)

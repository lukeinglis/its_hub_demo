"""
LiteLLM-based language model wrapper.

Re-implemented locally: the upstream its_hub library removed
`LiteLLMLanguageModel` in the api/core restructure (BREAKING_CHANGES.md).
This module keeps LiteLLM-routed providers working, e.g. Vertex AI Model
Garden models using the `vertex_ai/` prefix with Google ADC auth.
"""

import asyncio
import logging
from typing import Dict, List

import litellm

from its_hub import AbstractLanguageModel
from its_hub.api import ChatMessage

logger = logging.getLogger(__name__)


class LiteLLMLanguageModel(AbstractLanguageModel):
    """Language model routed through LiteLLM.

    Supports any provider prefix LiteLLM understands (e.g. `vertex_ai/`),
    with provider-specific credentials passed as extra kwargs
    (e.g. `vertex_project=`, `vertex_location=`).
    """

    def __init__(
        self,
        model_name: str,
        api_key: str | None = None,
        system_prompt: str | None = None,
        temperature: float = 0.7,
        max_concurrency: int = -1,
        **kwargs,
    ):
        self.model_name = model_name
        self.api_key = api_key
        self.system_prompt = system_prompt
        self.temperature = temperature
        self.max_concurrency = max_concurrency
        # LiteLLM provider-specific credentials (vertex_project, vertex_location, ...)
        self.extra_kwargs = kwargs

    def _prepare_request_data(
        self,
        messages: List[ChatMessage],
        stop: str | None = None,
        temperature: float | None = None,
        tools: List[dict] | None = None,
        tool_choice: str | dict | None = None,
    ) -> dict:
        # Convert dict messages to Message objects if needed
        messages = [
            msg if isinstance(msg, ChatMessage) else ChatMessage(**msg)
            for msg in messages
        ]

        if self.system_prompt:
            messages = [
                ChatMessage(role="system", content=self.system_prompt),
                *messages,
            ]

        request_data = {
            "model": self.model_name,
            "messages": [msg.to_dict() for msg in messages],
            "temperature": temperature if temperature is not None else self.temperature,
        }

        if self.api_key:
            request_data["api_key"] = self.api_key
        if stop is not None:
            request_data["stop"] = stop
        if tools is not None:
            request_data["tools"] = tools
        if tool_choice is not None:
            request_data["tool_choice"] = tool_choice

        request_data.update(self.extra_kwargs)
        return request_data

    async def agenerate_single(
        self,
        messages: List[ChatMessage],
        stop: str | None = None,
        **kwargs,
    ) -> Dict:
        """Generate a single response via LiteLLM.

        Required by the new its_hub orchestrator-based algorithms.
        """
        request_data = self._prepare_request_data(
            messages,
            stop=stop,
            temperature=kwargs.pop("temperature", None),
            tools=kwargs.pop("tools", None),
            tool_choice=kwargs.pop("tool_choice", None),
        )
        response = await litellm.acompletion(**request_data)
        # Return the full message dict to preserve tool calls
        return response.choices[0].message.model_dump()

    async def _generate(
        self,
        messages_lst: List[List[ChatMessage]],
        stop: str | None = None,
        temperature: float | None = None,
        tools: List[dict] | None = None,
        tool_choice: str | dict | None = None,
    ) -> List[Dict]:
        # limit concurrency using a semaphore
        semaphore = asyncio.Semaphore(
            len(messages_lst) if self.max_concurrency == -1 else self.max_concurrency
        )

        async def fetch_response(messages: List[ChatMessage]) -> Dict:
            async with semaphore:
                request_data = self._prepare_request_data(
                    messages,
                    stop=stop,
                    temperature=temperature,
                    tools=tools,
                    tool_choice=tool_choice,
                )
                response = await litellm.acompletion(**request_data)
                # Return the full message dict to preserve tool calls
                return response.choices[0].message.model_dump()

        return list(await asyncio.gather(*(fetch_response(m) for m in messages_lst)))

    async def agenerate(
        self,
        messages_or_messages_lst: List[ChatMessage] | List[List[ChatMessage]],
        stop: str | None = None,
        temperature: float | List[float] | None = None,
        **kwargs,
    ) -> Dict | List[Dict]:
        is_single = not isinstance(messages_or_messages_lst[0], list)
        messages_lst = (
            [messages_or_messages_lst] if is_single else messages_or_messages_lst
        )

        responses = await self._generate(messages_lst, stop=stop, **kwargs)

        return responses[0] if is_single else responses

    def generate(
        self,
        messages_or_messages_lst: List[ChatMessage] | List[List[ChatMessage]],
        stop: str | None = None,
        temperature: float | List[float] | None = None,
        **kwargs,
    ) -> Dict | List[Dict]:
        """Generate response(s) synchronously."""
        import concurrent.futures

        coro = self.agenerate(messages_or_messages_lst, stop=stop, **kwargs)
        try:
            asyncio.get_running_loop()
            with concurrent.futures.ThreadPoolExecutor() as executor:
                return executor.submit(asyncio.run, coro).result()
        except RuntimeError:
            return asyncio.run(coro)

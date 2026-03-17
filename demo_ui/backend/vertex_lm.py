"""
Vertex AI Language Model wrappers for Claude and Gemini models.

Uses the official google-cloud-aiplatform and anthropic[vertex] packages.
"""

import asyncio
import concurrent.futures
import logging
from abc import abstractmethod
from typing import List, Dict

from anthropic import AnthropicVertex
from vertexai.generative_models import GenerativeModel
import vertexai
from its_hub.base import AbstractLanguageModel
from its_hub.types import ChatMessage

logger = logging.getLogger(__name__)


class _VertexAIBaseModel(AbstractLanguageModel):
    """Shared base for Vertex AI model wrappers."""

    def __init__(
        self,
        project_id: str,
        location: str,
        model_name: str,
        max_tokens: int = 4096,
        temperature: float = 1.0,
    ):
        self.project_id = project_id
        self.location = location
        self.model_name = model_name
        self.max_tokens = max_tokens
        self.temperature = temperature

    @abstractmethod
    async def _generate_single(
        self, messages: List[ChatMessage], max_tok: int, temp: float, stop: str | None
    ) -> Dict:
        """Generate a single response via the provider-specific API."""

    async def agenerate(
        self,
        messages_or_messages_lst: List[ChatMessage] | List[List[ChatMessage]],
        stop: str | None = None,
        max_tokens: int | None = None,
        temperature: float | List[float] | None = None,
        include_stop_str_in_output: bool | None = None,
        tools: List[dict] | None = None,
        tool_choice: str | dict | None = None,
    ) -> Dict | List[Dict]:
        """Generate response(s) asynchronously."""
        is_single = not isinstance(messages_or_messages_lst[0], list)
        messages_lst = (
            [messages_or_messages_lst] if is_single else messages_or_messages_lst
        )

        max_tok = max_tokens if max_tokens is not None else self.max_tokens
        temp = temperature if temperature is not None else self.temperature
        temps = temp if isinstance(temp, list) else [temp] * len(messages_lst)

        responses = await asyncio.gather(
            *(self._generate_single(msgs, max_tok, t, stop)
              for msgs, t in zip(messages_lst, temps))
        )

        return responses[0] if is_single else responses

    def generate(
        self,
        messages_or_messages_lst: List[ChatMessage] | List[List[ChatMessage]],
        stop: str | None = None,
        max_tokens: int | None = None,
        temperature: float | List[float] | None = None,
        include_stop_str_in_output: bool | None = None,
        tools: List[dict] | None = None,
        tool_choice: str | dict | None = None,
    ) -> Dict | List[Dict]:
        """Generate response(s) synchronously."""
        coro = self.agenerate(
            messages_or_messages_lst,
            stop,
            max_tokens,
            temperature,
            include_stop_str_in_output,
            tools,
            tool_choice,
        )
        try:
            asyncio.get_running_loop()
            with concurrent.futures.ThreadPoolExecutor() as executor:
                return executor.submit(asyncio.run, coro).result()
        except RuntimeError:
            return asyncio.run(coro)

    def evaluate(self, prompt: str, generation: str) -> List[float]:
        raise NotImplementedError(
            f"Evaluate not implemented for {self.__class__.__name__}"
        )


class VertexAIClaudeModel(_VertexAIBaseModel):
    """Wrapper for Claude models on Vertex AI using the Anthropic Vertex SDK."""

    def __init__(
        self,
        project_id: str,
        location: str,
        model_name: str,
        max_tokens: int = 4096,
        temperature: float = 1.0,
    ):
        super().__init__(project_id, location, model_name, max_tokens, temperature)
        self.client = AnthropicVertex(project_id=project_id, region=location)
        logger.info(
            f"Initialized VertexAI Claude model: {model_name} "
            f"(project: {project_id}, location: {location})"
        )

    def _convert_messages(self, messages: List[ChatMessage]) -> List[Dict]:
        """Convert ChatMessage list to Anthropic API format."""
        anthropic_messages = []
        for msg in messages:
            role = "user" if msg.role == "system" else msg.role
            anthropic_messages.append({
                "role": role,
                "content": msg.extract_text_content(),
            })
        return anthropic_messages

    async def _generate_single(
        self, messages: List[ChatMessage], max_tok: int, temp: float, stop: str | None
    ) -> Dict:
        anthropic_messages = self._convert_messages(messages)

        def sync_call():
            kwargs = {
                "model": self.model_name,
                "max_tokens": max_tok,
                "temperature": temp,
                "messages": anthropic_messages,
            }
            if stop and stop.strip():
                kwargs["stop_sequences"] = [stop]
            return self.client.messages.create(**kwargs)

        response = await asyncio.to_thread(sync_call)
        result = {"role": "assistant", "content": response.content[0].text}
        # Capture token usage from the Anthropic response
        if hasattr(response, 'usage') and response.usage:
            result["usage"] = {
                "input_tokens": getattr(response.usage, 'input_tokens', 0),
                "output_tokens": getattr(response.usage, 'output_tokens', 0),
            }
        return result


class VertexAIGeminiModel(_VertexAIBaseModel):
    """Wrapper for Gemini models on Vertex AI."""

    def __init__(
        self,
        project_id: str,
        location: str,
        model_name: str,
        max_tokens: int = 4096,
        temperature: float = 1.0,
    ):
        super().__init__(project_id, location, model_name, max_tokens, temperature)
        vertexai.init(project=project_id, location=location)
        self.model = GenerativeModel(model_name)
        logger.info(
            f"Initialized VertexAI Gemini model: {model_name} "
            f"(project: {project_id}, location: {location})"
        )

    def _convert_messages(self, messages: List[ChatMessage]) -> str:
        """Convert ChatMessage list to prompt string for Gemini."""
        prompt_parts = []
        for msg in messages:
            content = msg.extract_text_content()
            if msg.role == "system":
                prompt_parts.append(f"System: {content}")
            elif msg.role == "user":
                prompt_parts.append(f"User: {content}")
            elif msg.role == "assistant":
                prompt_parts.append(f"Assistant: {content}")
            else:
                prompt_parts.append(content)
        return "\n\n".join(prompt_parts)

    async def _generate_single(
        self, messages: List[ChatMessage], max_tok: int, temp: float, stop: str | None
    ) -> Dict:
        prompt = self._convert_messages(messages)

        def sync_call():
            gen_config = {
                "max_output_tokens": max_tok,
                "temperature": temp,
            }
            if stop and stop.strip():
                gen_config["stop_sequences"] = [stop]
            return self.model.generate_content(prompt, generation_config=gen_config)

        response = await asyncio.to_thread(sync_call)
        return {"role": "assistant", "content": response.text}

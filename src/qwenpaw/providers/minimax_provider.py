# -*- coding: utf-8 -*-
"""MiniMax provider using the standard text generation endpoint."""

from __future__ import annotations

from datetime import datetime
from typing import Any

import openai
from agentscope.credential._openai import OpenAICredential
from agentscope.model import OpenAIChatModel
from openai import AsyncStream
from openai.types.chat import ChatCompletion, ChatCompletionChunk

from .capping_formatter import _CappingOpenAIFormatter
from .openai_chat_model_compat import OpenAIChatModelCompat
from .openai_provider import OpenAIProvider

MINIMAX_CHAT_COMPLETIONS_PATH = "/text/chatcompletion_v2"


class MiniMaxChatModel(OpenAIChatModelCompat):
    """Call MiniMax's standard endpoint with OpenAI-shaped payloads."""

    async def _call_api(
        self,
        model_name: str,
        messages: Any,
        tools: list[dict] | None = None,
        tool_choice: Any | None = None,
        **generate_kwargs: Any,
    ) -> Any:
        merged = {**self._extra_generate_kwargs, **generate_kwargs}
        self._consume_disable_thinking(merged)

        extra_headers = dict(self._default_headers or {})
        extra_headers.update(merged.pop("extra_headers", None) or {})
        client = openai.AsyncClient(
            api_key=self.credential.api_key.get_secret_value(),
            organization=self.credential.organization,
            base_url=self.credential.base_url,
            default_headers=extra_headers or None,
            **self.client_kwargs,
        )

        body: dict[str, Any] = {
            "model": model_name,
            "messages": await self.formatter.format(messages),
            "stream": self.stream,
        }
        if self.parameters.max_tokens is not None:
            body["max_completion_tokens"] = self.parameters.max_tokens
        if self.parameters.temperature is not None:
            body["temperature"] = self.parameters.temperature
        if self.parameters.top_p is not None:
            body["top_p"] = self.parameters.top_p

        extra_body = dict(self.extra_body or {})
        extra_body.update(merged.pop("extra_body", None) or {})
        body.update(extra_body)
        body.update(merged)

        formatted_tools, formatted_choice = self._format_tools(
            tools,
            tool_choice,
        )
        if formatted_tools:
            body["tools"] = formatted_tools
            if not self.parameters.parallel_tool_calls:
                body["parallel_tool_calls"] = False
        if formatted_choice is not None:
            body["tool_choice"] = formatted_choice
        if self.stream:
            body["stream_options"] = {"include_usage": True}

        start_datetime = datetime.now()
        if self.stream:
            response = await client.post(
                MINIMAX_CHAT_COMPLETIONS_PATH,
                body=body,
                cast_to=ChatCompletionChunk,
                stream=True,
                stream_cls=AsyncStream[ChatCompletionChunk],
            )
            return self._parse_stream_response(start_datetime, response)

        response = await client.post(
            MINIMAX_CHAT_COMPLETIONS_PATH,
            body=body,
            cast_to=ChatCompletion,
        )
        return self._parse_completion_response(start_datetime, response)


class MiniMaxProvider(OpenAIProvider):
    """Provider for MiniMax's standard ``chatcompletion_v2`` API."""

    def get_chat_model_instance(self, model_id: str) -> MiniMaxChatModel:
        credential = OpenAICredential(
            id=f"qwenpaw-{self.id}",
            api_key=self.api_key,
            base_url=self.base_url,
        )
        generate_kwargs = self.get_effective_generate_kwargs(model_id)
        max_tokens = generate_kwargs.pop("max_tokens", None)
        if "max_completion_tokens" in generate_kwargs:
            max_tokens = None

        parameters = OpenAIChatModel.Parameters(
            max_tokens=max_tokens,
            temperature=generate_kwargs.pop("temperature", None),
            top_p=generate_kwargs.pop("top_p", None),
        )
        return MiniMaxChatModel(
            credential=credential,
            model=model_id,
            parameters=parameters,
            stream=True,
            default_headers=self._build_default_headers() or None,
            extra_generate_kwargs=generate_kwargs or None,
            context_size=self._get_context_size(model_id),
            formatter=_CappingOpenAIFormatter(
                max_bytes=self.max_inline_media_bytes,
                relay_reasoning_content=self._get_relay_reasoning(model_id),
            ),
        )

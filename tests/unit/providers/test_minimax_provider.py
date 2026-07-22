# -*- coding: utf-8 -*-
# pylint: disable=protected-access
from __future__ import annotations

from agentscope.message import Msg, TextBlock

import qwenpaw.providers.minimax_provider as minimax_provider_module
from qwenpaw.providers.minimax_provider import (
    MINIMAX_CHAT_COMPLETIONS_PATH,
    MiniMaxProvider,
)
from qwenpaw.providers.provider import ModelInfo


async def test_minimax_m3_uses_standard_endpoint_and_token_parameter(
    monkeypatch,
) -> None:
    captured: dict = {}

    class FakeClient:
        def __init__(self, **kwargs) -> None:
            captured["client"] = kwargs

        async def post(self, path, **kwargs):
            captured["path"] = path
            captured["request"] = kwargs
            return object()

    monkeypatch.setattr(
        minimax_provider_module.openai,
        "AsyncClient",
        FakeClient,
    )
    provider = MiniMaxProvider(
        id="minimax-cn",
        name="MiniMax (China)",
        base_url="https://api.minimaxi.com/v1",
        api_key="test-key",
        models=[
            ModelInfo(
                id="MiniMax-M3",
                name="MiniMax M3",
                max_tokens=16384,
            ),
        ],
    )
    model = provider.get_chat_model_instance("MiniMax-M3")

    await model._call_api(
        "MiniMax-M3",
        [
            Msg(
                name="user",
                role="user",
                content=[TextBlock(text="hello")],
            ),
        ],
    )

    assert captured["path"] == MINIMAX_CHAT_COMPLETIONS_PATH
    body = captured["request"]["body"]
    assert body["model"] == "MiniMax-M3"
    assert body["max_completion_tokens"] == 16384
    assert "max_tokens" not in body

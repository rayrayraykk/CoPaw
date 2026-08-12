# -*- coding: utf-8 -*-
"""Tests for ACP multimodal prompt conversion."""

# pylint: disable=protected-access

from __future__ import annotations

from acp import text_block
from acp.schema import (
    AudioContentBlock,
    BlobResourceContents,
    EmbeddedResourceContentBlock,
    ImageContentBlock,
    ResourceContentBlock,
    TextResourceContents,
)

from qwenpaw.agents.acp.server import QwenPawACPAgent, _prompt_content
from qwenpaw.providers.capping_formatter import _CappingOpenAIFormatter
from qwenpaw.runtime.message_convert import _request_input_to_msgs
from qwenpaw.schemas import Message


class _FakeConn:
    async def session_update(self, session_id, update):  # noqa: ANN001
        del session_id, update


class _FakeWorkspace:
    def __init__(self) -> None:
        self.requests = []

    async def stream_query(self, request):  # noqa: ANN001
        self.requests.append(request)
        for event in ():
            yield event


class _TestACPAgent(QwenPawACPAgent):
    def __init__(self, workspace: _FakeWorkspace) -> None:
        super().__init__(agent_id="default")
        self._fake_workspace = workspace

    async def _ensure_workspace(self):
        return self._fake_workspace


def test_prompt_content_preserves_acp_multimodal_blocks():
    content = _prompt_content(
        [
            text_block("inspect these inputs"),
            ImageContentBlock(
                type="image",
                data="aW1hZ2U=",
                mimeType="image/png",
            ),
            AudioContentBlock(
                type="audio",
                data="YXVkaW8=",
                mimeType="audio/wav",
            ),
            ResourceContentBlock(
                type="resource_link",
                name="remote.webp",
                uri="https://example.test/remote.webp",
                mimeType="image/webp",
            ),
            EmbeddedResourceContentBlock(
                type="resource",
                resource=TextResourceContents(
                    text="embedded notes",
                    uri="file:///task/notes.txt",
                    mimeType="text/plain",
                ),
            ),
            EmbeddedResourceContentBlock(
                type="resource",
                resource=BlobResourceContents(
                    blob="cGRm",
                    uri="file:///task/input.pdf",
                    mimeType="application/pdf",
                ),
            ),
        ],
    )

    assert content == [
        {"type": "text", "text": "inspect these inputs"},
        {
            "type": "image",
            "mime_type": "image/png",
            "data": "aW1hZ2U=",
        },
        {
            "type": "audio",
            "mime_type": "audio/wav",
            "data": "YXVkaW8=",
        },
        {
            "type": "image",
            "mime_type": "image/webp",
            "image_url": "https://example.test/remote.webp",
        },
        {"type": "text", "text": "embedded notes"},
        {
            "type": "file",
            "mime_type": "application/pdf",
            "data": "cGRm",
        },
    ]


async def test_image_only_prompt_reaches_workspace():
    workspace = _FakeWorkspace()
    agent = _TestACPAgent(workspace)
    agent.on_connect(_FakeConn())
    session = await agent.new_session(cwd="/task")

    response = await agent.prompt(
        prompt=[
            ImageContentBlock(
                type="image",
                data="aW1hZ2U=",
                mimeType="image/png",
            ),
        ],
        session_id=session.session_id,
    )

    assert response.stop_reason == "end_turn"
    assert len(workspace.requests) == 1
    image = workspace.requests[0].input[0].content[0]
    assert image.type == "image"
    assert image.mime_type == "image/png"
    assert image.data == "aW1hZ2U="


def test_request_conversion_keeps_base64_media_type():
    messages = _request_input_to_msgs(
        [
            Message(
                role="user",
                content=[
                    {
                        "type": "image",
                        "data": "aW1hZ2U=",
                        "mime_type": "image/png",
                    },
                    {
                        "type": "audio",
                        "data": "YXVkaW8=",
                        "mime_type": "audio/wav",
                    },
                ],
            ),
        ],
    )

    assert len(messages) == 1
    image_source = messages[0].content[0].source
    audio_source = messages[0].content[1].source
    assert image_source.type == "base64"
    assert image_source.data == "aW1hZ2U="
    assert image_source.media_type == "image/png"
    assert audio_source.type == "base64"
    assert audio_source.data == "YXVkaW8="
    assert audio_source.media_type == "audio/wav"

    formatter = _CappingOpenAIFormatter()
    assert formatter._format_image_source(image_source) == {
        "type": "image_url",
        "image_url": {
            "url": "data:image/png;base64,aW1hZ2U=",
        },
    }
    assert formatter._format_audio_source(audio_source) == {
        "type": "input_audio",
        "input_audio": {
            "data": "YXVkaW8=",
            "format": "wav",
        },
    }

# -*- coding: utf-8 -*-
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest
from fastapi import HTTPException

from qwenpaw.app.driver_config_service import DriverConfigService
from qwenpaw.drivers.capabilities import DriverRuntimeInfo
from qwenpaw.drivers.contracts import CredentialRef, DriverCard
from qwenpaw.drivers.storage import card_path, dump_card


def _card(
    name: str = "demo",
    *,
    protocol: str = "mcp",
) -> DriverCard:
    return DriverCard(
        name=name,
        protocol=protocol,
        endpoint={"transport": "stdio", "command": "demo"},
        credential=CredentialRef(kind="none"),
    )


def _service(tmp_path: Path, manager=None) -> DriverConfigService:
    workspace = SimpleNamespace(
        workspace_dir=tmp_path,
        driver_manager=manager,
    )
    return DriverConfigService(workspace)


@pytest.mark.asyncio
async def test_save_card_removes_same_name_cards_from_other_protocols(
    tmp_path: Path,
) -> None:
    stale = card_path(tmp_path / "drivers", "demo", protocol="acp")
    dump_card(_card(protocol="acp"), stale)

    saved = await _service(tmp_path).save_card(
        _card(protocol="mcp"),
        reload_driver=False,
    )

    assert saved == tmp_path / "drivers" / "mcp" / "demo.yaml"
    assert saved.is_file()
    assert not stale.exists()


class FailingDeleteManager:
    async def delete_driver(self, name: str) -> None:
        assert name == "demo"
        raise RuntimeError("driver still shutting down")


@pytest.mark.asyncio
async def test_delete_driver_best_effort_falls_back_to_storage_cleanup(
    tmp_path: Path,
) -> None:
    mcp_path = card_path(tmp_path / "drivers", "demo", protocol="mcp")
    acp_path = card_path(tmp_path / "drivers", "demo", protocol="acp")
    dump_card(_card(protocol="mcp"), mcp_path)
    dump_card(_card(protocol="acp"), acp_path)

    await _service(tmp_path, FailingDeleteManager()).delete_driver_best_effort(
        "demo",
    )

    assert not mcp_path.exists()
    assert not acp_path.exists()


class FakeCapabilityManager:
    def __init__(self, status: str) -> None:
        self.status = status
        self.list_capability_calls: list[dict] = []

    async def list_drivers(
        self,
        *,
        protocol: str | None = None,
    ) -> list[DriverRuntimeInfo]:
        assert protocol == "mcp"
        return [
            DriverRuntimeInfo(
                name="demo",
                protocol="mcp",
                enabled=True,
                status=self.status,
            ),
        ]

    async def list_driver_capabilities(
        self,
        name: str,
        *,
        kind: str,
        request_context: dict[str, str],
    ) -> list[str]:
        self.list_capability_calls.append(
            {
                "name": name,
                "kind": kind,
                "request_context": dict(request_context),
            },
        )
        return ["echo"]


@pytest.mark.asyncio
async def test_list_driver_capabilities_requires_active_driver(
    tmp_path: Path,
) -> None:
    manager = FakeCapabilityManager(status="inactive")

    with pytest.raises(HTTPException) as exc_info:
        await _service(tmp_path, manager).list_driver_capabilities(
            "demo",
            protocol="mcp",
            kind="tool",
            request_context={"channel": "console"},
        )

    assert exc_info.value.status_code == 503
    assert "status=inactive" in str(exc_info.value.detail)
    assert not manager.list_capability_calls


@pytest.mark.asyncio
async def test_list_driver_capabilities_forwards_request_context(
    tmp_path: Path,
) -> None:
    manager = FakeCapabilityManager(status="active")

    capabilities = await _service(tmp_path, manager).list_driver_capabilities(
        "demo",
        protocol="mcp",
        kind="tool",
        request_context={"channel": "console"},
    )

    assert capabilities == ["echo"]
    assert manager.list_capability_calls == [
        {
            "name": "demo",
            "kind": "tool",
            "request_context": {"channel": "console"},
        },
    ]

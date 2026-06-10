# -*- coding: utf-8 -*-
from pathlib import Path

import pytest

from qwenpaw.drivers.errors import DriverCardError
from qwenpaw.drivers.contracts import (
    CredentialRef,
    DriverCard,
    DriverPolicy,
    PolicyCondition,
    PolicyPrincipal,
    PolicyRule,
    PolicyTarget,
    TimeRange,
)
from qwenpaw.drivers.storage import (
    card_path,
    dump_card,
    list_card_paths,
    load_card,
)


def _card(name: str = "demo", enabled: bool = True) -> DriverCard:
    return DriverCard(
        name=name,
        protocol="mcp",
        endpoint={"transport": "stdio", "command": "demo"},
        credential=CredentialRef(kind="none"),
        enabled=enabled,
        policy=[
            PolicyRule(
                subject="user:*",
                effect="allow",
                condition=PolicyCondition(
                    time_range=TimeRange(after="09:00", before="18:00"),
                ),
            ),
        ],
    )


def test_card_yaml_round_trip(tmp_path: Path) -> None:
    path = tmp_path / "demo.yaml"
    dump_card(_card(), path)

    loaded = load_card(path)

    assert loaded.name == "demo"
    assert loaded.protocol == "mcp"
    assert loaded.endpoint["command"] == "demo"
    assert loaded.policy[0].condition is not None
    assert loaded.policy[0].condition.time_range is not None
    assert loaded.policy[0].condition.time_range.after == "09:00"
    assert loaded.policy.default_effect == "deny"
    assert loaded.policy[0].target.kind == "*"
    assert loaded.policy[0].target.name == "*"


def test_card_yaml_round_trip_new_policy_shape(tmp_path: Path) -> None:
    path = tmp_path / "demo.yaml"
    card = _card()
    card.policy = DriverPolicy(
        default_effect="ask",
        rules=[
            PolicyRule(
                subject="*",
                effect="allow",
                target=PolicyTarget(kind="tool", name="echo"),
                principal=PolicyPrincipal(
                    source_type="channel",
                    source_value="console",
                    subject_type="user",
                    subject_value="alice",
                ),
            ),
        ],
    )

    dump_card(card, path)
    loaded = load_card(path)

    assert loaded.policy.default_effect == "ask"
    assert loaded.policy.rules[0].target.kind == "tool"
    assert loaded.policy.rules[0].target.name == "echo"
    assert loaded.policy.rules[0].principal.source_type == "channel"
    assert loaded.policy.rules[0].principal.source_value == "console"
    assert loaded.policy.rules[0].principal.subject_type == "user"
    assert loaded.policy.rules[0].principal.subject_value == "alice"


def test_load_legacy_policy_list_as_wildcard_target(tmp_path: Path) -> None:
    path = tmp_path / "legacy.yaml"
    path.write_text(
        """
name: demo
protocol: mcp
endpoint:
  transport: stdio
  command: demo
credential:
  kind: none
policy:
  - subject: "*"
    effect: allow
""",
        encoding="utf-8",
    )

    loaded = load_card(path)

    assert loaded.policy.default_effect == "deny"
    assert loaded.policy.rules[0].target.kind == "*"
    assert loaded.policy.rules[0].target.name == "*"


def test_load_card_missing_required_field_raises(tmp_path: Path) -> None:
    path = tmp_path / "bad.yaml"
    path.write_text("name: demo\nprotocol: mcp\n", encoding="utf-8")

    with pytest.raises(DriverCardError, match="missing required fields"):
        load_card(path)


def test_disabled_card_round_trip(tmp_path: Path) -> None:
    path = tmp_path / "disabled.yaml"
    dump_card(_card(enabled=False), path)

    assert load_card(path).enabled is False


def test_list_card_paths_filters_and_sorts(tmp_path: Path) -> None:
    (tmp_path / "b.yaml").write_text("{}", encoding="utf-8")
    (tmp_path / "a.yml").write_text("{}", encoding="utf-8")
    (tmp_path / "mcp").mkdir()
    (tmp_path / "mcp" / "c.yaml").write_text("{}", encoding="utf-8")
    (tmp_path / ".hidden").mkdir()
    (tmp_path / ".hidden" / "ignored.yaml").write_text("{}", encoding="utf-8")
    (tmp_path / ".legacy_mcp_migration_report.yaml").write_text(
        "{}",
        encoding="utf-8",
    )
    (tmp_path / "ignored.txt").write_text("{}", encoding="utf-8")

    assert [
        path.relative_to(tmp_path).as_posix()
        for path in list_card_paths(tmp_path)
    ] == [
        "mcp/c.yaml",
    ]


def test_protocol_card_path_and_lookup_ignore_flat_files(
    tmp_path: Path,
) -> None:
    flat = tmp_path / "demo.yaml"
    flat.write_text("{}", encoding="utf-8")

    assert card_path(tmp_path, "demo", protocol="mcp") == (
        tmp_path / "mcp" / "demo.yaml"
    )
    assert list_card_paths(tmp_path) == []

    nested = tmp_path / "mcp" / "demo.yaml"
    nested.parent.mkdir()
    nested.write_text("{}", encoding="utf-8")

    assert list_card_paths(tmp_path) == [nested]


def test_atomic_write_failure_keeps_existing_file(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    path = tmp_path / "demo.yaml"
    dump_card(_card(name="old"), path)
    before = path.read_text(encoding="utf-8")

    def fail_safe_dump(*args, **kwargs):
        del args
        del kwargs
        raise RuntimeError("boom")

    with monkeypatch.context() as patcher:
        patcher.setattr(
            "qwenpaw.drivers.storage.yaml.safe_dump",
            fail_safe_dump,
        )
        with pytest.raises(DriverCardError, match="Failed to write"):
            dump_card(_card(name="new"), path)

    assert path.read_text(encoding="utf-8") == before
    assert load_card(path).name == "old"

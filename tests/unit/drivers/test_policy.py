from datetime import datetime, timezone

from qwenpaw.drivers.contracts import (
    DriverPolicy,
    PolicyCondition,
    PolicyPrincipal,
    PolicyRule,
    PolicyTarget,
    TimeRange,
)
from qwenpaw.drivers.policy import (
    PolicyContext,
    condition_satisfied,
    evaluate_policy,
    specificity,
    subject_matches,
)


def _ctx(subject: str = "user:alice", hour: int = 10) -> PolicyContext:
    return PolicyContext(
        subject=subject,
        driver_name="demo",
        protocol="mcp",
        target=PolicyTarget(kind="tool", name="echo"),
        now=datetime(2026, 6, 1, hour, 0, tzinfo=timezone.utc),
    )


def test_empty_rules_deny() -> None:
    assert evaluate_policy([], _ctx()) == "deny"


def test_driver_policy_uses_default_effect_when_no_rule_matches() -> None:
    policy = DriverPolicy(default_effect="ask", rules=[])

    assert evaluate_policy(policy, _ctx()) == "ask"


def test_subject_matching_and_specificity() -> None:
    assert subject_matches("user:*", "user:alice")
    assert subject_matches("app:*", "app:finance")
    assert subject_matches("*", "service:cron")
    assert not subject_matches("user:*", "service:cron")
    assert specificity("user:alice") == 2
    assert specificity("user:*") == 1
    assert specificity("app:*") == 1
    assert specificity("*") == 0


def test_exact_subject_wins_over_wildcards() -> None:
    rules = [
        PolicyRule(subject="*", effect="deny"),
        PolicyRule(subject="user:*", effect="ask"),
        PolicyRule(subject="user:alice", effect="allow"),
    ]

    assert evaluate_policy(rules, _ctx()) == "allow"


def test_context_subject_set_matches_secondary_app_subject() -> None:
    policy = DriverPolicy(
        default_effect="deny",
        rules=[
            PolicyRule(
                subject="app:finance",
                effect="allow",
                target=PolicyTarget(kind="tool", name="echo"),
            )
        ],
    )
    context = PolicyContext(
        subject="user:alice",
        subjects=("app:finance", "channel:console"),
        driver_name="demo",
        protocol="mcp",
        target=PolicyTarget(kind="tool", name="echo"),
    )

    assert evaluate_policy(policy, context) == "allow"


def test_structured_principal_matches_source_and_user_with_and_semantics() -> None:
    policy = DriverPolicy(
        default_effect="deny",
        rules=[
            PolicyRule(
                subject="*",
                effect="allow",
                target=PolicyTarget(kind="tool", name="echo"),
                principal=PolicyPrincipal(
                    source_type="channel",
                    source_value="dingtalk",
                    subject_type="user",
                    subject_value="alice",
                ),
            )
        ],
    )

    assert (
        evaluate_policy(
            policy,
            PolicyContext(
                subject="user:alice",
                subjects=("channel:dingtalk",),
                driver_name="demo",
                protocol="mcp",
                target=PolicyTarget(kind="tool", name="echo"),
                request_context={
                    "channel": "dingtalk",
                    "user_id": "alice",
                },
            ),
        )
        == "allow"
    )
    assert (
        evaluate_policy(
            policy,
            PolicyContext(
                subject="user:alice",
                subjects=("channel:dingtalk",),
                driver_name="demo",
                protocol="mcp",
                target=PolicyTarget(kind="tool", name="echo"),
                request_context={
                    "channel": "dingtalk",
                    "user_id": "bob",
                },
            ),
        )
        == "deny"
    )


def test_structured_principal_specificity_wins_within_same_source() -> None:
    policy = DriverPolicy(
        default_effect="deny",
        rules=[
            PolicyRule(
                subject="*",
                effect="deny",
                target=PolicyTarget(kind="tool", name="echo"),
                principal=PolicyPrincipal(
                    source_type="channel",
                    source_value="dingtalk",
                    subject_type="all",
                    subject_value="",
                ),
            ),
            PolicyRule(
                subject="*",
                effect="ask",
                target=PolicyTarget(kind="tool", name="echo"),
                principal=PolicyPrincipal(
                    source_type="channel",
                    source_value="dingtalk",
                    subject_type="user",
                    subject_value="alice",
                ),
            ),
        ],
    )

    assert (
        evaluate_policy(
            policy,
            PolicyContext(
                subject="user:alice",
                driver_name="demo",
                protocol="mcp",
                target=PolicyTarget(kind="tool", name="echo"),
                request_context={
                    "channel": "dingtalk",
                    "user_id": "alice",
                },
            ),
        )
        == "ask"
    )


def test_strictness_wins_at_same_specificity() -> None:
    rules = [
        PolicyRule(subject="user:*", effect="allow"),
        PolicyRule(subject="user:*", effect="ask"),
        PolicyRule(subject="user:*", effect="deny"),
    ]

    assert evaluate_policy(rules, _ctx()) == "deny"


def test_exact_tool_target_wins_over_global_target() -> None:
    policy = DriverPolicy(
        default_effect="deny",
        rules=[
            PolicyRule(
                subject="user:alice",
                effect="deny",
                target=PolicyTarget(kind="tool", name="*"),
            ),
            PolicyRule(
                subject="*",
                effect="allow",
                target=PolicyTarget(kind="tool", name="echo"),
            ),
        ],
    )

    assert evaluate_policy(policy, _ctx()) == "allow"


def test_structured_console_tool_rule_beats_tool_default_rule() -> None:
    policy = DriverPolicy(
        default_effect="allow",
        rules=[
            PolicyRule(
                subject="*",
                effect="deny",
                target=PolicyTarget(kind="tool", name="add_note"),
            ),
            PolicyRule(
                subject="*",
                effect="ask",
                target=PolicyTarget(kind="tool", name="add_note"),
                principal=PolicyPrincipal(
                    source_type="channel",
                    source_value="console",
                    subject_type="all",
                    subject_value="",
                ),
            ),
        ],
    )

    target = PolicyTarget(kind="tool", name="add_note")
    context = PolicyContext(
        subject="user:default",
        driver_name="qwenpaw_test_notes",
        protocol="mcp",
        target=target,
        request_context={
            "channel": "console",
            "user_id": "default",
        },
    )

    assert evaluate_policy(policy, context) == "ask"
    assert (
        evaluate_policy(
            policy,
            PolicyContext(
                subject="user:default",
                driver_name="qwenpaw_test_notes",
                protocol="mcp",
                target=target,
                request_context={
                    "channel": "",
                    "user_id": "default",
                },
            ),
        )
        == "deny"
    )


def test_new_tool_without_override_uses_default_effect() -> None:
    policy = DriverPolicy(
        default_effect="ask",
        rules=[
            PolicyRule(
                subject="*",
                effect="allow",
                target=PolicyTarget(kind="tool", name="echo"),
            )
        ],
    )

    assert (
        evaluate_policy(
            policy,
            PolicyContext(
                subject="user:alice",
                driver_name="demo",
                protocol="mcp",
                target=PolicyTarget(kind="tool", name="new_tool"),
            ),
        )
        == "ask"
    )


def test_time_range_weekday_satisfied() -> None:
    condition = PolicyCondition(
        time_range=TimeRange(
            after="09:00",
            before="18:00",
            weekdays=[0],
        ),
    )

    assert condition_satisfied(condition, _ctx(hour=10))
    assert not condition_satisfied(condition, _ctx(hour=20))


def test_condition_fallback_to_other_rule() -> None:
    rules = [
        PolicyRule(
            subject="user:*",
            effect="deny",
            condition=PolicyCondition(
                time_range=TimeRange(after="20:00", before="21:00"),
            ),
        ),
        PolicyRule(subject="user:*", effect="allow"),
    ]

    assert evaluate_policy(rules, _ctx(hour=10)) == "allow"


def test_malformed_subject_does_not_crash() -> None:
    assert (
        evaluate_policy(
            [PolicyRule(subject="", effect="allow")],
            _ctx(subject=""),
        )
        == "deny"
    )

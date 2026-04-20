# -*- coding: utf-8 -*-
"""Approval level strategies for Tool Guard.

Defines approval levels and policy logic for determining whether a tool call
should bypass guardrails, require approval, or auto-approve based on risk.

Approval Levels:
- STRICT: All tool calls require manual approval (regardless of guardrails)
- AUTO: Current mode - tools blocked by guardrails require approval
- SMART: Low-risk blocked tools auto-approve, high-risk require approval
- OFF: Bypass Tool Guard entirely (no security checks)

Default Levels:
- Interactive agents: AUTO (current behavior, backward compatible)
- Background tasks (CLI, cron, agent tools): SMART (user requested)
"""

from __future__ import annotations

import logging
from enum import Enum
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ...security.tool_guard.models import ToolGuardResult

logger = logging.getLogger(__name__)


class ApprovalLevel(str, Enum):
    """Approval level for Tool Guard.

    Attributes:
        STRICT: All tool calls require manual approval
            (even if guardrails pass)
        AUTO: Tool calls blocked by guardrails require approval
            (current default for interactive agents)
        SMART: Auto-approve low-risk blocks, require approval for
            high-risk (recommended for background tasks)
        OFF: Bypass Tool Guard entirely (no security checks)
    """

    STRICT = "STRICT"
    AUTO = "AUTO"
    SMART = "SMART"
    OFF = "OFF"


# Default for interactive agents (backward compatible)
DEFAULT_APPROVAL_LEVEL = ApprovalLevel.AUTO

# Default for background tasks (CLI, cron, agent tools)
DEFAULT_BACKGROUND_APPROVAL_LEVEL = ApprovalLevel.SMART


def parse_approval_level(value: str | None) -> ApprovalLevel:
    """Parse approval level from string.

    Args:
        value: Approval level string (case-insensitive)

    Returns:
        ApprovalLevel enum value

    Raises:
        ValueError: If value is invalid
    """
    if not value:
        return DEFAULT_APPROVAL_LEVEL

    normalized = value.strip().upper()

    # Empty string after strip
    if not normalized:
        return DEFAULT_APPROVAL_LEVEL

    try:
        return ApprovalLevel(normalized)
    except ValueError as exc:
        valid_levels = ", ".join(level.value for level in ApprovalLevel)
        raise ValueError(
            f"Invalid approval level: {value}. "
            f"Valid levels: {valid_levels}",
        ) from exc


def should_require_approval(
    level: ApprovalLevel,
    guard_result: "ToolGuardResult | None",
) -> bool:
    """Determine if tool call requires manual approval.

    Args:
        level: Approval level
        guard_result: Tool Guard result (None if guardrails bypassed/disabled)

    Returns:
        True if requires manual approval, False if can proceed

    Logic:
        OFF: Never require approval (guardrails disabled)
        STRICT: Always require approval
        AUTO: Require approval if guardrails blocked (findings_count > 0)
        SMART: Require approval only for high-risk blocks
    """
    if level == ApprovalLevel.OFF:
        # OFF: Tool Guard disabled, never require approval
        return False

    if level == ApprovalLevel.STRICT:
        # STRICT: All tool calls require approval
        return True

    # No guard result means guardrails passed or were bypassed
    if guard_result is None or guard_result.findings_count == 0:
        return False

    if level == ApprovalLevel.AUTO:
        # AUTO: Require approval if guardrails found any issues
        return True

    if level == ApprovalLevel.SMART:
        # SMART: Only require approval for high-risk blocks
        return _is_high_risk(guard_result)

    # Unknown level: fail safe to requiring approval
    logger.warning(
        f"Unknown approval level: {level}, falling back to STRICT",
    )
    return True


def _is_high_risk(guard_result: "ToolGuardResult") -> bool:
    """Check if guard result is high-risk (requires approval in SMART mode).

    High-risk criteria:
    - findings_count >= 3 (multiple security issues)
    - OR any CRITICAL severity finding

    Low/Medium risk (auto-approved in SMART mode):
    - findings_count <= 2 AND no CRITICAL findings

    Args:
        guard_result: Tool Guard result

    Returns:
        True if high-risk (requires approval), False if low-risk
    """
    from ...security.tool_guard.models import GuardSeverity

    # Check for CRITICAL severity findings
    for finding in guard_result.findings:
        if finding.severity == GuardSeverity.CRITICAL:
            logger.info(
                "High-risk detected: CRITICAL severity finding",
            )
            return True

    # Check findings count (HIGH risk threshold: >= 3)
    if guard_result.findings_count >= 3:
        logger.info(
            f"High-risk detected: multiple findings "
            f"(count={guard_result.findings_count})",
        )
        return True

    # Low/Medium risk: can auto-approve in SMART mode
    logger.info(
        f"Low-risk detected: auto-approvable in SMART mode "
        f"(count={guard_result.findings_count})",
    )
    return False


def get_effective_approval_level(
    request_level: str | None,
    agent_level: str | None,
    global_level: str | None,
) -> ApprovalLevel:
    """Determine effective approval level using priority chain.

    Priority order (highest to lowest):
    1. Request-level (per-request override)
    2. Agent-level (agent default)
    3. Global-level (system default)
    4. AUTO (hardcoded fallback for backward compatibility)

    Args:
        request_level: Request-specific level (from request_context)
        agent_level: Agent default level (from agent config)
        global_level: Global default level (from system config)

    Returns:
        Effective approval level to use

    Example:
        # Request overrides agent
        get_effective_approval_level("SMART", "AUTO", "AUTO")
        → ApprovalLevel.SMART

        # Agent overrides global
        get_effective_approval_level(None, "OFF", "AUTO")
        → ApprovalLevel.OFF

        # Global fallback
        get_effective_approval_level(None, None, "STRICT")
        → ApprovalLevel.STRICT

        # Hardcoded fallback (backward compatible)
        get_effective_approval_level(None, None, None)
        → ApprovalLevel.AUTO
    """
    # Priority 1: Request-level
    if request_level:
        try:
            return parse_approval_level(request_level)
        except ValueError as exc:
            logger.warning(
                f"Invalid request-level approval: {request_level}, "
                f"falling back to agent/global level. Error: {exc}",
            )

    # Priority 2: Agent-level
    if agent_level:
        try:
            return parse_approval_level(agent_level)
        except ValueError as exc:
            logger.warning(
                f"Invalid agent-level approval: {agent_level}, "
                f"falling back to global level. Error: {exc}",
            )

    # Priority 3: Global-level
    if global_level:
        try:
            return parse_approval_level(global_level)
        except ValueError as exc:
            logger.warning(
                f"Invalid global-level approval: {global_level}, "
                f"falling back to AUTO. Error: {exc}",
            )

    # Priority 4: Hardcoded fallback (backward compatible)
    return DEFAULT_APPROVAL_LEVEL


def get_scenario_approval_level(
    agent_id: str,
    scenario: str,
) -> str | None:
    """Get approval level for a specific scenario (cron/agent_chat).

    Args:
        agent_id: Agent ID to load config from
        scenario: Scenario type ("cron" or "agent_chat")

    Returns:
        Approval level string to use in request_context, or None if not set

    Example:
        # Get cron job approval level
        level = get_scenario_approval_level("default", "cron")
        # Returns agent_config.approval_level_cron or None

        # Get agent chat approval level (for CLI and Tool)
        level = get_scenario_approval_level("default", "agent_chat")
        # Returns agent_config.approval_level_agent_chat or None
    """
    from ...config.config import load_agent_config

    try:
        agent_config = load_agent_config(agent_id)
    except Exception as exc:
        logger.warning(
            f"Failed to load agent config for {agent_id}: {exc}",
        )
        return None

    # Get scenario-specific approval level
    if scenario == "cron":
        return agent_config.approval_level_cron
    elif scenario == "agent_chat":
        return agent_config.approval_level_agent_chat
    else:
        logger.warning(f"Unknown scenario: {scenario}")
        return None

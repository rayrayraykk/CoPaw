# -*- coding: utf-8 -*-
"""WorkspaceRegistry — creates bootstrapped ``Workspace`` instances.

Extends ``MultiAgentManager`` with:
- ``app_services`` reference (cross-workspace shared)
- ``bootstrap_plugins_kwargs`` — the 5 built-in class lists discovered
  once at lifespan startup, injected into each new Workspace

The parent's ``Workspace`` creation is overridden to produce fully
bootstrapped instances.  All other behaviour (lazy loading, hot reload,
parallel startup) is inherited unchanged.
"""

from __future__ import annotations

import logging
from typing import Any

from .multi_agent_manager import MultiAgentManager

logger = logging.getLogger(__name__)


class WorkspaceRegistry(MultiAgentManager):
    """MultiAgentManager that creates bootstrapped ``Workspace`` instances."""

    def __init__(
        self,
        *,
        app_services: Any = None,
        bootstrap_plugins_kwargs: dict[str, Any] | None = None,
    ) -> None:
        super().__init__()
        self.app_services = app_services
        self._bootstrap_kwargs = bootstrap_plugins_kwargs or {}

    def _create_workspace(
        self,
        agent_id: str,
        workspace_dir: str,
    ) -> Any:
        """Create workspace, dispatching by agent type.

        ``external_acp`` agents get an
        ``ExternalAgentWorkspace``; all others get a native
        ``Workspace`` with bootstrapped plugins.
        """
        from ..config.utils import load_config

        config = load_config()
        profile = config.agents.profiles.get(agent_id)
        is_external = (
            profile is not None
            and getattr(profile, "type", "native") == "external_acp"
        )

        if is_external:
            from .workspace.external_agent_workspace import (
                ExternalAgentWorkspace,
            )

            workspace = ExternalAgentWorkspace(
                agent_id=agent_id,
                workspace_dir=workspace_dir,
            )
        else:
            from .workspace import Workspace

            workspace = Workspace(
                agent_id=agent_id,
                workspace_dir=workspace_dir,
            )
            if self._bootstrap_kwargs:
                workspace.bootstrap_plugins(
                    **self._bootstrap_kwargs,
                )

        if self.app_services is not None:
            workspace.set_app_services(self.app_services)
        return workspace


__all__ = ["WorkspaceRegistry"]

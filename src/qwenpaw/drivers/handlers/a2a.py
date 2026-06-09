"""A2A Driver handler skeleton for future integration."""

from __future__ import annotations

from typing import Any

from qwenpaw.drivers.credentials.types import ResolvedCredential
from qwenpaw.drivers.handler import DriverHandler
from qwenpaw.drivers.policy import PolicyContext


class A2ADriverHandler(DriverHandler):
    async def _setup(self) -> None:
        """A2A runtime integration is intentionally deferred."""

    async def _teardown(self) -> None:
        """No resources are owned by the skeleton handler."""

    async def _execute(
        self,
        credential: ResolvedCredential,
        context: PolicyContext,
        **kwargs: Any,
    ) -> Any:
        del credential
        del context
        del kwargs
        raise NotImplementedError("A2ADriverHandler is not wired yet")

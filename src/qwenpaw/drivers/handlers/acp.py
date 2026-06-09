"""ACP Driver handler skeleton for future integration."""

from __future__ import annotations

from typing import Any

from qwenpaw.drivers.credentials.types import ResolvedCredential
from qwenpaw.drivers.handler import DriverHandler
from qwenpaw.drivers.policy import PolicyContext


class ACPDriverHandler(DriverHandler):
    async def _setup(self) -> None:
        """ACP runtime integration is wired in a later phase."""

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
        raise NotImplementedError("ACPDriverHandler is not wired yet")

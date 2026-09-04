from __future__ import annotations

import json
import unittest
from pathlib import Path

from qwenpaw_sdk.protocol import (
    APP_PROTOCOL_REQUEST_METHODS,
    APP_PROTOCOL_SERVER_NOTIFICATION_METHODS,
    PROTOCOL_VERSION,
)


class ProtocolContractTest(unittest.TestCase):
    def test_matches_shared_app_protocol_fixtures(self) -> None:
        fixture_path = (
            Path(__file__).resolve().parents[3]
            / f"docs"
            / f"api-contract"
            / f"fixtures"
            / f"app-protocol-v3.json"
        )
        fixture = json.loads(fixture_path.read_text(encoding=f"utf-8"))
        self.assertEqual(fixture[f"protocolVersion"], PROTOCOL_VERSION)
        self.assertEqual(
            sorted(fixture[f"requests"]),
            sorted(APP_PROTOCOL_REQUEST_METHODS),
        )
        self.assertEqual(
            sorted(fixture[f"serverNotifications"]),
            sorted(APP_PROTOCOL_SERVER_NOTIFICATION_METHODS),
        )


if __name__ == f"__main__":
    unittest.main()

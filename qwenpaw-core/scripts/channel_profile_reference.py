"""Check original Profile update semantics with in-memory persistence."""

import ast
import json
import sys
import unittest
from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace

import httpx
from fastapi import APIRouter, Body, FastAPI, HTTPException, Request
from fastapi import Path as PathParam

from qwenpaw.config.config import AgentProfileConfig, ChannelConfig


OBSERVATIONS = []


class ChannelProfileReference(unittest.IsolatedAsyncioTestCase):
    """Exercise the original handler and request validation, not a copy."""

    async def asyncSetUp(self):
        source = (
            Path(__file__).resolve().parents[2]
            / f"src/qwenpaw/app/routers/agents.py"
        )
        selected = [
            node
            for node in ast.parse(source.read_text(encoding=f"utf-8")).body
            if isinstance(node, ast.AsyncFunctionDef)
            and node.name == f"update_agent"
        ]
        self.assertEqual(len(selected), 1)
        self.workspace = str(Path(sys.argv[1]).resolve() / f"writer")
        self.config = AgentProfileConfig(
            id=f"writer",
            name=f"writer",
            workspace_dir=self.workspace,
            channels=ChannelConfig.model_validate({
                f"console": {f"bot_prefix": f"previous-value"},
            }),
        )
        self.previous = self.config.model_dump(mode=f"json")
        self.reloads = []
        self.writes = 0

        async def run_sync(function, *args, **kwargs):
            return function(*args, **kwargs)

        async def mutate(agent_id, operation):
            self.assertEqual(agent_id, f"writer")
            candidate = deepcopy(self.config)
            operation(candidate)
            # Round-trip the full payload across the persistence boundary.
            self.config = AgentProfileConfig.model_validate_json(
                candidate.model_dump_json(),
            )
            self.writes += 1
            return deepcopy(self.config)

        def validate_backend(backend, mail):
            self.assertEqual((backend, mail), (f"qwenpaw", None))

        root = SimpleNamespace(agents=SimpleNamespace(
            profiles={f"writer": SimpleNamespace(
                workspace_dir=self.workspace,
            )},
        ))
        router = APIRouter(prefix=f"/agents")
        namespace = {
            f"AgentProfileConfig": AgentProfileConfig,
            f"PathParam": PathParam,
            f"Path": Path,
            f"Body": Body,
            f"Request": Request,
            f"HTTPException": HTTPException,
            f"AppBaseException": RuntimeError,
            f"router": router,
            f"deepcopy": deepcopy,
            f"run_sync_io": run_sync,
            f"load_config": lambda: root,
            f"load_agent_config": lambda _: deepcopy(self.config),
            f"update_agent_config_async": mutate,
            f"_validate_mail_backend_compatibility": validate_backend,
            f"_sync_qwenpawmail_driver_card": lambda *a, **k: True,
            f"schedule_agent_reload": (
                lambda request, agent_id: self.reloads.append(agent_id)
            ),
        }
        module = ast.Module(body=selected, type_ignores=[])
        exec(compile(module, str(source), f"exec"), namespace)
        app = FastAPI()
        app.include_router(router, prefix=f"/api")
        self.client = httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app),
            base_url=f"http://fixture.invalid",
        )

    async def asyncTearDown(self):
        await self.client.aclose()

    async def update(self, body, channels, status=200):
        response = await self.client.put(
            f"/api/agents/writer",
            json=body,
        )
        self.assertEqual(response.status_code, status)
        expected = deepcopy(self.previous)
        if status == 200:
            expected[f"channels"] = channels
            self.assertEqual(response.json(), expected)
        self.assertEqual(self.config.model_dump(mode=f"json"), expected)
        self.assertEqual(self.writes, int(status == 200))
        self.assertEqual(
            self.reloads, [f"writer"] if status == 200 else [],
        )
        OBSERVATIONS.append({
            f"case": self._testMethodName,
            f"status": status,
            f"channels": self.config.model_dump(mode=f"json")[f"channels"],
            f"writes": self.writes,
        })

    async def test_missing_preserves(self):
        await self.update(
            {f"id": f"writer", f"name": f"writer"},
            self.previous[f"channels"],
        )

    async def test_null_is_unconfigured(self):
        await self.update(
            {f"id": f"writer", f"name": f"writer", f"channels": None},
            None,
        )

    async def test_empty_materializes_defaults(self):
        await self.update(
            {f"id": f"writer", f"name": f"writer", f"channels": {}},
            ChannelConfig().model_dump(mode=f"json"),
        )

    async def test_partial_replaces_and_materializes_defaults(self):
        channels = {f"console": {f"bot_prefix": f"next-value"}}
        await self.update(
            {f"id": f"writer", f"name": f"writer", f"channels": channels},
            ChannelConfig.model_validate(channels).model_dump(mode=f"json"),
        )

    async def test_full_round_trip(self):
        await self.update(self.previous, self.previous[f"channels"])

    async def test_invalid_console_type_rejected(self):
        await self.update(
            {
                f"id": f"writer",
                f"name": f"writer",
                f"channels": {f"console": {f"bot_prefix": 123}},
            },
            self.previous[f"channels"],
            422,
        )

    async def test_id_and_name_required(self):
        await self.update(
            {f"channels": {}}, self.previous[f"channels"], 422,
        )

    async def test_console_array_rejected(self):
        await self.update(
            {
                f"id": f"writer",
                f"name": f"writer",
                f"channels": {f"console": []},
            },
            self.previous[f"channels"],
            422,
        )


if __name__ == f"__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(
        ChannelProfileReference,
    )
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    print(json.dumps({
        f"passed": result.wasSuccessful(),
        f"tests": result.testsRun,
        f"handler": f"original AST with real models and ASGI validation",
        f"persistence": f"in-memory JSON round-trip, no file I/O",
        f"observations": OBSERVATIONS,
    }))
    sys.exit(0 if result.wasSuccessful() else 1)

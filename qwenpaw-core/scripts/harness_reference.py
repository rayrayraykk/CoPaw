"""Execute original Harness HTTP handlers with explicit runtime doubles."""

import asyncio
import importlib
import json
import sys
import tempfile
import types
from pathlib import Path

import httpx
from fastapi import FastAPI


def load_handlers():
    """Load definitions without product startup or real provider discovery."""
    source = Path(__file__).resolve().parents[2] / f"src/qwenpaw"
    for suffix in (
        f"",
        f"app",
        f"app.routers",
        f"harnesses",
        f"harnesses.codex",
    ):
        name = f"qwenpaw.{suffix}" if suffix else f"qwenpaw"
        package = types.ModuleType(name)
        package.__path__ = [str(source.joinpath(*suffix.split(f".")))]
        sys.modules[name] = package

    def reject_runtime(*args, **kwargs):
        raise AssertionError(f"The reference must not create a real adapter")

    codex = types.ModuleType(f"qwenpaw.harnesses.codex.adapter")
    codex.CodexAdapter = reject_runtime
    sys.modules[codex.__name__] = codex
    context = types.ModuleType(f"qwenpaw.app.agent_context")
    context.get_agent_for_request = reject_runtime
    sys.modules[context.__name__] = context
    return importlib.import_module(f"qwenpaw.app.routers.harnesses")


class ReferenceAdapter:
    """Capture handler calls, never launch a provider or read credentials."""

    def __init__(self, fixture, calls, events, base):
        self.fixture = fixture
        self.calls = calls
        self.events = events
        self.base = base
        self.capability_unavailable_message = fixture.get(f"unavailable")

    async def status(self):
        self.calls.append({f"method": f"status"})
        return self.events.HarnessProvider(**self.fixture[f"status"])

    async def models(self):
        self.calls.append({f"method": f"models"})
        return [
            self.events.HarnessModel(**item)
            for item in self.fixture.get(f"models", [])
        ]

    async def discover_mcp(self, cwd):
        self.calls.append({f"method": f"discover_mcp", f"cwd": str(cwd)})
        return [
            self.events.HarnessDiscoveredMCPServer(**item)
            for item in self.fixture.get(f"servers", [])
        ]

    async def discover_skills(self, cwd):
        self.calls.append({f"method": f"discover_skills", f"cwd": str(cwd)})
        return [
            self.events.HarnessDiscoveredSkill(**item)
            for item in self.fixture.get(f"skills", [])
        ]

    async def start_login(self, device_code=False):
        self.calls.append(
            {
                f"method": f"start_login",
                f"device_code": device_code,
            }
        )
        return self.fixture[f"login"]

    async def logout(self):
        self.calls.append({f"method": f"logout"})
        if message := self.fixture.get(f"logout_not_supported"):
            raise self.base.HarnessOperationNotSupportedError(message)


class ReferenceRuntime:
    """Expose deterministic adapter and provider values to real handlers."""

    def __init__(self, fixture, calls, events, base):
        self.fixture = fixture
        self.calls = calls
        self.events = events
        self.base = base

    async def adapter(self, provider_id, settings):
        self.calls.append(
            {
                f"method": f"adapter",
                f"provider": provider_id,
                f"settings": settings,
            }
        )
        return ReferenceAdapter(
            self.fixture,
            self.calls,
            self.events,
            self.base,
        )

    async def providers(self, settings):
        self.calls.append({f"method": f"providers", f"settings": settings})
        return [
            self.events.HarnessProvider(**item)
            for item in self.fixture.get(f"providers", [])
        ]


def portable_paths(value, directory):
    """Normalize only the isolated workspace prefix in recorded results."""
    if isinstance(value, str):
        try:
            relative = Path(value).relative_to(directory)
        except ValueError:
            return value
        return f"$WORKSPACE/{relative.as_posix()}"
    if isinstance(value, list):
        return [portable_paths(item, directory) for item in value]
    if isinstance(value, dict):
        return {
            key: portable_paths(item, directory) for key, item in value.items()
        }
    return value


async def run_reference(fixture):
    """Return real HTTP status, JSON body and explicit dependency calls."""
    handlers = load_handlers()
    events = sys.modules[f"qwenpaw.harnesses.events"]
    base = sys.modules[f"qwenpaw.harnesses.base"]
    app = FastAPI()
    app.include_router(handlers.router, prefix=f"/api")
    calls = []
    responses = []
    with tempfile.TemporaryDirectory(prefix=f"qwenpaw-harness-ref-") as root:
        directory = str(Path(root).resolve())

        async def workspace_for_request(request):
            agent = request.headers.get(f"x-test-agent", f"default")
            workspace = fixture[f"workspaces"][agent]
            calls.append({f"method": f"workspace", f"agent": agent})
            return types.SimpleNamespace(
                workspace_dir=Path(directory) / agent,
                config=types.SimpleNamespace(
                    backend=workspace[f"backend"],
                    backend_settings=workspace.get(f"backend_settings", {}),
                ),
                harness_runtime=ReferenceRuntime(
                    workspace,
                    calls,
                    events,
                    base,
                ),
            )

        handlers.get_agent_for_request = workspace_for_request
        transport = httpx.ASGITransport(app=app)
        async with httpx.AsyncClient(
            transport=transport,
            base_url=f"http://reference.test",
        ) as client:
            for item in fixture[f"requests"]:
                calls.clear()
                kwargs = {
                    f"headers": {
                        f"x-test-agent": item.get(f"agent", f"default"),
                    }
                }
                if f"body" in item:
                    kwargs[f"json"] = item[f"body"]
                response = await client.request(
                    item[f"method"],
                    item[f"path"],
                    **kwargs,
                )
                responses.append(
                    portable_paths(
                        {
                            f"status": response.status_code,
                            f"body": response.json(),
                            f"calls": list(calls),
                        },
                        directory,
                    )
                )
    return {f"responses": responses}


if __name__ == f"__main__":
    print(json.dumps(asyncio.run(run_reference(json.load(sys.stdin)))))

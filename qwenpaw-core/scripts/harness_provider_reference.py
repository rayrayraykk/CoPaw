"""Evaluate original catalog and Codex status without importing the runtime."""

import ast
import asyncio
import importlib.util
import json
import sys
import types
from dataclasses import dataclass
from pathlib import Path


class ReferenceError(RuntimeError):
    """Represent an original transport error using fixture text."""


class ReferenceClient:
    def __init__(self, fixture):
        self.fixture = fixture
        self.installed = fixture.get(f"installed", False)
        path = fixture.get(f"runtime_path")
        self.binary_resolution = (
            types.SimpleNamespace(
                path=Path(path), source=fixture[f"runtime_source"]
            )
            if path is not None
            else None
        )

    async def start(self):
        """Do not start a process in the reference."""

    async def request(self, method, params):
        """Read account fixtures using the exact original request contract."""
        assert method == f"account/read"
        assert params == {f"refreshToken": False}
        if f"error" in self.fixture:
            raise ReferenceError(self.fixture[f"error"])
        return self.fixture.get(f"response")


def original_namespace():
    """Compile original declarations and the unchanged status method."""
    root = Path(__file__).resolve().parents[2] / f"src/qwenpaw/harnesses"
    spec = importlib.util.spec_from_file_location(
        f"reference_provider_events", root / f"events.py"
    )
    events = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = events
    spec.loader.exec_module(events)
    namespace = {
        f"dataclass": dataclass,
        f"HarnessProvider": events.HarnessProvider,
        f"HarnessCapabilities": events.HarnessCapabilities,
        f"HarnessCommand": events.HarnessCommand,
        f"HarnessApprovalPreset": events.HarnessApprovalPreset,
        f"CodexAppServerError": ReferenceError,
    }
    registry = root / f"registry.py"
    nodes = ast.parse(registry.read_text(encoding=f"utf-8")).body
    selected = [
        node
        for node in nodes
        if (
            isinstance(node, ast.ClassDef)
            and node.name == f"ProviderCatalogItem"
        )
        or (
            isinstance(node, ast.Assign)
            and any(
                isinstance(target, ast.Name)
                and target.id == f"PROVIDER_CATALOG"
                for target in node.targets
            )
        )
    ]
    assert len(selected) == 2
    exec(compile(ast.Module(selected, []), str(registry), f"exec"), namespace)
    adapter_path = root / f"codex/adapter.py"
    nodes = ast.parse(adapter_path.read_text(encoding=f"utf-8")).body
    adapter = next(
        node
        for node in nodes
        if isinstance(node, ast.ClassDef) and node.name == f"CodexAdapter"
    )
    selected = [
        node
        for node in nodes
        if isinstance(node, ast.Assign)
        and any(
            isinstance(target, ast.Name)
            and target.id == f"_CODEX_CLI_INSTALL_MESSAGE"
            for target in node.targets
        )
    ] + [
        node
        for node in adapter.body
        if isinstance(node, ast.AsyncFunctionDef) and node.name == f"status"
    ]
    assert len(selected) == 2
    exec(
        compile(ast.Module(selected, []), str(adapter_path), f"exec"),
        namespace,
    )
    return namespace


async def evaluate(fixture):
    """Return full original output, including router capability overlay."""
    original = original_namespace()
    catalog = original[f"PROVIDER_CATALOG"]
    if fixture[f"operation"] == f"catalog":
        return [
            {
                f"id": item.id,
                f"name": item.name,
                f"coming_soon": item.coming_soon,
                f"capabilities": item.capabilities.model_dump(),
            }
            for item in catalog
        ]
    assert fixture[f"operation"] == f"status"
    owner = types.SimpleNamespace(_client=ReferenceClient(fixture))
    status = await original[f"status"](owner)
    status.capabilities = catalog[0].capabilities
    return status.model_dump()


if __name__ == f"__main__":
    print(json.dumps(asyncio.run(evaluate(json.loads(sys.argv[1])))))

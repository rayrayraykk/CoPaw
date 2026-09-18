"""Run original control-method AST bodies with a deterministic RPC double."""

import ast
import asyncio
import importlib.util
import json
import sys
import types
from pathlib import Path
from typing import Any


class ReferenceError(RuntimeError):
    """Represent a transport error without starting a process."""


class ReferenceClient:
    """Return fixture replies and capture RPC calls, never credentials."""

    installed = True
    binary_resolution = None

    def __init__(self, responses):
        self.responses = list(responses)
        self.requests = []

    async def start(self):
        """The reference transport is already connected."""

    async def request(self, method, params):
        """Record method parameters and return the next fixture result."""
        self.requests.append({f"method": method, f"params": params})
        if not self.responses:
            raise AssertionError(f"Original method requested an extra page")
        return self.responses.pop(0)


def original_methods():
    """Compile original method bodies without importing the runtime."""
    source = Path(__file__).resolve().parents[2] / f"src/qwenpaw/harnesses"
    spec = importlib.util.spec_from_file_location(
        f"reference_harness_events",
        source / f"events.py",
    )
    events = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = events
    spec.loader.exec_module(events)
    adapter_path = source / f"codex/adapter.py"
    tree = ast.parse(adapter_path.read_text(encoding=f"utf-8"))
    adapter = next(
        node
        for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == f"CodexAdapter"
    )
    names = {
        f"status",
        f"start_login",
        f"logout",
        f"models",
        f"discover_skills",
        f"discover_mcp",
    }
    bodies = [
        node
        for node in adapter.body
        if isinstance(node, ast.AsyncFunctionDef) and node.name in names
    ]
    assert {node.name for node in bodies} == names
    namespace = {
        f"Any": Any,
        f"HarnessProvider": events.HarnessProvider,
        f"HarnessModel": events.HarnessModel,
        f"HarnessDiscoveredSkill": events.HarnessDiscoveredSkill,
        f"HarnessDiscoveredMCPServer": events.HarnessDiscoveredMCPServer,
        f"Path": Path,
        f"CodexAppServerError": ReferenceError,
    }
    exec(
        compile(
            ast.Module(body=bodies, type_ignores=[]),
            str(adapter_path),
            f"exec",
        ),
        namespace,
    )
    return namespace


async def run_reference(fixture):
    """Project original methods onto connected-client controls."""
    methods = original_methods()
    client = ReferenceClient(fixture[f"responses"])
    owner = types.SimpleNamespace(_client=client)
    operation = fixture[f"operation"]
    if operation == f"mcp":
        return await mcp_reference(methods, owner, fixture)
    if operation == f"skills":
        result = await methods[f"discover_skills"](
            owner, Path(fixture[f"cwd"])
        )
        result = [item.model_dump() for item in result]
    elif operation in (f"browser", f"device"):
        result = await methods[f"start_login"](
            owner,
            device_code=operation == f"device",
        )
    else:
        result = await methods[operation](owner)
        if operation == f"status":
            result = {
                f"authenticated": result.authenticated,
                f"account": result.account,
            }
        elif operation == f"models":
            result = [item.model_dump() for item in result]
    assert not client.responses, f"Original method left unused replies"
    return {f"result": result, f"requests": client.requests}


async def mcp_reference(methods, owner, fixture):
    """Run the unchanged discovery body with a fake subprocess boundary."""
    calls = []
    binary = fixture[f"binary"]
    owner._client.binary_resolution = (
        types.SimpleNamespace(path=Path(binary)) if binary else None
    )

    async def communicate():
        return bytes(fixture[f"stdout"]), bytes(fixture[f"stderr"])

    async def create_subprocess_exec(*args, **kwargs):
        calls.append({f"program": args[0], f"args": list(args[1:]), **kwargs})
        return types.SimpleNamespace(
            communicate=communicate, returncode=fixture[f"returncode"]
        )

    methods[f"asyncio"] = types.SimpleNamespace(
        create_subprocess_exec=create_subprocess_exec,
        subprocess=types.SimpleNamespace(PIPE=-1),
    )
    methods[f"json"] = json
    try:
        result = await methods[f"discover_mcp"](owner, Path(fixture[f"cwd"]))
    except ReferenceError as error:
        return {f"error": str(error), f"requests": calls}
    return {
        f"result": [item.model_dump() for item in result],
        f"requests": calls,
    }


if __name__ == f"__main__":
    print(json.dumps(asyncio.run(run_reference(json.loads(sys.argv[1])))))

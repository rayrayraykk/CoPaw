"""Execute original session methods using in-memory storage and RPC doubles."""

import ast
import asyncio
import json
import sys
import types
from pathlib import Path
from typing import Any


class ReferenceError(RuntimeError):
    """Represent an explicit remote protocol rejection."""


class Client:
    """Record thread RPCs without launching a process."""

    def __init__(self, reject):
        self.reject = reject
        self.requests = []
        self.count = 0

    async def request(self, method, params):
        """Reply using the same deterministic IDs as the Rust fixture."""
        self.requests.append({f"method": method, f"params": params})
        if method == f"thread/resume":
            if self.reject:
                raise ReferenceError(f"thread missing")
            return {f"thread": {f"id": params[f"threadId"]}}
        assert method == f"thread/start"
        self.count += 1
        return {f"thread": {f"id": f"fixture-thread-{self.count}"}}


async def ignore_write(path, payload):
    """The reference records in-memory state only; no filesystem writes."""


def original_methods():
    """Load unchanged method ASTs without importing the product runtime."""
    source = (
        Path(__file__)
        .resolve()
        .parents[2]
        .joinpath(f"src/qwenpaw/harnesses/codex/adapter.py")
    )
    tree = ast.parse(source.read_text(encoding=f"utf-8"))
    adapter = next(
        node
        for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == f"CodexAdapter"
    )
    names = {f"_thread_for_session", f"reset_session"}
    bodies = [
        node
        for node in adapter.body
        if isinstance(node, ast.AsyncFunctionDef) and node.name in names
    ]
    assert {node.name for node in bodies} == names
    namespace = {
        f"Any": Any,
        f"Path": Path,
        f"CodexAppServerError": ReferenceError,
        f"write_json_atomic_async": ignore_write,
    }
    exec(
        compile(
            ast.Module(body=bodies, type_ignores=[]), str(source), f"exec"
        ),
        namespace,
    )
    return namespace


async def run_reference(fixture):
    """Return complete requests, results and mappings for a sequence."""
    methods = original_methods()
    owner = types.SimpleNamespace(
        _session_lock=asyncio.Lock(),
        _threads=dict(fixture.get(f"threads", {})),
        _loaded_threads=set(),
        _thread_contexts={},
        _session_clients={},
        _session_fingerprints={},
        _session_path=Path(f"unused-reference-path"),
    )
    client = Client(fixture.get(f"reject_resume", False))
    results = []
    for operation in fixture[f"operations"]:
        session = operation[f"session"]
        if operation.get(f"reset"):
            await methods[f"reset_session"](owner, session)
            results.append(None)
        else:
            results.append(
                await methods[f"_thread_for_session"](
                    owner,
                    client,
                    session,
                    Path(fixture[f"cwd"]),
                    operation.get(f"settings", {}),
                )
            )
    return {
        f"results": results,
        f"requests": client.requests,
        f"threads": owner._threads,
    }


if __name__ == f"__main__":
    print(json.dumps(asyncio.run(run_reference(json.loads(sys.argv[1])))))

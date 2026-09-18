"""Execute original capability projection with explicit in-memory fixtures."""

import ast
import hashlib
import importlib.util
import json
import sys
import types
from pathlib import Path
from typing import Any


def original_modules():
    """Load original models and function bodies without product boot."""
    root = Path(__file__).resolve().parents[2] / f"src/qwenpaw/harnesses"
    spec = importlib.util.spec_from_file_location(
        f"reference_capability_models", root / f"capabilities/models.py"
    )
    models = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = models
    spec.loader.exec_module(models)

    source = root / f"codex/projection.py"
    tree = ast.parse(source.read_text(encoding=f"utf-8"))
    tree.body = [
        node
        for node in tree.body
        if not (isinstance(node, ast.ImportFrom) and node.level)
    ]
    projection = types.ModuleType(f"reference_codex_projection")
    projection.HarnessRuntimeCapabilities = models.HarnessRuntimeCapabilities
    sys.modules[projection.__name__] = projection
    exec(compile(tree, str(source), f"exec"), projection.__dict__)

    resolver_path = root / f"capabilities/resolver.py"
    resolver = ast.parse(resolver_path.read_text(encoding=f"utf-8"))
    owner = next(
        node
        for node in resolver.body
        if isinstance(node, ast.ClassDef)
        if node.name == f"HarnessCapabilityResolver"
    )
    method = next(
        node
        for node in owner.body
        if isinstance(node, ast.FunctionDef)
        if node.name == f"_runtime_revision"
    )
    owner.body = [method]
    namespace = {f"Any": Any, f"hashlib": hashlib, f"json": json}
    exec(
        compile(
            ast.Module(body=[owner], type_ignores=[]),
            str(resolver_path),
            f"exec",
        ),
        namespace,
    )
    revision = namespace[f"HarnessCapabilityResolver"]._runtime_revision
    return models, projection, revision


def reference(fixture):
    """Return whole projection and identity using fixture values only."""
    models, projection, revision = original_modules()
    inputs = dict(fixture[f"capabilities"])
    servers = [dict(server) for server in inputs.get(f"mcp_servers", [])]
    for server in servers:
        if f"env_entries" in server:
            server[f"env"] = dict(server.pop(f"env_entries"))
    inputs[f"mcp_servers"] = servers
    capabilities = models.HarnessRuntimeCapabilities.model_validate(inputs)
    if fixture.get(f"refresh_runtime_revisions", False):
        for server in capabilities.mcp_servers:
            server.runtime_revision = revision(server.env, server.headers)
    try:
        result = projection.project_runtime(capabilities)
    except ValueError as error:
        return {f"error": str(error)}
    return {
        f"fingerprint": capabilities.fingerprint,
        f"runtime_revisions": [
            server.runtime_revision for server in capabilities.mcp_servers
        ],
        f"projection": {
            f"config_overrides": list(result.config_overrides),
            f"environment": result.environment,
            f"skill_roots": list(result.skill_roots),
        },
    }


if __name__ == f"__main__":
    print(json.dumps(reference(json.loads(sys.argv[1]))))

"""Run original frontend plugin handlers without product initialization."""

import ast
import asyncio
import importlib.util
import json
import logging
import mimetypes
import re
import sys
import tempfile
import types
from pathlib import Path

import httpx
from fastapi import APIRouter, FastAPI, HTTPException, Request
from fastapi.responses import FileResponse


def functions(source, names, namespace):
    """Execute unchanged selected definitions from the real source files."""
    tree = ast.parse(source.read_text(encoding=f"utf-8"))
    selected = [
        node for node in tree.body
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and node.name in names
    ]
    assert len(selected) == len(names)
    exec(compile(ast.Module(selected, []), str(source), f"exec"), namespace)


async def main():
    """Compare isolated manifests and actual ASGI static responses."""
    source = Path(__file__).resolve().parents[2] / f"src/qwenpaw"
    for name in (
        f"qwenpaw", f"qwenpaw.config", f"qwenpaw.plugins", f"qwenpaw.app",
        f"qwenpaw.app.routers", f"qwenpaw.config.utils",
        f"qwenpaw.plugins.loader",
    ):
        module = types.ModuleType(name)
        module.__path__ = []
        sys.modules[name] = module
    architecture = f"qwenpaw.plugins.architecture"
    spec = importlib.util.spec_from_file_location(
        architecture, source / f"plugins/architecture.py"
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules[architecture] = module
    spec.loader.exec_module(module)
    loader = sys.modules[f"qwenpaw.plugins.loader"].__dict__
    loader[f"Path"] = Path
    functions(
        source / f"plugins/loader.py", {f"_is_disabled_plugin_dir"}, loader
    )
    with tempfile.TemporaryDirectory(prefix=f"qwenpaw-plugin-reference-") as d:
        plugins = Path(d) / f"plugins"
        ui = plugins / f"demo/ui"
        ui.mkdir(parents=True)
        manifest = json.loads(sys.argv[1])
        (ui.parent / f"plugin.json").write_text(json.dumps(manifest))
        for name in (f"main.js", f"chunk-abcdefgh.js", f"style.css"):
            (ui / name).write_text(f"hello")
        sys.modules[f"qwenpaw.config.utils"].get_plugins_dir = lambda: plugins
        router = APIRouter(prefix=f"/frontend_plugin")
        namespace = {
            f"__name__": f"qwenpaw.app.routers.plugins",
            f"__package__": f"qwenpaw.app.routers",
            f"Path": Path, f"Request": Request,
            f"HTTPException": HTTPException,
            f"FileResponse": FileResponse, f"router": router,
            f"json": json, f"re": re, f"mimetypes": mimetypes,
            f"logger": logging.getLogger(f"plugin-reference"),
        }
        functions(source / f"app/routers/plugins.py", {
            f"_list_plugins_from_disk", f"serve_plugin_ui_file"
        }, namespace)
        functions(source / f"app/routers/frontend_plugin.py", {
            f"list_frontend_plugins"
        }, namespace)
        app = FastAPI()
        app.include_router(router, prefix=f"/api")
        results = []
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app),
            base_url=f"http://fixture.invalid",
        ) as client:
            response = await client.get(f"/api/frontend_plugin")
            results.append([response.status_code, response.json()])
            for name in (f"main.js", f"chunk-abcdefgh.js", f"style.css"):
                for method, headers in (
                    (f"GET", {}),
                    (f"GET", {f"Range": f"bytes=1-2"}),
                    (f"GET", {
                        f"If-Modified-Since": f"Sat, 01 Jan 2050 00:00:00 GMT"
                    }),
                ):
                    response = await client.request(
                        method, f"/api/frontend_plugin/demo/files/ui/{name}",
                        headers=headers,
                    )
                    results.append({
                        f"status": response.status_code,
                        f"body": response.text,
                        f"mime": response.headers[f"content-type"],
                        f"cache": response.headers[f"cache-control"],
                    })
        print(json.dumps(results))


if __name__ == f"__main__":
    asyncio.run(main())

"""Execute original debug handlers without loading the legacy application."""

import ast
import asyncio
import json
import logging
import sys
from pathlib import Path

import httpx
from fastapi import APIRouter, FastAPI, Query


async def main() -> None:
    """Read only the supplied isolated log through the original handlers."""
    source = (
        Path(__file__).resolve().parents[2]
        / f"src/qwenpaw/app/routers/console.py"
    )
    tree = ast.parse(source.read_text(encoding=f"utf-8"))
    names = {f"_tail_text_file", f"get_backend_debug_logs"}
    functions = [
        node for node in tree.body
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and node.name in names
    ]
    assert len(functions) == 2
    router = APIRouter(prefix=f"/console")
    namespace = {
        f"Path": Path,
        f"LOG_FILE_PATH": Path(sys.argv[1]),
        f"MAX_DEBUG_LOG_LINES": 1000,
        f"logger": logging.getLogger(f"isolated-reference"),
        f"router": router,
        f"Query": Query,
    }
    module = ast.Module(body=functions, type_ignores=[])
    exec(compile(module, str(source), f"exec"), namespace)
    app = FastAPI()
    app.include_router(router, prefix=f"/api")
    results = []
    async with httpx.AsyncClient(
        transport=httpx.ASGITransport(app=app),
        base_url=f"http://fixture.invalid",
    ) as client:
        for query in json.loads(sys.argv[2]):
            response = await client.get(
                f"/api/console/debug/backend-logs{query}"
            )
            results.append([response.status_code, response.json()])
    print(json.dumps(results, ensure_ascii=False))


if __name__ == f"__main__":
    asyncio.run(main())

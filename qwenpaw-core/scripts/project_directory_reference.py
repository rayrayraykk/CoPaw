# -*- coding: utf-8 -*-
"""Run original project handlers with isolated in-memory Agent bindings."""

import ast
import asyncio
import json
import sys
from pathlib import Path
from types import SimpleNamespace

import httpx
from fastapi import APIRouter, FastAPI, HTTPException, Request
from pydantic import BaseModel


async def main() -> None:
    """Compare responses without importing or initializing the legacy app."""
    roots = json.loads(sys.argv[1])
    agents = {
        name: SimpleNamespace(
            agent_id=name,
            workspace_dir=Path(path),
            project_dir=None,
        )
        for name, path in roots.items()
    }

    async def resolve(request: Request) -> SimpleNamespace:
        return agents[request.headers.get(f"X-Agent-Id", f"default")]

    def project(agent: SimpleNamespace) -> Path:
        return Path(agent.project_dir or agent.workspace_dir)

    def save(agent_id: str, path: str | None) -> None:
        agents[agent_id].project_dir = path

    source = (
        Path(__file__).resolve().parents[2]
        / f"src/qwenpaw/app/routers/project_directory.py"
    )
    names = {
        f"SetProjectRequest",
        f"_projects_base",
        f"get_project",
        f"set_project",
        f"list_projects",
    }
    selected = [
        node
        for node in ast.parse(source.read_text(encoding=f"utf-8")).body
        if isinstance(
            node,
            (
                ast.ClassDef,
                ast.FunctionDef,
                ast.AsyncFunctionDef,
            ),
        )
        and node.name in names
    ]
    assert len(selected) == len(names)
    router = APIRouter(prefix=f"/workspace/project-directory")
    namespace = {
        f"asyncio": asyncio,
        f"Path": Path,
        f"Request": Request,
        f"BaseModel": BaseModel,
        f"HTTPException": HTTPException,
        f"router": router,
        f"CODING_PROJECT_SUBDIR": f"coding_projects",
        f"get_agent_for_request": resolve,
        f"get_agent_project_dir": project,
        f"_save_project_dir": save,
    }
    module = ast.Module(body=selected, type_ignores=[])
    exec(compile(module, str(source), f"exec"), namespace)
    app = FastAPI()
    app.include_router(router, prefix=f"/api")
    results = []
    async with httpx.AsyncClient(
        transport=httpx.ASGITransport(app=app),
        base_url=f"http://fixture.invalid",
    ) as client:
        for method, suffix, actor, body in json.loads(sys.argv[2]):
            response = await client.request(
                method,
                f"/api/workspace/project-directory{suffix}",
                headers={f"X-Agent-Id": actor},
                json=body,
            )
            results.append([response.status_code, response.json()])
    print(json.dumps(results, ensure_ascii=False))


if __name__ == f"__main__":
    asyncio.run(main())

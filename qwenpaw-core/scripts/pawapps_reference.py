"""Run original PawApp directory routes with isolated, supplied fixtures."""

import asyncio
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

import httpx
from fastapi import FastAPI


async def main() -> None:
    """Return actual Python HTTP responses, without loading user settings."""
    source = (
        Path(__file__).resolve().parents[2]
        / f"src/qwenpaw/app/routers/pawapps.py"
    )
    spec = importlib.util.spec_from_file_location(f"pawapps_reference", source)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    manifest = json.loads(sys.argv[1])
    with tempfile.TemporaryDirectory(
        prefix=f"qwenpaw-pawapps-reference-"
    ) as d:
        plugins = Path(d) / f"plugins"
        app_dir = plugins / f"demo"
        app_dir.mkdir(parents=True)
        (app_dir / f"plugin.json").write_text(
            json.dumps(manifest), encoding=f"utf-8"
        )
        module._get_apps_dir = lambda: plugins
        app = FastAPI()
        app.include_router(module.router, prefix=f"/api")
        responses = []
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app),
            base_url=f"http://fixture.invalid",
        ) as client:
            for method, path in [
                (f"GET", f"/api/pawapps"),
                (f"GET", f"/api/pawapps/demo"),
                (f"GET", f"/api/pawapps/demo/settings"),
                (f"DELETE", f"/api/pawapps/demo"),
                (f"GET", f"/api/pawapps"),
                (f"GET", f"/api/pawapps/demo"),
            ]:
                response = await client.request(method, path)
                responses.append([response.status_code, response.json()])
        assert not app_dir.exists()
        print(json.dumps(responses, ensure_ascii=False))


if __name__ == f"__main__":
    asyncio.run(main())

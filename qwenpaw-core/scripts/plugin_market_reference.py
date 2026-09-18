"""Execute original market search with an isolated HTTPX transport."""

import asyncio
import json
import logging
import sys
from pathlib import Path
from typing import Optional

import httpx
from fastapi import APIRouter, FastAPI, HTTPException

from .frontend_plugin_reference import functions


async def main():
    """Return actual ASGI responses and upstream query parameters."""
    queries = json.loads(sys.argv[1])
    payload = json.loads(sys.argv[2])
    requests = []

    def remote(request):
        requests.append(dict(request.url.params))
        return httpx.Response(200, json=payload)

    client_type = httpx.AsyncClient
    transport = httpx.MockTransport(remote)
    httpx.AsyncClient = lambda **kwargs: client_type(
        transport=transport, **kwargs
    )
    router = APIRouter(prefix=f"/plugins")
    namespace = {
        f"router": router, f"Optional": Optional,
        f"HTTPException": HTTPException,
        f"logger": logging.getLogger(f"plugin-market-reference"),
        f"_PLUGIN_MARKET_TIMEOUT": 15,
        f"_PLUGIN_MARKET_BASE_URL": f"https://fixture.invalid",
    }
    source = Path(__file__).resolve().parents[2]
    functions(
        source / f"src/qwenpaw/app/routers/plugins.py",
        {f"search_market_plugins"}, namespace,
    )
    app = FastAPI()
    app.include_router(router, prefix=f"/api")
    results = []
    async with client_type(
        transport=httpx.ASGITransport(app=app),
        base_url=f"http://fixture.invalid",
    ) as client:
        for query in queries:
            before = len(requests)
            response = await client.get(f"/api/plugins/market/search{query}")
            results.append({
                f"status": response.status_code, f"body": response.json(),
                f"query": requests[-1] if len(requests) > before else None,
            })
    print(json.dumps(results))


if __name__ == f"__main__":
    asyncio.run(main())

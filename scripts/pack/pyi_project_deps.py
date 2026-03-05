# -*- coding: utf-8 -*-
"""
Shared PyInstaller helper: list installed packages to collect (macOS/Windows).

Use get_collect_packages_from_installed() after `pip install -e ".[full]"` so
direct and transitive deps are bundled; no manual list. Stdlib only.

Root cause of missed packages: some deps are only loaded at runtime via
entry points (e.g. opentelemetry propagators: tracecontext, baggage). Those
packages are not imported statically, so PyInstaller never sees them. We
supplement by also collecting any distribution that provides an entry point
in known groups (see get_packages_from_entry_point_groups).
"""
from __future__ import annotations

import site
import sys
from pathlib import Path

from importlib.metadata import distributions, entry_points


def get_pathex_extensions() -> list[str]:
    """
    Return extra paths for PyInstaller Analysis pathex so it can find all
    installed modules (e.g. venv/lib/pythonX.Y/site-packages). Reduces
    missing-module errors. Use like: pathex=base_pathex + get_pathex_extensions().
    """
    seen: set[str] = set()
    out: list[str] = []
    for p in getattr(site, "getsitepackages", lambda: [])():
        p = Path(p)
        if p.is_dir():
            s = str(p.resolve())
            if s not in seen:
                seen.add(s)
                out.append(s)
    up = getattr(site, "getusersitepackages", lambda: None)()
    if up:
        p = Path(up)
        if p.is_dir():
            s = str(p.resolve())
            if s not in seen:
                seen.add(s)
                out.append(s)
    if not out:
        for p in sys.path:
            if "site-packages" in p or "dist-packages" in p:
                p = Path(p)
                if p.is_dir():
                    s = str(p.resolve())
                    if s not in seen:
                        seen.add(s)
                        out.append(s)
    return out


# PyPI dist name -> import name (where they differ). Rest use - -> _.
PYPI_TO_IMPORT = {
    "agentscope-runtime": "agentscope_runtime",
    "discord-py": "discord",
    "discord.py": "discord",
    "dingtalk-stream": "dingtalk_stream",
    "python-dotenv": "dotenv",
    "python-socks": "python_socks",
    "lark-oapi": "lark_oapi",
    "python-telegram-bot": "telegram",
    "reme-ai": "reme",
    "llama-cpp-python": "llama_cpp",
    "mlx-lm": "mlx_lm",
    "email-validator": "email_validator",
}

# Do not bundle these (build tools / copaw from source).
EXCLUDE_FROM_BUNDLE = frozenset(
    {"pip", "setuptools", "wheel", "pyinstaller", "copaw"},
)

# Entry point groups used at runtime by our deps; packages that register here
# are often loaded only via entry_points().load(), so static analysis misses them.
ENTRY_POINT_GROUPS = ("opentelemetry_propagator",)


def _pypi_to_import(name: str) -> str:
    n = name.strip().lower()
    return PYPI_TO_IMPORT.get(n, n.replace("-", "_"))


def get_packages_from_entry_point_groups(
    groups: tuple[str, ...] = ENTRY_POINT_GROUPS,
) -> list[str]:
    """
    Return import names of distributions that provide any entry point in
    the given groups. Use this to collect packages that are only loaded at
    runtime via entry points (e.g. opentelemetry-propagator-tracecontext).
    Requires Python 3.10+ for EntryPoint.dist.
    """
    out: list[str] = []
    seen: set[str] = set()
    for group in groups:
        try:
            eps = entry_points(group=group)
        except Exception:
            continue
        for ep in eps:
            try:
                dist = getattr(ep, "dist", None)
                if dist is None:
                    continue
                name = dist.metadata.get("Name")
                if not name:
                    continue
                name = name.strip().lower()
                if name in EXCLUDE_FROM_BUNDLE:
                    continue
                imp = _pypi_to_import(name)
                if not imp or imp == "copaw" or imp in seen:
                    continue
                seen.add(imp)
                out.append(imp)
            except Exception:
                continue
    return sorted(out)


def get_collect_packages_from_installed() -> list[str]:
    """
    Return import names of all installed packages (for collect_all), excluding
    build tools and copaw. Call after `pip install -e ".[full]"` so direct and
    transitive deps are included. Also merges packages that provide entry
    points in ENTRY_POINT_GROUPS so runtime-only loads (e.g. OTEL propagators)
    are not missed.
    """
    seen: set[str] = set()
    out: list[str] = []
    for dist in distributions():
        try:
            name = dist.metadata.get("Name")
            if not name:
                continue
            name = name.strip().lower()
            if name in EXCLUDE_FROM_BUNDLE:
                continue
            imp = _pypi_to_import(name)
            if not imp or imp == "copaw" or imp in seen:
                continue
            seen.add(imp)
            out.append(imp)
        except Exception:
            continue
    for imp in get_packages_from_entry_point_groups():
        if imp not in seen:
            seen.add(imp)
            out.append(imp)
    return sorted(out)

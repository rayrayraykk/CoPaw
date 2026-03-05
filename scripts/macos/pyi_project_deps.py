# -*- coding: utf-8 -*-
"""
PyInstaller spec helper: list packages to collect from pyproject or installed env.

Use get_collect_packages_from_installed() so we collect whatever is installed
after `pip install -e ".[full]"` (direct + transitive deps); no manual list.
Stdlib only.
"""
from __future__ import annotations

import re
from importlib.metadata import distributions
from pathlib import Path


# PyPI dist name -> import name (where they differ). Rest use name with - -> _.
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

# Do not bundle these (build tools / we handle copaw from source).
EXCLUDE_FROM_BUNDLE = frozenset(
    {"pip", "setuptools", "wheel", "pyinstaller", "copaw"},
)


def _pypi_to_import(name: str) -> str:
    n = name.strip().lower()
    return PYPI_TO_IMPORT.get(n, n.replace("-", "_"))


def _parse_dep_spec(spec: str) -> str:
    """Extract PyPI name from 'name>=x', 'name==x', 'name[x,y]' etc."""
    spec = spec.strip().strip("\"'")
    # Drop [extras] and version specifiers
    base = re.split(r"\s*[\[\]>=<\!]", spec)[0].strip()
    return base


def get_collect_packages_from_installed() -> list[str]:
    """
    Return import names of all installed packages (for collect_all), excluding
    build tools and copaw. Call after `pip install -e ".[full]"` so direct and
    transitive deps are included; no manual extra_collect needed.
    """
    out: list[str] = []
    seen: set[str] = set()
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
    return sorted(out)


def get_collect_packages(  # pylint: disable=too-many-branches
    pyproject_path: Path | None = None,
    extras: tuple[str, ...] = ("full",),
    extra_collect: tuple[str, ...] = (),
) -> list[str]:
    """
    Return list of import names for project deps + given optional extras.

    extra_collect: transitive deps not in pyproject that must be bundled
    (e.g. chromadb, flowllm, fastmcp, email_validator).

    Used by copaw.spec / copaw_dev.spec to collect_all() each so the frozen
    app has every dependency and its metadata without manual patches.
    """
    if pyproject_path is None:
        pyproject_path = (
            Path(__file__).resolve().parent.parent.parent / "pyproject.toml"
        )
    text = pyproject_path.read_text(encoding="utf-8")

    import_names: list[str] = []
    seen: set[str] = set()

    def add(name: str) -> None:
        imp = _pypi_to_import(name)
        if imp and imp not in seen:
            seen.add(imp)
            import_names.append(imp)

    in_deps = False
    in_optional_list = False
    want_this_extra = False

    for line in text.splitlines():
        line_strip = line.strip()
        if line_strip == "dependencies = [":
            in_deps = True
            continue
        if in_deps:
            if line_strip == "]":
                in_deps = False
                continue
            m = re.match(r'^\s*["\']([^"\']+)["\']', line)
            if m:
                add(_parse_dep_spec(m.group(1)))
            continue
        if line_strip == "[project.optional-dependencies]":
            in_optional_list = False
            continue
        if " = [" in line_strip:
            extra_name = line_strip.split(" = [")[0].strip()
            in_optional_list = True
            want_this_extra = extra_name in extras
            continue
        if in_optional_list and line_strip == "]":
            in_optional_list = False
            want_this_extra = False
            continue
        if in_optional_list and want_this_extra and line_strip:
            m = re.match(r'^\s*["\']([^"\']+)["\']', line)
            if m:
                spec = m.group(1)
                if not spec.startswith("copaw["):
                    add(_parse_dep_spec(spec))
            continue

    for _pkg in extra_collect:
        if _pkg and _pkg not in seen:
            seen.add(_pkg)
            import_names.append(_pkg)

    return import_names

# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec for CoPaw macOS .app **Dev** variant (console=True for logs).
# Run from repo root: pyinstaller scripts/pack/macos/copaw_dev.spec

import sys
from pathlib import Path

from PyInstaller.utils.hooks import collect_all, collect_submodules

REPO_ROOT = Path.cwd().resolve()
_SPEC_DIR = REPO_ROOT / "scripts" / "pack" / "macos"
_PACK_DIR = REPO_ROOT / "scripts" / "pack"
sys.path.insert(0, str(_PACK_DIR))

from pyi_project_deps import (
    get_collect_packages_from_installed,
    get_pathex_extensions,
    get_pyproject_dep_import_names,
)

CONSOLE_STATIC = REPO_ROOT / "src" / "copaw" / "console"
MD_FILES_SRC = REPO_ROOT / "src" / "copaw" / "agents" / "md_files"
SKILLS_SRC = REPO_ROOT / "src" / "copaw" / "agents" / "skills"
TOKENIZER_SRC = REPO_ROOT / "src" / "copaw" / "tokenizer"
_DEV_LAUNCHER = _PACK_DIR / "dev_gui_launcher.py"

_console_datas = (
    [(str(CONSOLE_STATIC), "copaw/console")] if CONSOLE_STATIC.is_dir() else []
)
_md_datas = (
    [(str(MD_FILES_SRC), "copaw/agents/md_files")] if MD_FILES_SRC.is_dir() else []
)
_skills_datas = (
    [(str(SKILLS_SRC), "copaw/agents/skills")] if SKILLS_SRC.is_dir() else []
)
_tokenizer_datas = (
    [(str(TOKENIZER_SRC), "copaw/tokenizer")] if TOKENIZER_SRC.is_dir() else []
)

# Collect only real packages to avoid PyInstaller "skipping ... as not a
# package" warnings. Pyproject deps (e.g. reme) always added as hidden if
# import/collect fails.
_pyproject_deps = set(get_pyproject_dep_import_names())
_dep_datas = []
_dep_binaries = []
_dep_hidden = []
for _pkg in get_collect_packages_from_installed():
    try:
        _mod = __import__(_pkg)
        if not getattr(_mod, "__path__", None):
            _dep_hidden.append(_pkg)
            continue
    except Exception:
        if _pkg in _pyproject_deps:
            _dep_hidden.append(_pkg)
        pass
    try:
        _d, _b, _h = collect_all(_pkg)
        _dep_datas += _d or []
        _dep_binaries += _b or []
        _dep_hidden.extend(_h or [])
    except Exception:
        if _pkg in _pyproject_deps:
            _dep_hidden.append(_pkg)
        pass

_extra_hidden = [
    "webview",
    "reme",
    "uvicorn.logging",
    "uvicorn.loops",
    "uvicorn.loops.auto",
    "uvicorn.protocols",
    "uvicorn.protocols.http",
    "uvicorn.protocols.http.auto",
    "uvicorn.protocols.websockets",
    "uvicorn.protocols.websockets.auto",
    "uvicorn.lifespan",
    "uvicorn.lifespan.on",
    "fastapi.staticfiles",
    "starlette.staticfiles",
    "pydantic_core",
    "httpx",
    "ollama",
    "dingtalk_stream",
    "lark_oapi",
]
try:
    __import__("pyobjc")
    _extra_hidden.append("pyobjc")
except Exception:
    pass

_BASE_PATHEX = [str(REPO_ROOT), str(REPO_ROOT / "src"), str(_PACK_DIR)]
a = Analysis(
    [str(_DEV_LAUNCHER)],
    pathex=_BASE_PATHEX + get_pathex_extensions(),
    binaries=_dep_binaries,
    datas=(
        _console_datas + _md_datas + _skills_datas + _tokenizer_datas + _dep_datas
    ),
    hiddenimports=(
        collect_submodules("copaw") + _dep_hidden + _extra_hidden
    ),
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
)

pyz = PYZ(a.pure)
exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="CoPaw-Dev",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
coll = COLLECT(
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=False,
    upx=False,
    upx_exclude=[],
    name="CoPaw-Dev",
)

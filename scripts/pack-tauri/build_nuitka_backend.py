# -*- coding: utf-8 -*-
"""Build and stage the experimental Nuitka desktop backend."""

from __future__ import annotations

import argparse
import importlib.util
import shutil
import subprocess
import sys
from importlib.metadata import PackageNotFoundError, distribution
from pathlib import Path
from typing import Sequence


_DYNAMIC_PACKAGES = (
    "qwenpaw.agents.acp",
    "qwenpaw.app.channels",
    "qwenpaw.backup",
    "qwenpaw.pawapp",
    "qwenpawmail_mcp",
)

_DATA_PACKAGES = (
    "agentscope",
    "qwenpaw",
    "reme",
    "whisper",
)

_METADATA_PACKAGES = (
    "agentscope",
    "agentscope-runtime",
    "anthropic",
    "anyio",
    "fastmcp",
    "httpcore",
    "httpx",
    "huggingface_hub",
    "mcp",
    "modelscope",
    "openai",
    "openai-codex",
    "openai-codex-cli-bin",
    "openai-whisper",
    "pydantic",
    "pydantic-core",
    "pydantic-settings",
    "qoder-agent-sdk",
    "qwenpaw",
    "sniffio",
    "starlette",
    "tiktoken",
    "uvicorn",
)


def _distribution_is_installed(distribution_name: str) -> bool:
    """Return whether optional runtime metadata exists in the build env."""
    try:
        distribution(distribution_name)
    except PackageNotFoundError:
        return False
    return True


def build_command(
    python_executable: Path,
    repo_root: Path,
    output_root: Path,
) -> list[str]:
    """Return the standalone Nuitka command for the desktop backend."""
    report_path = output_root / "nuitka-compilation-report.xml"
    command = [
        str(python_executable),
        "-m",
        "nuitka",
        "--mode=standalone",
        "--assume-yes-for-downloads",
        "--remove-output",
        "--jobs=2",
        f"--output-dir={output_root}",
        "--output-filename=qwenpaw-backend",
        f"--report={report_path}",
        "--report-diffable",
        (
            f"--include-data-dir={repo_root / 'console' / 'dist'}="
            "qwenpaw/console"
        ),
    ]
    if sys.platform == "win32":
        command.append("--windows-console-mode=attach")
    for package_name in _DYNAMIC_PACKAGES:
        command.append(f"--include-package={package_name}")
    for package_name in _DATA_PACKAGES:
        command.append(f"--include-package-data={package_name}")
    for package_name in _METADATA_PACKAGES:
        if _distribution_is_installed(package_name):
            command.append(
                f"--include-distribution-metadata={package_name}",
            )
    command.append(str(repo_root / "src" / "qwenpaw" / "tauri" / "entry.py"))
    return command


def find_standalone_dir(output_root: Path) -> Path:
    """Return the single standalone directory produced by Nuitka."""
    candidates = sorted(output_root.glob("*.dist"))
    if len(candidates) != 1:
        rendered = ", ".join(str(path) for path in candidates)
        raise RuntimeError(
            f"Expected one Nuitka standalone directory, found: {rendered}",
        )
    return candidates[0]


def _package_dir(package_name: str) -> Path:
    """Resolve an installed package directory without importing it."""
    spec = importlib.util.find_spec(package_name)
    if spec is None or not spec.submodule_search_locations:
        raise RuntimeError(f"Installed package not found: {package_name}")
    return Path(next(iter(spec.submodule_search_locations)))


def stage_external_tools(bundle_dir: Path) -> None:
    """Copy SDK-owned executables that Nuitka treats as non-code files."""
    qoder_source = _package_dir("qoder_agent_sdk") / "_bundled"
    qoder_target = bundle_dir / "qoder_agent_sdk" / "_bundled"
    shutil.copytree(qoder_source, qoder_target, dirs_exist_ok=True)

    codex_source = _package_dir("codex_cli_bin")
    codex_target = bundle_dir / "codex_cli_bin"
    for name in (
        "bin",
        "codex-path",
        "codex-resources",
    ):
        shutil.copytree(
            codex_source / name,
            codex_target / name,
            dirs_exist_ok=True,
        )
    shutil.copy2(
        codex_source / "codex-package.json",
        codex_target / "codex-package.json",
    )


def stage_bundle(bundle_dir: Path, destination: Path) -> None:
    """Stage the Nuitka directory under the existing Tauri resource path."""
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(bundle_dir, destination)
    executable_name = (
        "qwenpaw-backend.exe" if sys.platform == "win32" else "qwenpaw-backend"
    )
    executable = destination / executable_name
    model_catalog = (
        destination / "qwenpaw" / "providers" / "data" / "model_catalog.json"
    )
    console_index = destination / "qwenpaw" / "console" / "index.html"
    for required_path in (executable, model_catalog, console_index):
        if not required_path.exists():
            raise RuntimeError(f"Nuitka output is missing: {required_path}")
    if sys.platform != "win32":
        executable.chmod(executable.stat().st_mode | 0o111)


def build(
    repo_root: Path,
    output_root: Path,
    destination: Path,
) -> None:
    """Compile, validate, and stage the standalone backend."""
    console_index = repo_root / "console" / "dist" / "index.html"
    if not console_index.is_file():
        raise RuntimeError(f"Console build not found: {console_index}")
    output_root.mkdir(parents=True, exist_ok=True)
    command = build_command(
        Path(sys.executable),
        repo_root,
        output_root,
    )
    subprocess.run(command, cwd=repo_root, check=True)
    bundle_dir = find_standalone_dir(output_root)
    stage_external_tools(bundle_dir)
    stage_bundle(bundle_dir, destination)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    """Run the experimental Nuitka build."""
    args = parse_args(argv)
    build(
        args.repo_root.resolve(),
        args.output_root.resolve(),
        args.destination.resolve(),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

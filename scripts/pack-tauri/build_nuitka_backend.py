# -*- coding: utf-8 -*-
"""Build and stage the experimental Nuitka desktop backend."""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import subprocess
import sys
from importlib.metadata import PackageNotFoundError, distribution
from pathlib import Path
from typing import Sequence


_DYNAMIC_PACKAGES = (
    "qwenpaw",
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
    """Return the Nuitka command for the backend and CLI multidist."""
    report_path = output_root / "nuitka-compilation-report.xml"
    mode = "app" if sys.platform == "darwin" else "standalone"
    command = [
        str(python_executable),
        "-m",
        "nuitka",
        f"--mode={mode}",
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
    entries_dir = repo_root / "scripts" / "pack-tauri" / "nuitka_entries"
    command.extend(
        (
            f"--main={entries_dir / 'qwenpaw-backend.py'}",
            f"--main={entries_dir / 'qwenpaw.py'}",
        ),
    )
    return command


def find_bundle_dir(output_root: Path) -> Path:
    """Return the single platform bundle produced by Nuitka."""
    pattern = "*.app" if sys.platform == "darwin" else "*.dist"
    candidates = sorted(output_root.glob(pattern))
    if len(candidates) != 1:
        rendered = ", ".join(str(path) for path in candidates)
        raise RuntimeError(
            f"Expected one Nuitka {pattern} bundle, found: {rendered}",
        )
    return candidates[0]


def bundle_content_dir(bundle_dir: Path) -> Path:
    """Return the directory containing executables and packaged modules."""
    if bundle_dir.suffix == ".app":
        return bundle_dir / "Contents" / "MacOS"
    return bundle_dir


def ensure_multidist_entrypoints(content_dir: Path) -> tuple[Path, Path]:
    """Create the CLI alias used by Nuitka multidist dispatch."""
    suffix = ".exe" if sys.platform == "win32" else ""
    backend = content_dir / f"qwenpaw-backend{suffix}"
    cli = content_dir / f"qwenpaw{suffix}"
    if not backend.is_file():
        raise RuntimeError(f"Nuitka backend executable is missing: {backend}")
    if not cli.exists():
        shutil.copy2(backend, cli)
    if sys.platform != "win32":
        for executable in (backend, cli):
            executable.chmod(executable.stat().st_mode | 0o111)
    return backend, cli


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


def stage_bundle(
    bundle_dir: Path,
    destination: Path,
) -> tuple[Path, Path]:
    """Stage the Nuitka bundle and return both executable paths."""
    if destination.exists():
        shutil.rmtree(destination)
    if bundle_dir.suffix == ".app":
        staged_bundle = destination / bundle_dir.name
        destination.mkdir(parents=True)
        shutil.copytree(bundle_dir, staged_bundle)
    else:
        staged_bundle = destination
        shutil.copytree(bundle_dir, staged_bundle)
    content_dir = bundle_content_dir(staged_bundle)
    backend, cli = ensure_multidist_entrypoints(content_dir)
    required_names = ("model_catalog.json", "index.html")
    for required_name in required_names:
        if not any(destination.rglob(required_name)):
            raise RuntimeError(
                f"Nuitka output is missing required data: {required_name}",
            )
    return backend, cli


def write_manifest(
    repo_root: Path,
    output_root: Path,
    backend: Path,
    cli: Path,
) -> None:
    """Write stable repo-relative executable paths for CI consumers."""
    payload = {
        "backend": backend.relative_to(repo_root).as_posix(),
        "cli": cli.relative_to(repo_root).as_posix(),
    }
    manifest_path = output_root / "staged-bundle.json"
    manifest_path.write_text(
        f"{json.dumps(payload, indent=2, sort_keys=True)}\n",
        encoding="utf-8",
    )


def build(
    repo_root: Path,
    output_root: Path,
    destination: Path,
) -> None:
    """Compile, validate, and stage the backend and CLI multidist."""
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
    bundle_dir = find_bundle_dir(output_root)
    content_dir = bundle_content_dir(bundle_dir)
    ensure_multidist_entrypoints(content_dir)
    stage_external_tools(content_dir)
    backend, cli = stage_bundle(bundle_dir, destination)
    write_manifest(repo_root, output_root, backend, cli)


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

"""Measure retained heartbeat SSE memory on Windows."""

from __future__ import annotations

import argparse
import asyncio
import base64
import gc
import importlib.util
import json
import sys
import time
import types
from pathlib import Path

import psutil


IMAGE_COUNT = 6
IMAGE_BYTES = 2 * 1024 * 1024
HEARTBEAT_COUNT = 160


def _load_runtime(repo_root: Path) -> tuple[type, types.ModuleType]:
    """Load the real Envelope without importing optional app dependencies."""
    source_root = repo_root / "src" / "qwenpaw"

    package = types.ModuleType("qwenpaw")
    package.__path__ = [str(source_root)]
    sys.modules["qwenpaw"] = package

    runtime_package = types.ModuleType("qwenpaw.runtime")
    runtime_package.__path__ = [str(source_root / "runtime")]
    sys.modules["qwenpaw.runtime"] = runtime_package

    schemas_spec = importlib.util.spec_from_file_location(
        "qwenpaw.schemas",
        source_root / "schemas.py",
    )
    assert schemas_spec is not None and schemas_spec.loader is not None
    schemas = importlib.util.module_from_spec(schemas_spec)
    sys.modules["qwenpaw.schemas"] = schemas
    schemas_spec.loader.exec_module(schemas)

    convert = types.ModuleType("qwenpaw.runtime.message_convert")
    convert._media_type_to_block_type = lambda value: value
    sys.modules["qwenpaw.runtime.message_convert"] = convert

    envelope_spec = importlib.util.spec_from_file_location(
        "qwenpaw.runtime.envelope",
        source_root / "runtime" / "envelope.py",
    )
    assert envelope_spec is not None and envelope_spec.loader is not None
    envelope_module = importlib.util.module_from_spec(envelope_spec)
    sys.modules["qwenpaw.runtime.envelope"] = envelope_module
    envelope_spec.loader.exec_module(envelope_module)
    return envelope_module.Envelope, schemas


def _build_envelope(envelope_type: type, schemas: types.ModuleType):
    """Build a response containing realistic frozen screenshot results."""
    envelope = envelope_type(session_id="windows-memory-benchmark")
    encoded = base64.b64encode(b"x" * IMAGE_BYTES).decode("ascii")

    for index in range(IMAGE_COUNT):
        output = json.dumps(
            [
                {
                    "type": "data",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": encoded,
                    },
                },
            ],
        )
        content = schemas.DataContent(
            data={
                "call_id": f"desktop-{index}",
                "name": "desktop_screenshot",
                "output": output,
            },
            delta=False,
            index=0,
        )
        message = schemas.Message(
            type=schemas.MessageType.PLUGIN_CALL_OUTPUT,
            role=schemas.Role.TOOL,
            content=[content],
            status=schemas.RunStatus.Completed,
        )
        envelope._response.output.append(message)

    envelope._response.status = schemas.RunStatus.InProgress
    return envelope


def _memory_mib(process: psutil.Process) -> dict[str, float]:
    info = process.memory_info()
    values = {"working_set": info.rss / 1024 / 1024}
    private = getattr(info, "private", None)
    if private is not None:
        values["private"] = private / 1024 / 1024
    return values


async def _lightweight_event(envelope):
    async for event in envelope.heartbeat():
        return event
    raise AssertionError("heartbeat emitted no event")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("full", "light"))
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    envelope_type, schemas = _load_runtime(repo_root)
    envelope = _build_envelope(envelope_type, schemas)
    process = psutil.Process()

    if args.mode == "full":
        event = envelope._response
    else:
        event = asyncio.run(_lightweight_event(envelope))

    sample = f"data: {event.model_dump_json()}\n\n"
    del sample
    gc.collect()
    before = _memory_mib(process)

    started = time.perf_counter()
    buffer = []
    for _ in range(HEARTBEAT_COUNT):
        buffer.append(f"data: {event.model_dump_json()}\n\n")
    elapsed = time.perf_counter() - started
    after = _memory_mib(process)

    event_bytes = len(buffer[0].encode("utf-8"))
    print(f"mode={args.mode}")
    print(f"images={IMAGE_COUNT}")
    print(f"raw_image_mib={IMAGE_BYTES / 1024 / 1024:.1f}")
    print(f"heartbeats={HEARTBEAT_COUNT}")
    print(f"equivalent_minutes={HEARTBEAT_COUNT * 25 / 60:.2f}")
    print(f"event_mib={event_bytes / 1024 / 1024:.4f}")
    print(f"buffer_gib={sum(map(len, buffer)) / 1024 / 1024 / 1024:.4f}")
    print(f"elapsed_seconds={elapsed:.3f}")
    for key in before:
        print(f"{key}_before_mib={before[key]:.1f}")
        print(f"{key}_after_mib={after[key]:.1f}")
        print(f"{key}_delta_mib={after[key] - before[key]:.1f}")


if __name__ == "__main__":
    main()

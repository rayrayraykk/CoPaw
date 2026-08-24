"""Measure same-session media and thinking memory on Windows."""

from __future__ import annotations

import argparse
import base64
import gc
import json
import os
import time
import tracemalloc

import psutil


MIB = 1024 * 1024


def _memory_mib() -> dict[str, float]:
    info = psutil.Process(os.getpid()).memory_info()
    values = {"working_set": info.rss / MIB}
    private = getattr(info, "private", None)
    if private is not None:
        values["private"] = private / MIB
    return values


def _print_memory(prefix: str, values: dict[str, float]) -> None:
    for key, value in values.items():
        print(f"{prefix}_{key}_mib={value:.1f}")


def _media_request(multimodal: bool) -> None:
    turns = 15
    images_per_turn = 4
    image_count = turns * images_per_turn
    encoded = base64.b64encode(b"x" * MIB).decode("ascii")
    baseline = _memory_mib()
    tracemalloc.start()
    max_memory = dict(baseline)

    for turn in range(1, turns + 1):
        messages = []
        for index in range(turn * images_per_turn):
            content = [{"type": "text", "text": f"shot {index}"}]
            if multimodal:
                content.append(
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": f"data:image/png;base64,{encoded}",
                        },
                    },
                )
            messages.append({"role": "user", "content": content})
        body_text = json.dumps(
            {"model": "deepseek-ai/DeepSeek-V4-Pro", "messages": messages},
        )
        body_bytes = body_text.encode("utf-8")
        current = _memory_mib()
        for key, value in current.items():
            max_memory[key] = max(max_memory[key], value)
        print(
            f"turn={turn} images={len(messages)} "
            f"body_mib={len(body_bytes) / MIB:.1f} "
            f"working_set_mib={current['working_set']:.1f}",
        )
        del body_bytes
        del body_text
        del messages
        gc.collect()

    retained = _memory_mib()
    traced_current, traced_peak = tracemalloc.get_traced_memory()
    print(f"mode=media multimodal={multimodal}")
    print(f"images={image_count}")
    _print_memory("baseline", baseline)
    _print_memory("peak", max_memory)
    _print_memory("retained", retained)
    for key in baseline:
        print(
            f"peak_{key}_delta_mib="
            f"{max_memory[key] - baseline[key]:.1f}",
        )
        print(
            f"retained_{key}_delta_mib="
            f"{retained[key] - baseline[key]:.1f}",
        )
    print(f"traced_current_mib={traced_current / MIB:.1f}")
    print(f"traced_peak_mib={traced_peak / MIB:.1f}")


def _thinking(use_fragments: bool) -> None:
    turns = 3
    deltas = 100_000
    baseline = _memory_mib()
    tracemalloc.start()
    max_memory = dict(baseline)
    started = time.perf_counter()

    for turn in range(1, turns + 1):
        if use_fragments:
            state: dict[str, object] = {"fragments": []}
        else:
            state = {"text": ""}
        turn_started = time.perf_counter()
        for index in range(deltas):
            delta = f"{index % 10_000:04d}"
            if use_fragments:
                fragments = state["fragments"]
                assert isinstance(fragments, list)
                fragments.append(delta)
            else:
                text = state["text"]
                assert isinstance(text, str)
                state["text"] = text + delta
            if index % 5_000 == 0:
                current = _memory_mib()
                for key, value in current.items():
                    max_memory[key] = max(max_memory[key], value)
        if use_fragments:
            fragments = state["fragments"]
            assert isinstance(fragments, list)
            final_text = "".join(fragments)
        else:
            final_text = state["text"]
            assert isinstance(final_text, str)
        print(
            f"turn={turn} final_mib={len(final_text) / MIB:.3f} "
            f"elapsed_seconds={time.perf_counter() - turn_started:.3f}",
        )
        del final_text
        del state
        gc.collect()

    retained = _memory_mib()
    traced_current, traced_peak = tracemalloc.get_traced_memory()
    print(f"mode=thinking fragments={use_fragments}")
    print(f"deltas_per_turn={deltas}")
    print(f"total_elapsed_seconds={time.perf_counter() - started:.3f}")
    _print_memory("baseline", baseline)
    _print_memory("peak", max_memory)
    _print_memory("retained", retained)
    for key in baseline:
        print(
            f"peak_{key}_delta_mib="
            f"{max_memory[key] - baseline[key]:.1f}",
        )
        print(
            f"retained_{key}_delta_mib="
            f"{retained[key] - baseline[key]:.1f}",
        )
    print(f"traced_current_mib={traced_current / MIB:.1f}")
    print(f"traced_peak_mib={traced_peak / MIB:.1f}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "mode",
        choices=(
            "media-multimodal",
            "media-text-only",
            "thinking-concat",
            "thinking-fragments",
        ),
    )
    mode = parser.parse_args().mode
    if mode == "media-multimodal":
        _media_request(True)
    elif mode == "media-text-only":
        _media_request(False)
    elif mode == "thinking-concat":
        _thinking(False)
    else:
        _thinking(True)


if __name__ == "__main__":
    main()

"""Run original Codex discovery with an isolated host environment."""

import importlib.util
import json
import os
import sys
from pathlib import Path
from unittest.mock import patch


def discover(fixture):
    """Resolve real fixture files without starting any executable."""
    source = (
        Path(__file__).resolve().parents[2]
        / f"src/qwenpaw/harnesses/codex/discovery.py"
    )
    spec = importlib.util.spec_from_file_location(
        f"original_codex_discovery", source
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    environment = dict(fixture[f"environment"])
    environment[f"HOME"] = fixture[f"home"]
    environment[f"USERPROFILE"] = fixture[f"home"]
    os.chdir(fixture[f"cwd"])
    with patch.dict(os.environ, environment, clear=True):
        candidates = module.default_install_candidates(
            Path(fixture[f"home"]), environ=environment
        )
        # Supplying a missing candidate prevents importing the optional SDK.
        bundled = fixture.get(f"bundled") or str(
            Path(fixture[f"home"]) / f"missing-bundled-candidate"
        )
        result = module.resolve_codex_binary_info(
            fixture.get(f"configured"),
            sdk_candidate=Path(bundled),
            install_candidates=candidates,
            environ=environment,
        )
    if result is None:
        return None
    return {f"path": str(result.path), f"source": result.source}


if __name__ == f"__main__":
    print(json.dumps(discover(json.loads(sys.argv[1]))))

"""Execute the original catalog in an isolated temporary data directory."""

import importlib.util
import json
import sys
import tempfile
import types
from pathlib import Path


def load_catalog():
    """Load real definitions without running product initialization."""
    source = Path(__file__).resolve().parents[2] / f"src/qwenpaw"
    for name in (
        f"qwenpaw", f"qwenpaw.config", f"qwenpaw.config.utils",
        f"qwenpaw.plugins",
    ):
        module = types.ModuleType(name)
        module.__path__ = []
        sys.modules[name] = module
    for suffix, relative in (
        (f"__version__", f"__version__.py"),
        (f"_version_compat", f"_version_compat.py"),
        (f"plugins.architecture", f"plugins/architecture.py"),
        (f"plugins.download_catalog", f"plugins/download_catalog.py"),
    ):
        name = f"qwenpaw.{suffix}"
        spec = importlib.util.spec_from_file_location(name, source / relative)
        module = importlib.util.module_from_spec(spec)
        sys.modules[name] = module
        spec.loader.exec_module(module)
    catalog = sys.modules[f"qwenpaw.plugins.download_catalog"]
    return catalog


def main():
    """Compare the complete catalog using isolated fixture manifests."""
    fixture = json.loads(sys.argv[1])
    catalog = load_catalog()
    base = fixture[f"base"]
    responses = {
        f"{base}/metadata/index.json": {
            f"products": {f"plugins": {f"index_url": f"/plugins/index.json"}},
        },
        f"{base}/plugins/index.json": fixture[f"index"],
    }
    catalog.PLUGIN_DOWNLOAD_CDN = base
    catalog._fetch_json = responses.__getitem__
    with tempfile.TemporaryDirectory(prefix=f"qwenpaw-catalog-ref-") as d:
        plugins = Path(d) / f"plugins"
        plugins.mkdir()
        for name, manifest in fixture[f"manifests"].items():
            directory = plugins / name
            directory.mkdir()
            (directory / f"plugin.json").write_text(
                json.dumps(manifest), encoding=f"utf-8"
            )
        sys.modules[f"qwenpaw.config.utils"].get_plugins_dir = lambda: plugins
        print(json.dumps(catalog.build_plugin_catalog()))


if __name__ == f"__main__":
    main()

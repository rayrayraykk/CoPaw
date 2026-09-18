"""Compare version decisions with unchanged original catalog functions."""

import json
import sys

from .official_plugin_reference import load_catalog


def main():
    """Evaluate the entire supplied matrix, without network or product init."""
    fixture = json.loads(sys.argv[1])
    catalog = load_catalog()
    versions = fixture[f"versions"]
    results = {
        f"upgrades": [
            [catalog._is_upgrade_available(left, right) for right in versions]
            for left in versions
        ],
        f"compatible": [
            catalog._is_entry_compatible(entry)
            for entry in fixture[f"entries"]
        ],
    }
    print(json.dumps(results))


if __name__ == f"__main__":
    main()

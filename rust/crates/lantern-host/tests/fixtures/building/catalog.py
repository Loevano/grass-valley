"""Refresh the reviewed R12 construction subset from the authoritative Python catalog."""
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import server
import progression


def catalog():
    return {kind: {'level': progression.BUILDINGS.get(kind, {}).get('level', 1),
                   'costs': progression.costs_of(progression.BUILDINGS.get(kind, {'wood': wood})),
                   'decoration': kind in progression.DECORATIONS}
            for kind, wood in server.COSTS.items()}


if __name__ == '__main__':
    (ROOT / 'rust/crates/lantern-host/src/building_catalog.json').write_text(
        json.dumps(catalog(), indent=2, sort_keys=True) + '\n')

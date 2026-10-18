"""Reviewed R12 construction upgrade data from the authoritative Python rules."""
import json
from catalog import ROOT
import server
import progression as p


def catalog():
    return dict(capacity_levels={k:len(p.WORKER_CAPACITIES.get(k,(1,2,3))) for k in server.COSTS},
                hall=[dict(level=u['level'],costs={k:v for k,v in p.costs_of(u).items() if k!='coins'},
                           seconds=u['seconds'],reward=u['coins'],production=u.get('production',{}),
                           processed_food=u.get('processed_food',0)) for u in p.HALL_UPGRADES],
                processed_food=[*(f'smoked_{k}' for k in p.FISH),*(r['output'] for k in ('bakery','butcher') for r in p.PROFESSION_RECIPES[k])],
                equipment={k:dict(building=u['building'],level=u['level'],skill=u.get('skill',''),xp=u.get('xp',0),
                                  requires=u.get('requires'),costs=p.costs_of(u)) for k,u in p.PROFESSION_UPGRADES.items()})


if __name__=='__main__':
    (ROOT/'rust/crates/lantern-host/src/building/upgrade_catalog.json').write_text(json.dumps(catalog(),indent=2,sort_keys=True)+'\n')

# Town and profession progression

Hall levels 1–10 are implemented. Existing towns and already-paid hall upgrades
retain their progress. New industrial branches use the existing worker and storage systems.

## Implemented unlocks

| Hall | Fishing | Farming | Livestock | Woodworking and services |
| --- | --- | --- | --- | --- |
| 1 — Settlement | Hut and rod | Farm and vegetable acres | — | Woodcutter, cottages, storehouses |
| 2 — Hamlet | Nets | Wheat | Barn and chickens → eggs | Improved axe |
| 3 — Village | Boat; durable wool nets | Carrots and fodder | Sheep → wool | Sawpit → planks |
| 4 — Working village | Smokehouse → preserved fish | Mill → flour | Pigs → meat | Workshop → tools and crafted axes |
| 5 — Market town | Fish ponds → breeding | Orchard → fruit | Cows → milk | Improved saw, timber-framed homes, market stall |

Earlier crops and animals remain available and useful. Hall levels allow new
possibilities; profession upgrades additionally require practice and materials.
The highest practice score among residents and the founder qualifies the town.
Practice is saved and gained only through productive work (fishing also learns
from unsuccessful casts when stock and storage are available).

| Upgrade | Hall | Practice | Materials | Benefit |
| --- | --- | --- | --- | --- |
| Improved axe | 2 | 10 woodcutting | 10 wood + 5 coins | +25% requested wood yield |
| Nets | 2 | 10 fishing | 10 wood + 5 coins | +25% requested fish yield |
| Boat | 3 | 20 fishing | 10 planks | Fishing source reach increases from 3 to 6 steps |
| Durable wool nets | 3 | 20 fishing | 5 wool | +25% requested fish yield |
| Crafted axe | 4 | 30 woodcutting | 3 tools | +25% requested wood yield |
| Improved saw | 5 | 20 woodworking | 3 tools | +25% sawpit throughput, with proportional inputs |
| Timber framing | 5 | — | 10 planks | Cottage adds two more population spaces |

These bonuses cannot exceed the available renewable supply. Ordinary building
and settler-capacity upgrades remain separate controls. Profession upgrades take 30 seconds
of builder construction before their benefits activate.

## Hall upgrade requirements

| Target | Development milestone | Materials | Construction | Coin reward |
| --- | --- | --- | --- | --- |
| 2 | Completed cottage, farm or fishing hut near water, and at least 20 food | 40 wood | 30 seconds | 30 |
| 3 | 10 practice in two of woodcutting, farming, fishing, husbandry, gathering | 60 wood | 60 seconds | 40 |
| 4 | Produce 10 planks cumulatively | 80 wood + 10 planks | 90 seconds | 50 |
| 5 | Produce at least one flour, smoked fish, or crafted tool | 80 wood + 15 planks | 120 seconds | 60 |

Choose upgrades manually in the hall panel. A builder must complete construction.
Materials are paid once when starting; rewards arrive once on completion.
Lifetime production milestones survive spending goods. Alternative professions
and processed goods let towns develop different specialties.

## Production and limits

- Each tree holds 8 wood supply and regenerates 0.5 every three seconds.
- Each connected pond holds 12 fish supply and regenerates 3.2 every three
  seconds. All its water tiles, nearby huts, and founders share that stock,
  including between towns. Farming uses water access without consuming fish.
- Berry bushes hold 6 supply and regenerate 0.3 per tick. Herb patches hold 4
  and regenerate 0.2. Existing maps gain patches only on unoccupied tiles.
- Production never removes trees or water permanently. Empty sources slow
  production until supply returns. Inspect sources to see stock and regeneration.
- Worker slots are enforced even for older saves with implicit job assignments.
  New barns, processors, ponds, and orchards start with two worker spaces;
  capacity upgrades can expand these to six. The market starts with one.
- Each farm without acres has one field's output. Acres each allow one base unit
  per tick; several workers cannot multiply the same field's harvest. Existing
  farm/acre yield upgrades still apply. Manure adds 25% to a tended field's yield,
  consuming 0.1 manure per harvest.
- A barn holds four animals per building level, up to twelve. Its animals are
  tended once per tick; extra workers do not duplicate their output.
- Chickens cost 3 coins, sheep 6, pigs 8, and cows 12. Animals consume respectively
  0.15, 0.25, 0.4, or 0.5 fodder per barn cycle; vegetables can substitute.
  Bought and bred animals require 300 seconds of full feeding to mature, with
  growth paused when feeding fails. Adult chickens, sheep, and cows produce 0.25
  eggs, wool, or milk per barn cycle. Animals in older saves remain adults.
- Two adults of the same kind can breed one young animal for 3 fodder and 1 herb,
  within barn capacity. Butchers consume a selected adult and spend 60 seconds
  working to produce raw meat: chicken 2, sheep 4, pig 8, cow 10. Pigs no longer
  supply recurring meat. One animal can be processed per shop at a time; existing
  prepared-meat recipes resume after the batch finishes.
- Goods have individual storage caps matching wood storage. Edible goods also
  count toward the shared Food pantry cap. Food includes the legacy provisions
  reserve; eating, processing, and selling update the pantry and goods together.
- All cycles, regeneration, construction, and practice pause when the host stops
  or another world opens. There is no offline catch-up.

| Workplace | Inputs per worker cycle | Output |
| --- | --- | --- |
| Sawpit | 2 wood | 1 plank |
| Mill | 2 wheat | 1 flour |
| Smokehouse | 2 fish + 0.5 wood | 2 smoked fish (3 food value) |
| Workshop | 2 planks + 1 wood | 1 tool |
| Fish pond | 0.5 fodder | 1 fish |
| Orchard | Worker and available storage | 1 fruit |

Select acres to choose crops. Wheat and fodder are crafting/feed reserves;
vegetables and carrots are edible. Wool supplies durable nets; planks build boats
and processing buildings. Herbs support breeding. At level 5, market stalls sell
surplus goods for coins (tools sell for three, other supported goods for one).
Town owners can create and accept goods or coin offers from their markets.

## Founder activities

Stand within 1.8 horizontal/vertical tile steps of a source. **F** fishes,
**C** cuts wood, and **G** gathers the nearest berry bush or herb patch.
Work repeats once per host tick. Press the same action key again, move, or press
Escape to cancel. Full storage or empty supply pauses work and practice.
Character practice is saved separately from resident skills. Every 20 practice
increases the displayed character skill level and requested yield. F/C/G can be
rebound in Settings; older movement and camera preferences are preserved.

## Implemented late progression

| Hall | Additions | Next hall milestone |
| --- | --- | --- |
| 5 | Tannery, hides from butchery, town trade offers | Produce 5 leather |
| 6 | Mines and visible iron veins | Mine 10 iron ore |
| 7 | Iron smelters | Smelt 10 iron |
| 8 | Wild horse capture, stables, breeding, saddlers, riding | Saddle a settled horse |
| 9 | Steelworks and steel pickaxes | Produce 15 steel and complete a town trade or sell 10 goods |
| 10 | Trade houses and valuable trade crates | Maximum hall level |

Horses settle or grow over 300 fed seconds. A stable houses 4 horses per building
level; two adults can breed. Riding requires a saddle and increases land speed by
75%, with road bonuses also applying. Captured horses follow their founder until
delivered beside a stable. Riding horses can return to a stable for breeding.

Each town's owner creates or accepts offers from their market. Stock and physical
storage are rechecked at acceptance; both sides exchange together and an offer can
only complete once. Trading does not count as producing materials.

Cooperative adventures, combat, magic, and valley projects remain future work.

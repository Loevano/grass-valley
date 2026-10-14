# Artwork improvement reference

[All improvement areas](README.md)

**Confirmed goal:** the game should look nice, support its cozy medieval setting,
and make the world and its rewards appealing. Angled 2D with depth is agreed;
the final choice between pixel art and soft storybook illustration remains open
in [DESIGN.md](DESIGN.md).

## Current foundation and direction

| Area | Current foundation | Improvement direction | Status |
| --- | --- | --- | --- |
| World view | Canvas drawing, angled tile grid, depth-ordered objects | Cohesive scale, silhouettes, lighting, and readable overlaps | Proposed |
| Buildings | Generated geometric shapes, workplace features, and visible upgrades | Make each profession and upgrade recognizable by shape and detail | Proposed |
| Landscape | Biome terrain, trees, water, rocks, crops, and villages | Consistent palettes and a distinct mood for each place | Proposed |
| Characters | Founder marker, workers, locals, outfits, and work animation | Stronger identity through clothing, pose, tools, and expressions | Proposed |
| Interface | Dark green panels, warm gold highlights, SVG icons | Keep information readable and visually connected to the world | Proposed |
| Rewards | Equipment and story keepsakes | Give important rewards memorable shapes and matching icons | Proposed |
| Ambient motion | Existing worker and gameplay animation | Water ripples, smoke, and crop motion are documented as deferred polish | Deferred in DESIGN.md |

Rendering lives in [web/app.js](../web/app.js); interface styling lives in
[web/style.css](../web/style.css). The sketch below is a proposed composition
reference, not a finished asset or a decision about the final art medium.

The enlarged hall, mill, sawpit, and woodworker's workshop keep the established
palette, angled roofs, simple shading, and warm lanterns. Their current refinement
adds connected yards, recognizable work equipment, cut timber, grain bins, and
flour sacks. The sawpit exposes its recessed cutting area and a large toothed saw
with two handles; round logs and square-ended boards mark its input and output.
The workshop has a bench canopy and tool rack, and the hall is wider without
scaling up its doors and windows. Their
existing footprints and visible upgrade conventions remain unchanged.

## Composition sketch

![Proposed artwork reference: an angled cottage, founder, path, and palette, with a diagram of depth layers.](artwork-reference.svg)

| Sketch element | Purpose | Question to settle |
| --- | --- | --- |
| Angled ground tile and cottage | Reference for footprint, roof shape, and scale | How detailed should buildings be at normal zoom? |
| Small founder with a gold marker | Keep the controlled character easy to find | Which outfit shapes and colors distinguish players? |
| Path and restrained landscape detail | Let activity and useful routes remain readable | How much decoration belongs near working buildings? |
| Green, timber, parchment, and lantern gold | Starting palette inspired by the current presentation | Should the final palette be softer, brighter, or more muted? |
| Depth-layer diagram | Keep ground below buildings and actors, with interface above | Which occlusion cases need special treatment? |

## Proposed visual rules

| Rule | Starting direction | Review reference to add |
| --- | --- | --- |
| Silhouette first | Recognize cottage, sawpit, barn, and hall without reading labels. | Side-by-side building silhouettes at normal zoom. |
| Consistent scale | Use doors, characters, tools, and trees as shared scale anchors. | Character beside every building family. |
| Clear lighting | Use one consistent light direction and restrained shadows. | A tile, building, tree, and character under the same light. |
| Readable interaction | Selection, valid placement, and blocked placement use clear shapes as well as color. | Selected, hovered, valid, and invalid versions of one scene. |
| Biome identity | Change vegetation, materials, and palette while retaining a common style. | Forest, swamp, desert, and ice thumbnails. |
| Reward identity | The world object, inventory icon, and cosmetic version share recognizable features. | A lantern reward shown in all three contexts. |
| Detail by zoom | Important shapes remain readable when zoomed out; close views add detail. | Same village at near, normal, and far zoom. |

## Asset and improvement priorities

| Proposed order | Asset group | First reference to produce | Acceptance question |
| --- | --- | --- | --- |
| 1 | Core town and founder | Hall, cottage, woodcutter, sawpit, founder, tree, and road on one sheet | Does the central play scene feel cohesive and readable? |
| 2 | Food and livestock | Farm, barn, bakery, butcher, crops, and animals | Can players identify the chain and its activities? |
| 3 | Locals and story rewards | Moss, Tobin, Flint, keepsakes, and one cosmetic concept | Are characters and rewards memorable? |
| 4 | Biomes and wilderness villages | Small composition for each village | Does each place have identity while belonging to the same world? |
| 5 | Adventure spaces and ambient polish | Quarry scene, courier course, and restrained effect examples | Are hazards clear and motion useful without clutter? |

## Open art decisions

| Decision | Options to compare | Status |
| --- | --- | --- |
| Final medium | Pixel art or soft storybook illustration | Open |
| Amount of stylization | Simple geometric charm or more illustrated detail | Open |
| Character cosmetics | Outfits, color variants, accessories, or a combination | Open |
| Building identity | Biome material variants, upgrade variants, cosmetic variants | Open |
| Animation scope | Refine existing work actions first; choose ambient additions later | Proposed |

Before replacing game assets, compare a small complete scene in the candidate
styles. Keep the chosen reference here so future artwork follows the same decisions.

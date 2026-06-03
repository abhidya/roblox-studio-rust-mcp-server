---
name: asset-driven-game-design
description: >-
  Turn a one-line game prompt into a playable Roblox game whose world is built
  from real Creator Store assets — not hand-rolled parts. Use when the user asks
  to "build a game", "make a prop hunt / obby / tycoon / sim", "fill a map with
  assets", or wants AI to storyboard and assemble a Roblox experience. Searches
  and ranks assets, inspects their real size/orientation, curates a palette per
  themed area, then places them in Studio. Ships a complete prop-hunt recipe.
---

# Asset-Driven Game Design

Build Roblox games where **the world is assembled from real Creator Store
assets**, chosen by ranked search and measured before placement — never
hand-modeled with `Instance.new("Part")` boxes.

## Golden rule: don't hand-roll the world

If you catch yourself writing `Instance.new("Part")` to represent a barrel, a
tree, a desk, or any *thing in the world*, STOP. Things in the world come from
the Creator Store via search. You only hand-write:

- **Game logic** (round system, scoring, state machine) — code, not content.
- **Structural geometry** (flat floors, invisible walls, spawn pads, zone
  triggers) — the stage, not the props.

Everything a player looks at or hides as is an **inspected, placed asset**.

## The two MCP servers you orchestrate

| Server | Role | Key tools |
|--------|------|-----------|
| **Roblox_Studio** (this repo's server, run search-only or full) | discovery | `search_assets` — ranked, multi-category, metadata-rich search |
| **StudioMCP** (official, bundled in Roblox Studio) | build | `insert_from_creator_store`, `execute_luau`, `run_code`, playtest tools |

`search_assets` is the advantage the official MCP lacks: it returns *ranked
candidates with metadata* so you can choose, instead of blindly inserting one.

If `search_assets` is unavailable, fall back to driving discovery through
`execute_luau` + the Toolbox endpoint, but prefer `search_assets`.

## Workflow

### 1. Storyboard into slots
From the prompt, write a short design brief and break the world into **themed
areas**, and each area into **asset slots**. A slot is one concrete thing to
find. Example for "spooky prop hunt":

```
areas:
  - haunted_kitchen:  [ fridge, stove, pots, table, chairs, hanging_lamp ]
  - graveyard:        [ tombstone, dead_tree, fence, lantern, crypt ]
  - attic:            [ crates, old_chair, trunk, cobweb_mesh, dusty_books ]
ambience: [ horror_ambient_loop, wind_audio, creak_sfx ]
```

Each leaf (fridge, tombstone, ...) is a slot the agents will fill.

### 2. Fan out — one agent per slot (parallel)
Spawn agents in parallel (use the Agent tool, multiple in one message). Each
agent runs this loop for its slot:

1. `search_assets(query="<slot> <theme>", extensive=true, max_results=12)`
   — get ranked candidates. Prefer `verified_only=true` for safety.
2. Shortlist the top ~4 by score, **diversity-checked** (don't take 4 from one
   creator pack).
3. **Inspect** each shortlisted asset to learn real size/orientation/issues:
   - If `inspect_asset` exists, call it (no Studio insert needed — cached).
   - Otherwise insert via `execute_luau` `InsertService:LoadAsset(id)`,
     measure `model:GetBoundingBox()` / `GetExtentsSize()`, read `Anchored`,
     count scripts, then `:Destroy()`.
4. Reject assets with disqualifying issues: huge tri-count, contains
   `LocalScript`/`Script` you didn't ask for, absurd scale, unanchored when it
   must be static.
5. `review_asset(asset_id, slot, verdict, rating, notes)` to cache the verdict
   for other agents (if the tool exists).
6. Pick the best survivor; `commit_palette(project, slot, asset_id)`.

### 3. Fan in — let assets reshape the design
Read the committed palette. **Bend the storyboard to what is buildable.** If no
good "crypt" exists but "mausoleum" assets are excellent, rename the area. The
design serves the assets, not the other way around.

### 4. Build in Studio
Drive **StudioMCP**:

1. Build the **stage** with `execute_luau`: flat `Baseplate`-style floors per
   area, invisible walls, spawn locations, zone parts (CanCollide off,
   Transparency 1) used as area triggers.
2. For each committed slot, insert the asset **by id** and seat it using its
   measured bounding box: drop it so its base sits on the floor
   (`PivotTo(CFrame.new(x, floorY + size.Y/2, z))`), face it sensibly, scale
   only if measured scale is wrong.
3. Scatter multiple instances of prop slots across the area with small random
   rotation/position jitter for a natural look — clone the inserted asset, do
   not re-insert each time (saves API calls).

### 5. Wire the game logic
Copy the prop-hunt game logic from `examples/prop-hunt` (round state machine,
hider/seeker assignment, prop-disguise, scoring). This is hand-written code —
it is logic, not content. The areas it references are the asset-built areas.

### 6. Playtest and iterate
Use StudioMCP playtest + `console_output` + `screen_capture` to verify props
load, are correctly scaled, and the round loop runs. Fix placement, re-run.

## Placement math (so assets sit right)

After inserting an asset and knowing `size = boundingBox.ExtentsSize`:

- **Sit on floor:** `y = floorY + size.Y/2` (for an origin-centered model).
- **Don't intersect:** keep a footprint grid of `size.X` × `size.Z` per area;
  place on free cells.
- **Face the player path:** rotate so the asset's long axis is along the walk
  direction; most Creator Store props face +Z.
- **Scale guard:** if `size.Magnitude > 200` or `< 0.5`, the asset is mis-scaled
  — apply `model:ScaleTo(targetMagnitude / size.Magnitude)`.

## Prop-hunt recipe (the worked example)

Goal: a prop-hunt map with 3+ visually distinct themed areas, each filled from
asset search, plus a working round loop.

1. Brief: "Prop hunt across three worlds." Pick 3 contrasting themes (e.g.
   medieval market, sci-fi lab, cozy cabin) so areas look different.
2. Slots per area: ~6 large set pieces + ~10 small hideable props.
3. Hideable props must be **single-model, reasonably sized (1–8 studs),
   anchored-capable, script-free** — enforce in the inspect/reject step. These
   become the disguises players morph into.
4. Build three floor zones side by side with teleport pads between them.
5. Commit palette, place set pieces, scatter props, register every placed prop
   model into a `HideableProps` folder the game logic reads.
6. Game logic (`examples/prop-hunt`): round timer, assign 1+ seekers, hiders
   pick a nearby prop to disguise as, seekers tag, scoring, repeat.

See `examples/prop-hunt/README.md` for the project scaffold and Rojo setup.

## Efficiency rules

- Parallelize slot agents; the shared search/review cache makes this cheap.
- Insert each asset **once**, then `:Clone()` for repeats.
- Reuse `commit_palette` ids across rebuilds so you don't re-search.
- Keep `verified_only=true` for hideable props to avoid griefy/malicious models.

## Definition of done

- 3+ visually distinct areas, each built from real assets (zero placeholder
  parts for props).
- Every placed prop was inspected (known size, no rejected issues).
- A `HideableProps` folder populated with valid disguise models.
- Round loop runs start→finish in playtest with no asset-load errors.

# Asset-Driven Game Design

A standalone MCP capability + a Claude skill that turns a single prompt into a
playable Roblox game whose design is **grounded in assets that actually exist**.

## The idea

> A prompt comes in -> the AI **storyboards a game** -> but the storyboard is
> grounded in assets it can actually find -> multiple agents work in parallel to
> search, vet and curate assets -> the vetted asset palette feeds back into and
> reshapes the design -> then it gets built.

The MCP is the shared, stateful **asset brain** that every parallel agent talks
to. It must:

- **Cache searches** so parallel agents do not re-hit the Toolbox API.
- **Search extensively** (query expansion + pagination), not one shallow call.
- **Return a curated list** (deduped, diverse, ranked) per design slot.
- **Cache agent reviews** so one agent's verdict is reused by every other agent.
- **Collect deep metadata** (size, scale, orientation, issues) by loading assets
  inside Studio.

## Two tiers of metadata (the core insight)

| Tier | Source | Cost | Examples |
|------|--------|------|----------|
| **Catalog** | Toolbox API (`toolbox-service/v2/assets:search`) | cheap, network | votes, creator, verified, script count, triangle summary, category |
| **Geometric** | Loading the asset *into Studio* and measuring | expensive, needs Studio | bounding-box size (studs), pivot/orientation, anchored?, part count, missing textures, issues |

"Size, scaling, orientation, issues" are **not** in the Toolbox API. They only
exist once an asset is loaded and measured. So enrichment is two-stage:
fast catalog search -> curated shortlist -> on-demand geometric inspection of
only the shortlist, cached forever (asset geometry is immutable per asset id).

## Architecture

A small **stateful service** backed by SQLite (`~/.roblox-asset-brain/brain.db`).

```
search_cache:  query_key  -> [ranked asset json]     (TTL ~24h)
asset_record:  asset_id    -> { catalog_json, geometric_json }
asset_review:  asset_id    -> [{ slot, verdict, rating, notes, reviewer }]
palette:       project_id  -> { slot -> asset_id }
```

In-flight identical searches are **coalesced (single-flight)** so N parallel
agents trigger one network storm, not N.

## MCP tool surface

| Tool | Tier | Purpose |
|------|------|---------|
| `search_assets` | catalog | cache-first, single-flight, `extensive` query expansion + pagination |
| `curate_assets` | catalog | brief with slots -> diverse curated shortlist per slot |
| `inspect_asset` | geometric | Studio round-trip: load -> measure size/orientation/issues -> destroy -> cache |
| `review_asset` | review | persist an agent's verdict for reuse |
| `commit_palette` / `get_palette` | palette | freeze chosen asset per slot for the build phase |

All are read-mostly and allowlist-friendly via `tools/search_only_mcp_filter.js`.

## End-to-end workflow (skill + agents)

```
1. Prompt -> storyboard with explicit asset SLOTS
   ("forest village": ground texture, 3 buildings, ambient audio, NPC, hero tool, skybox)

2. FAN-OUT: one agent per slot, in parallel. Each agent:
   a. search_assets(slot, extensive=true)   <- cache-coalesced
   b. curate_assets -> shortlist of ~5
   c. inspect_asset on each                  <- cached geometric meta + issues
   d. review_asset with verdict + notes      <- shared review cache
   e. commit_palette(slot, best id)

3. FAN-IN: real assets reshape the storyboard. No good "dragon" but great
   "wyvern" assets -> the narrative bends to what is buildable. Asset-driven.

4. BUILD: insert committed ids, using cached size/orientation to scale and seat
   them correctly (no guessing - metadata says 8x8x12 studs, +Z facing).
```

## Worked example: prop-hunt with themed areas

The skill ships a worked example: a prop-hunt map whose distinct areas
(e.g. medieval market, sci-fi lab, cozy kitchen) are each generated from
asset searches for that theme, so the props players hide as are real,
inspected, correctly-scaled Creator Store assets.

## Constraints

- Geometric metadata requires Studio open; `inspect_asset` degrades to
  `geometric: pending` when no Studio is connected.
- Extensive search needs a rate limiter + the single-flight coalescing or the
  Toolbox API throttles.
- Loaded assets may carry scripts: load into a non-running container, never
  execute, flag scripts as an issue, destroy immediately.
- Catalog/votes drift -> TTL. Geometry is immutable -> cache forever.

# Prop Hunt — asset-driven example

A complete prop-hunt game where the **logic is hand-written** (this folder) and
the **world is assembled from real Creator Store assets** by the
`asset-driven-game-design` skill. You never model a single prop by hand.

## What's in here (the logic, not the content)

```
default.project.json      Rojo project — fixes "no project file found"
src/shared/Config.luau     round timings, ratios, prop folder name
src/shared/Remotes.luau    RemoteEvent setup/accessors
src/server/init.server.luau  round state machine, disguise, tagging, scoring
src/client/init.client.luau  HUD + "press E to hide as <prop>" input
```

The server reads disguise props from `Workspace.HideableProps` — a folder the
asset-driven build fills with inspected, valid prop models.

## Setup on your Mac

### 1. Fix the Rojo error and serve
```bash
cd ~/Documents/Claude/Projects/RobloxAIGameDev
# copy this example in (or start here directly)
cp -R /path/to/roblox-studio-rust-mcp-server/examples/prop-hunt/* .
rojo serve            # now finds default.project.json
```
In Studio: install the Rojo plugin, click **Connect**. The logic scripts sync
into ServerScriptService / ReplicatedStorage / StarterPlayer.

### 2. Connect the MCP servers
- **Official build MCP** is already at
  `/Applications/RobloxStudio.app/Contents/MacOS/StudioMCP`.
- **This repo's search MCP** — build and register it:
  ```bash
  cd /path/to/roblox-studio-rust-mcp-server
  cargo build --release
  claude mcp add --transport stdio Roblox_Studio -- \
    "$(pwd)/target/release/rbx-studio-mcp" --stdio
  ```
  (Or run search-only: `node tools/search_only_mcp_filter.js`.)

### 3. Build the world with the skill
In Claude Code (with both MCPs connected and Studio open):
```
/asset-driven-game-design build a 3-theme prop hunt
  (medieval market, sci-fi lab, cozy cabin)
```
The skill storyboards slots, fans out parallel agents to search/inspect/curate
assets, places them in Studio, and registers props into `Workspace.HideableProps`.

### 4. Playtest
Press **Play**. Hiders press **E** near a prop to disguise; seekers tag them.

## Why props come from search, not `Instance.new`

Hand-rolled boxes make a prop hunt unplayable — players need recognisable,
varied objects to hide as. The skill enforces that every prop is a real asset,
inspected for size/scale/scripts, so disguises look right and behave safely.

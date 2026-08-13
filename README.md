# skls

[English](README.md) | [日本語](README.ja.md)

[![crates.io](https://img.shields.io/crates/v/skls.svg)](https://crates.io/crates/skls)
[![docs.rs](https://docs.rs/skls/badge.svg)](https://docs.rs/skls)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/malanjp/skls)](https://github.com/malanjp/skls/releases)

**skls** (*skills list*) — a TUI for managing agent skills across Cursor, Claude Code, Codex, and 20+ other hosts.

See which skills are installed, where they apply, and whether they are used — then add, delete, or update them from the same screen.

![skls TUI](docs/images/skls.png)

## Features

- **Inventory**: 27 agent hosts (Cursor loads `~/.agents/skills` too). Scans skills bundled in Claude Code / Cursor / Codex / agents plugins. Filter by project / user scope and agent; multi-select for bulk actions
- **Views**: `t` cycles **skills → plugins → MCP**. Plugin packages and bundled MCP servers (`mcp.json`) show in the same TUI
- **Metrics**: Compute activation rate from conversation logs and show a delete-recommendation score (`delete_score`)
- **Add**: Skills via `gh skill` / `npx skills`. Plugins via `claude plugin` / `copilot plugin` / `codex plugin` (Cursor has no catalog CLI)
- **Delete**: Remove inventory paths (confirmation required). npx-sourced skills also run `npx skills remove`. Plugin delete uses the host catalog CLI first. Warns on shared-store and plugin paths
- **Update**: Skills via `gh skill` / `npx skills`. Plugins via the same catalog CLIs (Codex re-runs `plugin add`)

## Dependencies

| Required | Optional |
|----------|----------|
| Rust 1.85+ (edition 2024) | [`gh`](https://cli.github.com/) (`gh skill`) |
| | Node.js / `npx` ([`npx skills`](https://skills.sh/)) |
| | `claude` / `copilot` / `codex` (plugin catalog add / update / delete) |

Listing and activation metrics work without `gh` / `npx` / plugin CLIs. Only operations that need a missing CLI are disabled.

## Install / run

From [crates.io](https://crates.io/crates/skls):

```bash
cargo install skls
skls
```

From source:

```bash
git clone https://github.com/malanjp/skls.git
cd skls
cargo install --path .
# or: cargo run --release
```

### CLI options

```bash
skls                                 # use cwd as project root
skls --project-root /path/to/repo
skls --window-days 30                # activation window (days)
skls --max-sessions 200              # sessions read per agent (default 80)
skls --max-bytes 1048576             # max bytes per session file (default 256KiB)
skls --full-scan                     # no session / byte caps (slow on large log trees)
skls --dump-json                     # print inventory JSON: { skills, plugins, mcp_servers }
```

On startup the skill list appears first, then activation is sampled. With defaults, the list usually shows within about 1–2 seconds.

## Screen

`t` cycles the list: **skills → plugins → MCP**. List on the left, detail on the right. `[ ]` / `[x]` mark multi-select.

**Skills** columns: `NAME` · `SCOPE` · `SRC` (`plugin` / `gh skill` / `npx skills` / `manual`) · `AUTHOR` · `RATE` · `SCORE`. Default sort is `delete_score` descending (higher = stronger delete candidate). `S` toggles asc/desc; cycling with `s` resets to that key's default direction. Author comes from the SKILL.md frontmatter, the plugin manifest, or the GitHub owner of the source repo.

**Plugins** columns: `NAME` · `SCOPE` · `MARKET` · `SK` (bundled skills) · `MCP`.

**MCP** columns: `NAME` · `TRANS` (`stdio` / `http` / `sse`) · `PLUGIN` · `AGENTS`. Servers are read from plugin `mcp.json` / `.mcp.json` (Agent Plugins 1.0, plus a looser `command`/`url` form).

`sample:` in the title is the activation analysis cap; `sampled (-N older)` in the status is how many older sessions were skipped.

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` | Move |
| `Ctrl+F` / `PgDn` | Page down (no wrap) |
| `Ctrl+B` / `PgUp` | Page up (no wrap) |
| `gg` / `Home` | Jump to first row |
| `L` / `Ctrl+L` / `End` | Jump to last row |
| `t` | Cycle view (skills → plugins → mcp) |
| `Space` | Toggle row selection |
| `*` | Select / clear all visible |
| `x` | Clear selection |
| `/` | Search name / description |
| `f` | Filter panel |
| `s` | Cycle sort key (`name` → `rate` → `delete_score` → `last_hit` → `author` → `source`); resets direction to that key's default |
| `S` | Toggle sort direction (asc / desc) |
| `a` | Add flow (skills: `gh`/`npx`; plugins: catalog CLI) |
| `d` | Delete confirm (selection if any, else current row) |
| `u` | Update |
| `r` | Light rescan (list only) |
| `R` | Recompute activation stats |
| `?` | Help |
| `q` | Quit |

### Filter (`f`)

| Key | Meaning |
|-----|---------|
| `p` / `u` / `a` | project / user / all scopes |
| `j` / `k` · `Space` | Move / toggle agent filter (empty = all) |
| `*` / `0` | Clear agent filter (all agents) |
| `x` | Clear agent filter |
| `c` | Clear all filters |

### Add (`a`)

On the **skills** view, step through dialogs. `Esc` goes back one step; `q` cancels.

1. Backend: `1`/`g` = `gh skill`, `2`/`n` = `npx skills`
2. Source (gh: search query; npx: `owner/repo` or `owner/repo@skill`)
3. Pick result (gh only)
4. Agents (`j`/`k` move, `Space` toggle, `*` all, `x` none; `Enter` next) → scope (`p`/`u`) to run

On the **plugins** view, enter a catalog spec (`name@marketplace`), pick hosts that have a plugin CLI (`claude-code` / `copilot` / `codex`), then scope. Cursor has no catalog CLI — install from the host marketplace.

On the **MCP** view, `a` / `u` point you at the plugins view (servers are bundled in plugins).

### Delete (`d`)

- Starts with all agents present on the target skills selected
- `j`/`k` move, `Space` toggle, `*` select all, `x` clear all
- `y` / `Enter` confirm; `n` / `q` / `Esc` cancel
- Removes inventory paths first; for `source: npx`, also runs `npx skills remove` per selected agent
- Paths under a shared store (e.g. `~/.agents/skills`) or inside a plugin install produce a warning; duplicate paths are deduped
- After delete, only the list is rescanned. Press `R` to recompute activation

Plugin delete (plugins view, or MCP view via the parent plugin) runs the host catalog CLI first:

| Host | CLI |
|------|-----|
| Claude Code | `claude plugin uninstall SPEC --scope user\|project` |
| Copilot | `copilot plugin uninstall NAME` |
| Codex | `codex plugin remove NAME` |
| Cursor | no CLI — message only; path left in place |

If every CLI call fails, inventory paths are removed as a fallback.

### Update (`u`)

**Skills**

1. Agents: `j`/`k` move, `Space` toggle, `*` all, `x` none; `Enter` next
2. Backend: `1`/`g` = `gh skill`, `2`/`n` = `npx skills` (`Esc` returns to agent selection)
3. When a suggestion exists, `Enter` accepts it
4. If only one CLI is available, the backend picker is skipped

Suggestion priority:

| Signal | Suggested |
|--------|-----------|
| `SKILL.md` has `github-repo` / `github-tree-sha`, or provenance from `gh skill list` | `gh skill` |
| Present in `~/.agents/.skill-lock.json` (or project lock) | `npx skills` |
| Both | prefer `gh skill` |
| Mixed / unknown | no suggestion (pick manually) |

`gh skill update` prefers host dirs with metadata (`.cursor` / `.codex`, …) for `--dir`. `npx skills update` adds `-g` / `-p` from scope.

**Plugins** pick agents then run:

| Host | CLI |
|------|-----|
| Claude Code | `claude plugin update SPEC --scope …` |
| Copilot | `copilot plugin update NAME` |
| Codex | `codex plugin add SPEC` (re-install) |

## Scan roots

| Agent | project | user |
|-------|---------|------|
| cursor | `.agents/skills/` · `.cursor/skills/` | `~/.cursor/skills/` · `~/.agents/skills/` · `~/.cursor/skills-cursor/` |
| claude-code | `.claude/skills/` | `~/.claude/skills/` |
| codex | `.agents/skills/` · `.codex/skills/` | `~/.codex/skills/` |
| gemini-cli | — | `~/.gemini/skills/` |
| antigravity / antigravity-cli / antigravity2.0 | — | `~/.gemini/antigravity{,-cli}/skills/` · `~/.gemini/config/skills/` |
| github-copilot | — | `~/.copilot/skills/` |
| opencode | — | `~/.config/opencode/skills/` |
| pi | — | `~/.pi/agent/skills/` |
| amp / kimi-cli / replit | — | `~/.config/agents/skills/` |
| qwen-code | — | `~/.qwen/skills/` |
| augment / continue / droid / kilo / qoder / roo / trae / codebuddy | — | `~/.<host>/skills/` (droid → `~/.factory/skills/`) |
| grok / warp / devin | — | `~/.grok/skills/` · `~/.warp/skills/` · `~/.config/devin/skills/` |
| cline / warp / universal | — | also linked to shared `~/.agents/skills/` |

The same skill on multiple hosts is merged into one row. Add flow defaults to cursor / claude-code / codex (`*` selects every host).

Extra metadata:

- If `gh skill list --json` is available, attach `source_url` / `version` / `pinned`
- If `~/.agents/.skill-lock.json` exists, set `source: npx` (startup does **not** call `npx skills list`)

Scope vocabulary follows `gh skill` (`project` / `user`). `npx skills -g` is treated as user.

### Plugins

Skills bundled inside agent plugins are attributed to the host that owns the files:

| Store | Path | Attributed to |
|-------|------|---------------|
| Claude Code | `~/.claude/plugins/` (via `installed_plugins.json`, scope from the manifest) | claude-code |
| Cursor | `~/.cursor/plugins/cache/*/*/*/skills/` | cursor |
| Codex | `~/.codex/plugins/cache/*/*/*/skills/` | codex |
| agents | `~/.agents/plugins/` (lenient walk for `skills/` dirs) | the shared-store hosts (cursor / cline / warp / universal) |

Plugin skills are marked `source: plugin`. They are not updated via `gh` / `npx`, and deleting a skill path inside a plugin warns that the path lives inside a plugin install.

Use the **plugins** view (`t`) to add / update / uninstall the package from a catalog instead. Bundled MCP servers (`mcp.json` / `.mcp.json`) appear on the **mcp** view.

## Activation rate and delete score

Parse session logs for the last N days (default 30) and count skill-name or path mentions **uniquely per session**.

| Source | Path |
|--------|------|
| Cursor | `~/.cursor/projects/**/agent-transcripts/**/*.jsonl` |
| Claude Code | `~/.claude/projects/**/*.jsonl` |
| Codex | `~/.codex/sessions/**/rollout-*.jsonl` |

- `activation_rate` = `hits / sessions_total` (N/A when sessions are 0)
- `delete_score` is higher for stronger delete candidates (sum of the points below)
- Detail labels: `keep` (under 35) / `review` (35–59) / `consider delete` (60+)

`delete_score` points (see `src/analytics/score.rs`):

| Factor | Condition | Points |
|--------|-----------|--------|
| Activation rate | no sessions / unknown | +25 |
| | 0% | +40 |
| | under 5% | +30 |
| | under 15% | +15 |
| | under 30% | +5 |
| Last hit | none | +20 |
| | over 60 days | +25 |
| | over 30 days | +15 |
| | over 14 days | +8 |
| Host count | 3+ agents | +10 |
| | 2 agents | +5 |
| Provenance | `manual` and no `source_url` | +5 |
| Hits | `hits == 0` | +10 |

Log matching is heuristic — not a precise “skill execution count”. Use it as a signal before deleting.

For speed, defaults read only the **latest 80 sessions per agent** and **up to 256KiB per file**. Change with `--max-sessions` / `--max-bytes` / `--full-scan`.

## Development

See [AGENTS.md](AGENTS.md) for layout and contributor notes.

```bash
cargo test --lib
cargo run --release -- --dump-json
```

## Changelog

See [CHANGELOG.md](CHANGELOG.md) ([日本語](CHANGELOG.ja.md)).

## License

[MIT](LICENSE)

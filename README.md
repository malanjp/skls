# skls

[English](README.md) | [日本語](README.ja.md)

[![crates.io](https://img.shields.io/crates/v/skls.svg)](https://crates.io/crates/skls)
[![docs.rs](https://docs.rs/skls/badge.svg)](https://docs.rs/skls)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/malanjp/skls)](https://github.com/malanjp/skls/releases)

**skls** (*skills list*) — a TUI for managing agent skills across Cursor, Claude Code, and Codex.

See which skills are installed, where they apply, and whether they are used — then add, delete, or update them from the same screen.

## Features

- **Inventory**: Filter by project / user scope and agent. Multi-select for bulk actions
- **Metrics**: Compute activation rate from conversation logs and show a delete-recommendation score (`delete_score`)
- **Add**: Install with `gh skill` or `npx skills` (chosen each time)
- **Delete**: Prefer paths from the inventory (confirmation required). Warns when a symlink points at a shared real path
- **Update**: Choose `gh skill` / `npx skills`. If provenance can be inferred, Enter accepts the suggestion

## Dependencies

| Required | Optional |
|----------|----------|
| Rust 1.85+ (edition 2024) | [`gh`](https://cli.github.com/) (`gh skill`) |
| | Node.js / `npx` ([`npx skills`](https://skills.sh/)) |

Listing and activation metrics work without `gh` / `npx`. Only operations that need a missing CLI are disabled.

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
skls --dump-json                     # print inventory as JSON (no TUI)
```

On startup the skill list appears first, then activation is sampled. With defaults, the list usually shows within about 1–2 seconds.

## Screen

```
┌ skls  scope:all  agents:all  sort:delete_score  window:30d  sample:≤80sess/256KiB ─┐
│ 170 skills | gh:ok npx:ok | activations ready | sampled (-N older) | /path/to/project   │
├──────────────────────────────────┬──────────────────────────────────────────────────────┤
│ NAME              SCOPE RATE SCORE│ detail                                              │
│ [ ] brand          user  0.0%  85 │ brand                                               │
│ [x] find-skills    user  1.2%  30 │ source: npx / gh …                                  │
│ ...                               │ hits / rate / delete_score / paths …                │
└──────────────────────────────────┴──────────────────────────────────────────────────────┘
│ j/k  Space/* /x select  d/u on selection  / f s a r R ? q                               │
```

List on the left, detail on the right. `[ ]` / `[x]` mark multi-select. Default sort is `delete_score` (higher = stronger delete candidate).

`sample:` in the title is the activation analysis cap; `sampled (-N older)` in the status is how many older sessions were skipped.

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` | Move |
| `Space` | Toggle row selection |
| `*` | Select / clear all visible |
| `x` | Clear selection |
| `/` | Search name / description |
| `f` | Filter panel |
| `s` | Cycle sort (`name` → `rate` → `delete_score` → `last_hit`) |
| `a` | Add flow |
| `d` | Delete confirm (selection if any, else current row) |
| `u` | Update (pick backend; Enter uses suggestion when available) |
| `r` | Light rescan (list only) |
| `R` | Recompute activation stats |
| `?` | Help |
| `q` | Quit |

### Filter (`f`)

| Key | Meaning |
|-----|---------|
| `p` / `u` / `a` | project / user / all scopes |
| `1` / `2` / `3` / `0` | cursor / claude-code / codex / all agents |
| `c` | Clear filters |

### Add (`a`)

Step through dialogs. `Esc` goes back one step; `q` cancels.

1. Backend: `1`/`g` = `gh skill`, `2`/`n` = `npx skills`
2. Source (gh: search query; npx: `owner/repo` or `owner/repo@skill`)
3. Pick result (gh only)
4. Agents (`j`/`k` move, `Space` toggle, `*` all, `x` none; `Enter` next) → scope (`p`/`u`) to run

### Delete (`d`)

- Starts with all agents present on the target skills selected
- `j`/`k` move, `Space` toggle, `*` select all, `x` clear all
- `y` / `Enter` confirm; `n` / `q` / `Esc` cancel
- Symlinks into a shared real path (e.g. `~/.agents/skills`) produce a warning
- After delete, only the list is rescanned. Press `R` to recompute activation

### Update (`u`)

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

```bash
cargo test --lib
cargo run --release -- --dump-json
```

## Changelog

See [CHANGELOG.md](CHANGELOG.md) ([日本語](CHANGELOG.ja.md)).

## License

[MIT](LICENSE)

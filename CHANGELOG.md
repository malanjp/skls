# Changelog

[English](CHANGELOG.md) | [日本語](CHANGELOG.ja.md)

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- User config at `~/.config/skls/config.toml` (`$XDG_CONFIG_HOME/skls/config.toml`). `projects` lists extra project-scope scan roots. `window_days` / `max_sessions` / `max_bytes` supply defaults when CLI flags are omitted. If the file is missing, skls discovers project-scope skill directories under well-known home children (`repos`, `src`, `dev`, `code`, `work`, `projects`, `orca`, `Documents`, `Developer`, `ghq`, `git`) and writes the config. Nothing found → no file (retry next launch). An existing file is never overwritten
- Skills with the same name in different projects are separate rows. The list shows a `PROJECT` column (directory name; user-scope is `-`)
- Left sidebar lists scan-root projects; selecting one filters skills to paths under that root. A non-selectable divider separates source nav from projects. Add / update with project scope still uses cwd / `--project-root`

### Changed

- Skill / plugin / MCP list columns grow with visible content (`NAME` / `PROJECT` / `SRC` / `AUTHOR` and the plugin / MCP equivalents). Leftover width goes to the name column

### Fixed

- Sidebar project counts are cached per view change instead of recomputed every frame. First-run project discovery walks only the seed directories above, not the whole home tree

## [0.5.0] - 2026-08-13

### Highlights

- **Left sidebar** — split the list into manual / gh / npx / plugins / mcp. `h`/`l` (Tab) move focus; `j`/`k` on the sidebar changes category; `t` still cycles. Skills are partitioned by source (manual includes plugin-bundled; gh / npx are those installers)
- **MCP list** — bundled servers from plugin `mcp.json` / `.mcp.json` (Agent Plugins 1.0 and the looser `command`/`url` form)
- **Plugin catalog ops** — add / update / uninstall from `claude plugin`, `copilot plugin`, and `codex plugin`. Cursor has no catalog CLI (install from the host marketplace). Listing still works without these binaries
- **Paging and sort direction** — `Ctrl+F` / `Ctrl+B` (and PageDown / PageUp) move by a page; `gg` / `Home` jump to first, `L` / `Ctrl+L` / `End` to last; `S` toggles asc/desc

### Added

- Left sidebar (`nav`): `manual` / `gh` / `npx` / `plugins` / `mcp`, with counts. `h`/`l` or Tab to focus. Add from the gh / npx nav skips the backend picker
- List views for installed plugin packages and bundled MCP servers. Plugin columns: `NAME` / `SCOPE` / `MARKET` / `SK` / `MCP`. MCP columns: `NAME` / `TRANS` / `PLUGIN` / `AGENTS`
- Plugin add / update / delete via host catalog CLIs (`claude plugin install|update|uninstall`, `copilot plugin install|update|uninstall`, `codex plugin add|remove`). Codex update re-runs `plugin add`. If every uninstall CLI fails, inventory paths are removed as a fallback
- `--dump-json` now prints `{ skills, plugins, mcp_servers }` instead of a bare skills array
- Skill list `SRC` uses `plugin` / `gh skill` / `npx skills` / `manual`. A plugin copy no longer hides `npx skills` / `gh skill` when a lockfile, `~/.agents/skills`, or `gh skill list` says otherwise. Plugin-only paths stay `plugin`
- List paging: `Ctrl+F` / `PageDown` forward, `Ctrl+B` / `PageUp` back (clamped; `j`/`k` still wrap). Step is the visible list height minus one row. `gg` / `Home` jumps to the first row, `L` / `Ctrl+L` / `End` to the last
- `S` toggles sort direction. `s` still cycles the key and resets to that key's default (`delete_score` / `rate` / `last_hit` → desc; `name` / `author` / `source` → asc). Header shows `↑` / `↓`

## [0.4.0] - 2026-08-13

### Highlights

- **Plugin skills** — skills bundled inside agent plugins now show up: Claude Code (`~/.claude/plugins/`, scope from `installed_plugins.json`), Cursor (`~/.cursor/plugins/cache/`), Codex (`~/.codex/plugins/cache/`), and the shared agents store (`~/.agents/plugins/`). Each is attributed to the host that owns the files and marked `source: plugin`
- **Author and source columns** — the list shows `SRC` (gh / npx / plugin / manual) and `AUTHOR`. Author is read from SKILL.md frontmatter, the plugin manifest, or the GitHub owner of the source repo

### Added

- Scan skills bundled inside agent plugins: Claude Code (`~/.claude/plugins/`, scope from `installed_plugins.json`), Cursor (`~/.cursor/plugins/cache/`), Codex (`~/.codex/plugins/cache/`), and the shared agents store (`~/.agents/plugins/`). Plugin skills are marked `source: plugin` and attributed to the host owning the files
- Delete confirms warn when a path lives inside a plugin install; plugin skills are excluded from `gh`/`npx` update suggestions and update dirs
- List shows `SRC` (source: gh / npx / plugin / manual) and `AUTHOR` columns; the detail panel and `--dump-json` include author. Author is read from SKILL.md frontmatter, the plugin manifest, or the GitHub owner of the source repo
- Sort cycle (`s`) extended with `author` (unknowns last) and `source` (gh → npx → plugin → manual)

## [0.3.2] - 2026-08-05

### Fixed

- Skill list column headers (`NAME` / `SCOPE` / `RATE` / `SCORE`) align with row values (checkbox + selection gutter)

### Changed

- README includes a sample TUI screenshot (`docs/images/skls.png`)

## [0.3.1] - 2026-08-04

### Added

- `AGENTS.md` for coding-agent contributors

### Changed

- Refresh `Cargo.lock` (transitive deps)

## [0.3.0] - 2026-08-04

### Highlights

- **See what Cursor actually sees** — inventory now covers `~/.agents/skills` and `~/.cursor/skills-cursor`, so skills that appear in Cursor (e.g. `brand`) show up in skls and can be deleted from the right path
- **27 agent hosts** — beyond Cursor / Claude Code / Codex: gemini-cli, antigravity*, github-copilot, opencode, pi, amp, kimi-cli, replit, qwen-code, augment, continue, droid, kilo, qoder, roo, trae, codebuddy, grok, cline, warp, universal, devin
- **Reliable npx cleanup** — deleting an npx-sourced skill removes filesystem paths *and* runs `npx skills remove` (lockfile / shared store stay consistent)
- **Safer shared-store deletes** — removing `~/.agents/skills/...` warns that other agents may share that path; delete plans dedupe paths across hosts

### Added

- Inventory roots for the hosts listed above (`gh skill` `agentHosts` / common install paths)
- Project scan for `.cursor/skills` and `.codex/skills`

### Fixed

- Cursor user scan missed shared / managed skill trees Cursor loads at runtime
- npx deletes previously skipped `npx skills remove` whenever inventory paths existed (and never passed `npx_available`)

## [0.2.0] - 2026-08-04

### Changed

- Add / delete / update: pick target agents with checkbox toggles (`j`/`k` + `Space`; `*` = all, `x` = none; `Enter` to continue)

## [0.1.0] - 2026-08-04

Initial release of **skls** (*skills list*).

### Added

- TUI inventory of agent skills across Cursor, Claude Code, and Codex
- Filter by project / user scope and agent; search by name / description
- Sort by name, activation rate, `delete_score`, or last hit
- Multi-select (`Space`, `*`, `x`) with bulk delete / update
- Activation metrics from conversation logs (session-unique hits) and `delete_score` recommendations
- Add flow via `gh skill` or `npx skills` (stepped dialogs)
- Delete with confirmation; warns when a symlink points at a shared real path
- Update via `gh skill` / `npx skills`, with provenance-based backend suggestion
- Fast startup: list first, then sampled activation analysis (defaults: 80 sessions/agent, 256KiB/file)
- CLI flags: `--project-root`, `--window-days`, `--max-sessions`, `--max-bytes`, `--full-scan`, `--dump-json`
- Docs: English README (main), Japanese README, MIT license

### Notes

- Log matching is heuristic, not a precise skill-execution count
- Listing and metrics work without `gh` / `npx`; only CLI-dependent actions are disabled

[Unreleased]: https://github.com/malanjp/skls/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/malanjp/skls/releases/tag/v0.5.0
[0.4.0]: https://github.com/malanjp/skls/releases/tag/v0.4.0
[0.3.2]: https://github.com/malanjp/skls/releases/tag/v0.3.2
[0.3.1]: https://github.com/malanjp/skls/releases/tag/v0.3.1
[0.3.0]: https://github.com/malanjp/skls/releases/tag/v0.3.0
[0.2.0]: https://github.com/malanjp/skls/releases/tag/v0.2.0
[0.1.0]: https://github.com/malanjp/skls/releases/tag/v0.1.0

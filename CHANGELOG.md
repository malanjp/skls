# Changelog

[English](CHANGELOG.md) | [日本語](CHANGELOG.ja.md)

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/malanjp/skls/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/malanjp/skls/releases/tag/v0.3.2
[0.3.1]: https://github.com/malanjp/skls/releases/tag/v0.3.1
[0.3.0]: https://github.com/malanjp/skls/releases/tag/v0.3.0
[0.2.0]: https://github.com/malanjp/skls/releases/tag/v0.2.0
[0.1.0]: https://github.com/malanjp/skls/releases/tag/v0.1.0

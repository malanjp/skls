# Changelog

[English](CHANGELOG.md) | [日本語](CHANGELOG.ja.md)

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-08-04

### Added

- Inventory for additional agent hosts (gemini-cli, antigravity*, github-copilot, opencode, pi, amp, kimi-cli, replit, qwen-code, augment, continue, droid, kilo, qoder, roo, trae, codebuddy, grok, cline, warp, universal, devin)

### Fixed

- Cursor inventory/delete now includes `~/.agents/skills` and `~/.cursor/skills-cursor` (paths Cursor actually loads)
- npx-sourced deletes always run `npx skills remove` after filesystem removal (lockfile / shared store cleanup)
- Project scan also covers `.cursor/skills` and `.codex/skills`

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

[Unreleased]: https://github.com/malanjp/skls/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/malanjp/skls/releases/tag/v0.3.0
[0.2.0]: https://github.com/malanjp/skls/releases/tag/v0.2.0
[0.1.0]: https://github.com/malanjp/skls/releases/tag/v0.1.0

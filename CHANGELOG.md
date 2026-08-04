# Changelog

[English](CHANGELOG.md) | [日本語](CHANGELOG.ja.md)

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/malanjp/skls/releases/tag/v0.1.0

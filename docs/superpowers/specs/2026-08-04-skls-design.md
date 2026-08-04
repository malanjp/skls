# skls Design Spec

Date: 2026-08-04

## Goals

Unified TUI to manage agent skills across Cursor, Claude Code, and Codex:

1. Inventory with project/user scope and agent filters
2. Activation rate from conversation logs + delete recommendation score
3. Add via selectable backend (`gh skill` or `npx skills`)
4. Delete via `npx skills` + filesystem hybrid
5. Update via `gh skill update` when provenance exists

## Architecture

Self-owned inventory (Approach 2) with CLI adapters:

- `FsScanner` discovers `SKILL.md` under known agent paths
- `GhSkillCli` enriches provenance and handles search/install/update
- `NpxSkillsCli` handles package add/remove
- `LogAnalyzer` computes session-unique hits
- `DeleteScore` ranks cleanup candidates
- ratatui app owns filter/sort/modals

## Data model

`SkillRecord` aggregates one logical skill per `(normalized_id, scope)`:

- agents[], locations[], install_kind, source, source_url, version, pinned
- stats: hits, sessions_total, activation_rate, last_hit_at, delete_score

Scope vocabulary follows `gh skill`: `project` | `user` (`npx skills -g` maps to user).

## Activation

Window: 30 days (CLI `--window-days`).

Hit = skill id/name/path mention in a session transcript. Counted once per session.

`activation_rate = hits / sessions_total` for agents where the skill is installed (fallback: all scanned sessions). Missing logs → N/A.

`delete_score` weights low rate, stale last_hit, multi-host clutter, missing provenance.

## Operations

### Add

1. Choose backend: `gh skill` or `npx skills`
2. Query / select skill
3. Choose agent + scope
4. Execute non-interactive CLI

### Delete

Confirm required. Prefer `npx skills remove -y -a <agent> [-g]`. Fallback: unlink symlink or `remove_dir_all` for copies. Warn when resolved target is under `~/.agents/skills`.

### Update

`gh skill update <name> --all` when `source_url` present.

## Non-goals (v1)

- Full 70+ agent host matrix
- `gh skill publish`
- Compatibility with `gh skill-tui`
- Cloud telemetry

## Testing

Unit tests cover frontmatter parsing, inventory merge, gh/npx argument shaping, log hit extraction, delete score, filter/sort state. Adapters use `FakeCommandRunner`.

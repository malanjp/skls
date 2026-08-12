# 実装計画: Claude Code / Cursor / Codex / agent プラグイン対応

状態: 承認済み（2026-08-12）。実装前にこのファイルに残す。

## 目的

TUI がスキルを管理できるホストを、プラグインに同梱されるスキルにも広げる。
現在は `skill_roots`（`adapters/fs.rs`）のスキャンだけだが、プラグインキャッシュ内の
`skills/` ディレクトリに置かれたスキルが見えていない。

## スキャン対象とホスト紐付け

| ストア | スキャン方式 | ホスト | Scope |
|---|---|---|---|
| `~/.claude/plugins/` | `installed_plugins.json` の install_path + scope | claude-code | user→User / project・local→Project |
| `~/.cursor/plugins/cache/*/*/*/skills/` | 直走査 + パス dedupe | cursor | User |
| `~/.codex/plugins/cache/*/*/*/skills/` | 直走査 + パス dedupe | codex | User |
| `~/.agents/plugins/` | lenient（深さ上限5で `skills/` 探索） | cursor / cline / warp / universal | User |

- プラグイン内 `skills/<name>/SKILL.md` と `skills/<ns>/<name>/SKILL.md`（1ネスト）を検出
- `.agents/plugins/marketplace.json`（superpowers 等）は agents CLI 用マニフェストのため、`~/.agents/plugins/` 側で lenient 対応
- Cursor の `marketplaces/`（raw checkout）、Claude の `data/`、`.git` はスキャンしない
- 紐付けは「実体の置かれたホスト」基準。`~/.agents/` 配下は共有ストア扱い（`~/.agents/skills` と同じホストセット）

## 実機での構造調査メモ（2026-08-12 確認）

- Claude Code: `~/.claude/plugins/installed_plugins.json` が `plugin@marketplace` → `[{scope, installPath, version}]`。実体は `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/skills/<name>/SKILL.md`
- Cursor: `~/.cursor/plugins/cache/<marketplace>/<plugin>/<commit>/skills/...`。installed 一覧 JSON 無し（`.installed` マーカーは一部のみで不正確）→ cache 直走査
- Codex: `~/.codex/plugins/cache/<marketplace>/<plugin>/<version|commit>/skills/...`。各プラグインに `.codex-plugin/plugin.json`。installed 一覧 JSON 無し → cache 直走査
- agent: 現状 `~/.agents/plugins/` は未存在。superpowers 等のプラグインリポジトリに `.agents/plugins/marketplace.json` が同梱される形式

## 変更ファイル

1. `src/adapters/fs.rs`
   - `collect_skills_in_dir` を `pub(crate)` 化（plugin.rs から再利用）。ロジックは変更しない
   - 共有ストア agent リストを `pub(crate) const AGENTS_SHARED_STORE` に定数化し、`skill_roots` と plugin.rs で共用

2. `src/adapters/plugin.rs`（新規）
   - `scan_plugin_skills(project_root, home) -> Result<(Vec<DiscoveredSkill>, Vec<String>)>` を実装
   - Claude: `installed_plugins.json` を parse（無ければ skip）。各エントリの `install_path/skills/` を `collect_skills_in_dir` で収集。agent=ClaudeCode、scope=JSON 由来。version は JSON から、source_url は `.claude-plugin/plugin.json` の repository/homepage を best-effort 補完
   - Cursor / Codex: `cache/*/*/*/skills/` を走査。agent=Cursor / Codex、scope=User
   - agents: `~/.agents/plugins/` を深さ上限5で `skills/` ディレクトリ探索。`AGENTS_SHARED_STORE` の各ホストへ。scope=User
   - 収集した `DiscoveredSkill` に `source: Some(Plugin)` を付与

3. `src/model/skill.rs`
   - `DiscoveredSkill` 相当（実際は `adapters/fs.rs` の構造体）に `source: Option<InstallSource>` を追加
   - `InstallSource::Plugin` 追加（`as_str` = "plugin"）

   ※ `DiscoveredSkill` は `adapters/fs.rs` にある点に注意。model 側の変更は `InstallSource` のみ

4. `src/inventory/mod.rs`
   - `build_inventory` で `scan_plugin_skills` の結果を既存 discovery と統合（`seen_paths` で重複排除）
   - `infer_source` / `prefer_source` を Plugin 対応（優先順: Gh > Npx > Plugin > Manual）
   - 同名スキルは既存の merge で 1 行に集約され、プラグインの場所が locations に追加される

5. `src/ops.rs`
   - `plan_delete`: `source == Plugin`（またはパスに `plugins/`）で警告 `plugin_warning` を付与（cache 破壊の注意）
   - `suggested_update_backend`: Plugin → 提案なし（gh/npx を誤提案しない）
   - `prefer_update_dirs`: プラグイン cache パスを除外

6. `src/ui/render.rs`
   - 削除確認モーダルに `plugin_warning` を表示（既存の shared_warning と併記）

7. `src/adapters/mod.rs`
   - `pub mod plugin;`

8. テスト（`cargo test --lib`）
   - installed_plugins.json fixture の parse / scope マッピング
   - Claude plugin → claude-code + scope 反映
   - Cursor cache → cursor / User
   - Codex cache → codex / User
   - `~/.agents/plugins` → 共有4ホスト
   - 通常スキルとプラグインスキルの同名 merge
   - delete の plugin 警告 / update 提案なし / `npx skills remove` を呼ばない

9. ドキュメント（日英ペア）
   - `README.md` / `README.ja.md`: Scan roots 表 + Features にプラグイン対応を追記
   - `CHANGELOG.md` / `CHANGELOG.ja.md`: Unreleased に追記

## 設計判断

- プラグイン内スキルの削除は警告のみで確認必須とし、`npx skills remove` は呼ばない
- 更新（gh/npx）はプラグイン cache に適用しない
- Cursor / Codex の旧バージョン残存はパス dedupe で対処（`.installed` マーカーは使わない）
- gh / npx を必須にしない方針（AGENTS.md）は維持。プラグインスキャンは純ファイルシステム操作

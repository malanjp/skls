# AGENTS.md

skls（*skills list*）向けの作業メモ。コーディングエージェント向け。

## 言語・方針

- ユーザーへの返答は日本語。
- コミットメッセージは日本語（conventional commits: `feat:` / `fix:` / `docs:` など）。
- ドキュメントは日英ペア（`README.md` / `README.ja.md`、`CHANGELOG.md` / `CHANGELOG.ja.md`）。片方だけ更新しない。
- コミット・push・リリースはユーザーが依頼したときだけ。

## プロダクト概要

TUI でエージェントスキルを横断管理する。一覧（スキル / プラグイン / MCP）・発動率 / `delete_score`・追加（`gh skill` / `npx skills` / プラグインカタログ）・削除・更新。

- crate 名: `skls`
- リポジトリ: https://github.com/malanjp/skls
- Rust edition 2024 / ratatui + crossterm

## レイアウト

```
src/
  main.rs          CLI・イベントループ・pending action
  lib.rs           モジュール公開
  app.rs           TUI 状態機械・キーバインド（`t` で skills/plugins/mcp）
  model/skill.rs   Agent / Scope / SkillRecord / SkillFilters
  model/catalog.rs ListView / PluginRecord / McpServerRecord / PluginBackend
  inventory/       FS 発見 + gh / skill-lock のマージ
  adapters/
    fs.rs          skill_roots・SKILL.md スキャン
    gh_skill.rs    gh skill CLI
    npx_skills.rs  npx skills CLI
    plugin.rs      プラグインストア走査（skills + mcp.json + パッケージ）
    plugin_cli.rs  claude / copilot / codex plugin CLI
    mcp.rs         mcp.json / .mcp.json パーサ
    skill_lock.rs  ~/.agents/.skill-lock.json
    command.rs     CommandRunner / FakeCommandRunner
  analytics/       ログ発動率・delete_score
  ops.rs           add / delete / update オーケストレーション（スキル + プラグイン）
  ui/render.rs     ratatui 描画
```

## 重要なドメインルール

### エージェントとスキャン

- `Agent` は `src/model/skill.rs`。`as_str()` / `parse()` は `gh skill` の `agentHosts` と揃える。
- スキャンパスは `adapters::fs::skill_roots`。新ホストを足すときは **enum + parse + skill_roots + README 表** をセットで更新。
- Cursor は `~/.cursor/skills` 以外に `~/.agents/skills` と `~/.cursor/skills-cursor` も読む。ここを見落とすと Cursor にあって skls に無いスキルが出る。
- 共有ストア（`~/.agents/skills`）は複数 `Agent` に紐づく。削除パスは `plan_delete` で dedupe。共有パス削除時は警告。

### 操作フロー

- エージェント選択 UI: `j`/`k` 移動、`Space` トグル、`*` 全選択、`x` 全解除。番号キー（1/2/3）は使わない（ホスト増加に耐えるため）。
- 追加の初期選択は `Agent::primary()`（cursor / claude-code / codex）。`*` で全ホスト。
- 削除: インベントリのパスを先に消す。`source == npx` かつ `npx` 利用可なら、その後必ず `npx skills remove`（early return しない）。
- 更新: エージェント選択 → backend（gh / npx）。推定 backend は provenance / lock から。
- ビュー切替: `t` で skills → plugins → mcp。スキル一覧のキーバインドは壊さない。
- プラグイン追加の初期選択は `plugin_cli_agents()`（claude-code / copilot / codex）。Cursor はカタログ CLI なし（marketplace から入れて、とメッセージ）。
- プラグイン削除: カタログ CLI が成功したらパスは消さない。全部失敗したときだけ `remove_skill_path`。
- MCP の追加・更新はプラグインビューへ誘導。削除は親プラグインのアンインストール確認。
- `gh` / `npx` / `claude` / `copilot` / `codex` を起動時の一覧構築で必須にしない。

### テスト

```bash
cargo test --lib
```

- CLI アダプタは `FakeCommandRunner` で引数を検証する。
- 一時ディレクトリは `tempfile`。
- 実ホームのスキルツリーを壊すテストを書かない。

### リリース（依頼時）

1. `Cargo.toml` バージョンを上げる（`Cargo.lock` の package 版も追随）。
2. `CHANGELOG.md` / `CHANGELOG.ja.md` の `[Unreleased]` を版セクションにし、ハイライトを書く。
3. テスト → commit → `git push` → `gh release create vX.Y.Z` → `cargo publish`。
4. タグ後に Changelog だけ直した場合はタグを docs コミットへ付け直してよい（ユーザー確認後）。

## やってはいけないこと

- `gh` / `npx` / `claude` / `copilot` / `codex` を起動時の一覧構築で必須にしない（無くても一覧・指標は動く）。
- 削除確認なしの破壊的操作。
- エージェント選択にホスト固定の数字キーを増やすこと。
- README / CHANGELOG の片言語だけの更新。

## Cursor Cloud specific instructions

- **toolchain**: edition 2024 のため Rust 1.85+ が必須。Cloud VM の既定 `rustc` は古い場合があり、update script が `rustup default stable` を設定する。個別セッションでツールチェインを固定したいときは `rustup override set stable` を使う。
- **lint/test/build/run**: 標準コマンドは `README.md`「Development」節と本ファイル「テスト」節を参照（`cargo clippy` / `cargo fmt --check` / `cargo test --lib` / `cargo build`）。`cargo clippy` は既存コードに warning（`collapsible_if` 等）が出るが error ではない。`cargo fmt --check` はクリーン。
- **TUI の実行**: `skls` は全画面 TUI なので描画確認には実ターミナル（Desktop / computer use）が必要。ヘッドレスな検証には `cargo run -- --dump-json`（TUI を出さず在庫を JSON 出力）が使える。
- **スキャン対象**: skls は実行ユーザーの HOME 配下スキルディレクトリを読む。Cloud VM は `~/.cursor/plugins/cache/**/skills/` にプラグインスキルが多数あり、`--dump-json` は実データ（`source: plugin`）を返す。テストで実 HOME のスキルツリーを壊さないこと。

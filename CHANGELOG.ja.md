# Changelog

[English](CHANGELOG.md) | [日本語](CHANGELOG.ja.md)

このプロジェクトの主な変更は本ファイルに記録する。

形式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に準拠し、バージョン付けは [Semantic Versioning](https://semver.org/lang/ja/) に従う。

## [Unreleased]

### ハイライト

- **MCP 一覧** — `t` で skills → plugins → MCP を切替。プラグインの `mcp.json` / `.mcp.json` から同梱サーバーを読む（Agent Plugins 1.0、および `command` / `url` だけの緩め形式）
- **プラグインカタログ操作** — `claude plugin` / `copilot plugin` / `codex plugin` から追加・更新・削除。Cursor にカタログ CLI は無い（ホストの marketplace から入れる）。一覧はこれらのバイナリ無しでも動く
- **ページ送りとソート方向** — `Ctrl+F` / `Ctrl+B`（および PageDown / PageUp）でページ移動。`Ctrl+H` / `Ctrl+L`（および Home / End）で先頭/末尾へ。`S` で昇順/降順を切替

### 追加

- インストール済みプラグインパッケージと同梱 MCP サーバーの一覧（`t` で切替）。プラグイン列: `NAME` / `SCOPE` / `MARKET` / `SK` / `MCP`。MCP 列: `NAME` / `TRANS` / `PLUGIN` / `AGENTS`
- ホストのカタログ CLI によるプラグインの追加・更新・削除（`claude plugin install|update|uninstall`、`copilot plugin install|update|uninstall`、`codex plugin add|remove`）。Codex の更新は `plugin add` の再実行。uninstall の CLI がすべて失敗したときだけパスをフォールバックで消す
- `--dump-json` の出力をスキル配列から `{ skills, plugins, mcp_servers }` に変更
- スキル一覧の `SRC` を `plugin` / `gh skill` / `npx skills` / `manual` と明示。プラグインコピーがあっても lockfile / `~/.agents/skills` / `gh skill list` があれば `npx skills` / `gh skill` を優先。プラグイン内だけのパスは `plugin` のまま
- 一覧のページ送り: `Ctrl+F` / `PageDown` で次、`Ctrl+B` / `PageUp` で前（端で停止。`j`/`k` は従来どおり循環）。歩幅はリスト表示行数 − 1。`Ctrl+H` / `Home` で先頭、`Ctrl+L` / `End` で末尾へジャンプ
- `S` でソート方向を切替。`s` はキーを循環し、そのキーの既定方向に戻す（`delete_score` / `rate` / `last_hit` は降順、`name` / `author` / `source` は昇順）。ヘッダに `↑` / `↓` を表示

## [0.4.0] - 2026-08-13

### ハイライト

- **プラグイン由来スキル** — エージェントプラグインに同梱されたスキルを一覧に表示するようにした: Claude Code（`~/.claude/plugins/`、scope は `installed_plugins.json` 由来）、Cursor（`~/.cursor/plugins/cache/`）、Codex（`~/.codex/plugins/cache/`）、共有ストア（`~/.agents/plugins/`）。各スキルはファイルの実体があるホストに紐付け、`source: plugin` として扱う
- **作者・出所の列** — 一覧に `SRC`（gh / npx / plugin / manual）と `AUTHOR` を表示。作者は SKILL.md の frontmatter・プラグインマニフェスト・ソースリポジトリの GitHub owner から取得する

### 追加

- エージェントプラグインに同梱されたスキルをスキャンするようにした: Claude Code（`~/.claude/plugins/`、scope は `installed_plugins.json` 由来）、Cursor（`~/.cursor/plugins/cache/`）、Codex（`~/.codex/plugins/cache/`）、共有ストア（`~/.agents/plugins/`）。プラグイン由来のスキルは `source: plugin` として扱い、ファイルの実体があるホストに紐付ける
- 削除確認でプラグインインストール内のパスを警告するようにした。プラグイン由来スキルは `gh` / `npx` の更新提案と更新対象ディレクトリから除外する
- 一覧に `SRC`（出所: gh / npx / plugin / manual）と `AUTHOR` 列を追加。詳細パネルと `--dump-json` にも作者を出力する。作者は SKILL.md の frontmatter・プラグインマニフェスト・ソースリポジトリの GitHub owner から取得
- ソート切替（`s`）に `author`（未設定は末尾）と `source`（gh → npx → plugin → manual の順）を追加

## [0.3.2] - 2026-08-05

### 修正

- スキル一覧の列ヘッダ（`NAME` / `SCOPE` / `RATE` / `SCORE`）が行の値と揃うようにした（チェックボックス + 選択ガター分）

### 変更

- README に TUI のサンプルスクリーンショットを追加（`docs/images/skls.png`）

## [0.3.1] - 2026-08-04

### 追加

- コーディングエージェント向け `AGENTS.md`

### 変更

- `Cargo.lock` を更新（間接依存）

## [0.3.0] - 2026-08-04

### ハイライト

- **Cursor が見ているものを一覧できる** — `~/.agents/skills` と `~/.cursor/skills-cursor` をスキャン対象に追加。Cursor に出るのに skls に出なかったスキル（例: `brand`）も表示・削除できる
- **27 エージェントホスト対応** — Cursor / Claude Code / Codex に加え、gemini-cli・antigravity*・github-copilot・opencode・pi・amp・kimi-cli・replit・qwen-code・augment・continue・droid・kilo・qoder・roo・trae・codebuddy・grok・cline・warp・universal・devin
- **npx 削除の後始末** — npx 由来スキルはパス削除のあと必ず `npx skills remove` を実行（lockfile / 共有ストアの整合）
- **共有ストア削除の注意** — `~/.agents/skills/...` 削除時に他エージェントへの影響を警告。複数ホストで同じパスは削除プランで重複排除

### 追加

- 上記ホスト向けのインベントリルート（`gh skill` の `agentHosts` / 一般的なインストール先）
- project スキャンに `.cursor/skills` と `.codex/skills`

### 修正

- Cursor user スキャンが、実行時に読まれる共有 / 管理ツリーを見落としていた
- インベントリにパスがあると `npx skills remove` を呼んでいなかった（`npx_available` も常に false だった）

## [0.2.0] - 2026-08-04

### 変更

- 追加 / 削除 / 更新で対象エージェントをチェックボックス選択（`j`/`k`+`Space`、`*`=全選択、`x`=全解除、`Enter`で次へ）

## [0.1.0] - 2026-08-04

**skls**（*skills list*）の初回リリース。

### 追加

- Cursor / Claude Code / Codex 横断のスキル一覧 TUI
- project / user スコープとエージェントでフィルタ、名前・説明の検索
- 名前 / 発動率 / `delete_score` / 最終ヒットでソート
- 複数選択（`Space` / `*` / `x`）と一括削除・更新
- 会話ログからの発動率（セッション単位ユニーク）と `delete_score`
- `gh skill` / `npx skills` による追加フロー（ステップダイアログ）
- 確認付き削除。共有実体への symlink は警告
- `gh skill` / `npx skills` による更新。provenance から backend を推定
- 高速起動: 一覧を先に表示し、発動率はサンプリング解析（既定: 80 セッション/エージェント、256KiB/ファイル）
- CLI: `--project-root` / `--window-days` / `--max-sessions` / `--max-bytes` / `--full-scan` / `--dump-json`
- ドキュメント: 英語 README（メイン）、日本語 README、MIT ライセンス

### 補足

- ログ照合はヒューリスティックであり、厳密なスキル実行回数ではない
- `gh` / `npx` がなくても一覧と指標は動く。CLI 依存の操作だけ無効になる

[Unreleased]: https://github.com/malanjp/skls/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/malanjp/skls/releases/tag/v0.3.2
[0.3.1]: https://github.com/malanjp/skls/releases/tag/v0.3.1
[0.3.0]: https://github.com/malanjp/skls/releases/tag/v0.3.0
[0.2.0]: https://github.com/malanjp/skls/releases/tag/v0.2.0
[0.1.0]: https://github.com/malanjp/skls/releases/tag/v0.1.0

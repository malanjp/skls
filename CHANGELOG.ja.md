# Changelog

[English](CHANGELOG.md) | [日本語](CHANGELOG.ja.md)

このプロジェクトの主な変更は本ファイルに記録する。

形式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に準拠し、バージョン付けは [Semantic Versioning](https://semver.org/lang/ja/) に従う。

## [Unreleased]

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

[Unreleased]: https://github.com/malanjp/skls/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/malanjp/skls/releases/tag/v0.2.0
[0.1.0]: https://github.com/malanjp/skls/releases/tag/v0.1.0

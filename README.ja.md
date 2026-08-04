# skls

[English](README.md) | [日本語](README.ja.md)

**skls**（*skills list*）— Cursor / Claude Code / Codex のエージェントスキルを横断管理する TUI。

どのスキルが入っているか、どこに効いているか、使われているかを一覧で把握し、追加・削除・更新まで同じ画面で行う。

## できること

- **一覧**: project / user スコープとエージェントでフィルタ。複数選択して一括操作できる
- **指標**: 会話ログから発動率を算出し、削除判断スコア（`delete_score`）を表示する
- **追加**: 都度 `gh skill` か `npx skills` を選んでインストールする
- **削除**: インベントリ上のパスを優先して削除する（確認必須）。共有実体への symlink は警告する
- **更新**: `gh skill` / `npx skills` を選んで更新する。インストール元を推定できれば Enter で採用できる

## 依存

| 必須 | 任意 |
|------|------|
| Rust 1.85+（edition 2024） | [`gh`](https://cli.github.com/)（`gh skill`） |
| | Node.js / `npx`（[`npx skills`](https://skills.sh/)） |

`gh` / `npx` がなくても一覧と発動率は動く。足りない CLI に依存する操作だけ無効になる。

## インストール / 起動

```bash
git clone https://github.com/malanjp/skls.git
cd skls
cargo run --release
```

パスに入れる場合:

```bash
cargo install --path .
skls
```

### CLI オプション

```bash
skls                                 # cwd を project root として起動
skls --project-root /path/to/repo
skls --window-days 30                # 発動率の集計窓（日）
skls --max-sessions 200              # エージェントあたり読むセッション数（既定 80）
skls --max-bytes 1048576             # セッションファイルあたりの読込上限バイト（既定 256KiB）
skls --full-scan                     # セッション / バイト上限なし（大きいログツリーでは遅い）
skls --dump-json                     # TUI なしでインベントリを JSON 出力
```

起動時はスキル一覧を先に出し、その後に発動率をサンプリング解析する。既定上限なら通常 1〜2 秒程度で一覧まで到達する。

## 画面

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

左が一覧、右が詳細。行頭の `[ ]` / `[x]` は複数選択。デフォルトソートは `delete_score`（高いほど削除候補）。

タイトルの `sample:` は発動率解析の上限、ステータスの `sampled (-N older)` はスキップした古いセッション数を示す。

## キーバインド

| キー | 動作 |
|------|------|
| `j` / `k` | 移動 |
| `Space` | 行の選択トグル |
| `*` | 表示中を全選択 / 全解除 |
| `x` | 選択クリア |
| `/` | 名前・説明の検索 |
| `f` | フィルタパネル |
| `s` | ソート切替（`name` → `rate` → `delete_score` → `last_hit`） |
| `a` | 追加フロー |
| `d` | 削除確認（選択があれば一括、なければカーソル行） |
| `u` | 更新（backend 選択。推定があれば Enter で採用） |
| `r` | 軽い再スキャン（一覧のみ） |
| `R` | 発動率を再計算 |
| `?` | ヘルプ |
| `q` | 終了 |

### フィルタ（`f`）

| キー | 内容 |
|------|------|
| `p` / `u` / `a` | project / user / 全スコープ |
| `1` / `2` / `3` / `0` | cursor / claude-code / codex / 全エージェント |
| `c` | フィルタクリア |

### 追加（`a`）

ダイアログで順に進む。`Esc` で前のステップ、`q` で中止。

1. backend 選択: `1`/`g` = `gh skill`、`2`/`n` = `npx skills`
2. ソース入力（gh: 検索語、npx: `owner/repo` または `owner/repo@skill`）
3. 結果選択（gh の場合）
4. エージェント（`1`/`2`/`3`）→ スコープ（`p`/`u`）で実行

### 削除（`d`）

- `y` / `Enter` で実行、`n` / `q` / `Esc` でキャンセル
- `1`/`2`/`3` で対象エージェントを絞り、`0` で全エージェントに戻す
- 共有実体（例: `~/.agents/skills`）を指す symlink は警告する
- 削除後は一覧だけ再スキャンする。発動率は `R` で再計算する

### 更新（`u`）

1. backend 選択: `1`/`g` = `gh skill`、`2`/`n` = `npx skills`
2. 推定があるときは `Enter` でその backend を採用できる
3. 片方の CLI しか無い場合は選択を省略する

推定の優先順位:

| 判定 | 推定 |
|------|------|
| `SKILL.md` に `github-repo` / `github-tree-sha` がある、または `gh skill list` の provenance | `gh skill` |
| `~/.agents/.skill-lock.json`（または project 側の lock）に載っている | `npx skills` |
| 両方ある場合 | `gh skill` を優先 |
| 混在・不明 | 推定なし（手動選択） |

`gh skill update` は、メタデータのあるホストディレクトリ（`.cursor` / `.codex` など）を優先して `--dir` する。`npx skills update` はスコープに応じて `-g` / `-p` を付ける。

## スキャン対象

| Agent | project | user |
|-------|---------|------|
| cursor | `.agents/skills/` | `~/.cursor/skills/` |
| claude-code | `.claude/skills/` | `~/.claude/skills/` |
| codex | `.agents/skills/` | `~/.codex/skills/` |

同一スキルが複数ホストにある場合は 1 行に集約する。

追加のメタデータ:

- `gh skill list --json` があれば `source_url` / `version` / `pinned` を付与する
- `~/.agents/.skill-lock.json` があれば `source: npx` を付与する（`npx skills list` は起動時に呼ばない）

スコープ語彙は `gh skill` 準拠（`project` / `user`）。`npx skills -g` は user と同一視する。

## 発動率と削除スコア

直近 N 日（デフォルト 30）のセッションログを解析し、スキル名またはパス言及を **セッション単位でユニークカウント** する。

| ソース | パス |
|--------|------|
| Cursor | `~/.cursor/projects/**/agent-transcripts/**/*.jsonl` |
| Claude Code | `~/.claude/projects/**/*.jsonl` |
| Codex | `~/.codex/sessions/**/rollout-*.jsonl` |

- `activation_rate` = `hits / sessions_total`（セッション 0 のときは N/A）
- `delete_score` が高いほど削除候補（下記の加点の合計）
- 詳細のラベル: `keep`（35 未満） / `review`（35–59） / `consider delete`（60 以上）

`delete_score` の加点（実装: `src/analytics/score.rs`）:

| 要因 | 条件 | 加点 |
|------|------|------|
| 発動率 | セッションなし / 不明 | +25 |
| | 0% | +40 |
| | 5% 未満 | +30 |
| | 15% 未満 | +15 |
| | 30% 未満 | +5 |
| 最終ヒット | なし | +20 |
| | 60 日超 | +25 |
| | 30 日超 | +15 |
| | 14 日超 | +8 |
| ホスト数 | 3 エージェント以上 | +10 |
| | 2 エージェント | +5 |
| provenance | `manual` かつ `source_url` なし | +5 |
| ヒット | `hits == 0` | +10 |

ログ照合はヒューリスティックであり、厳密な「スキル実行回数」ではない。削除前の判断材料として使う。

速度のため、既定ではエージェントあたり **直近 80 セッション**、ファイルあたり **最大 256KiB** だけ読む。上限は `--max-sessions` / `--max-bytes` / `--full-scan` で変えられる。

## 開発

```bash
cargo test --lib
cargo run --release -- --dump-json
```

設計メモ: [docs/superpowers/specs/2026-08-04-skls-design.md](docs/superpowers/specs/2026-08-04-skls-design.md)

## ライセンス

未定（リポジトリ作成時点では未設定）。

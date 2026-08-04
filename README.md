# skillui

Cursor / Claude Code / Codex のエージェントスキルを横断管理する TUI。

どのスキルが入っているか、どこに効いているか、使われているかを一目で見て、追加・削除の判断まで一気通貫で行う。

## できること

- **一覧**: project / user スコープとエージェントでフィルタ
- **指標**: 会話ログから発動率を算出し、削除判断スコア（`delete_score`）を表示
- **追加**: `gh skill` か `npx skills` を都度選択してインストール
- **削除**: `npx skills remove` とファイルシステム操作のハイブリッド（確認必須）
- **更新**: provenance があるスキルは `gh skill update`

## 依存

| 必須 | 任意 |
|------|------|
| Rust 1.85+（edition 2024） | [`gh`](https://cli.github.com/)（`gh skill`） |
| | Node.js / `npx`（[`npx skills`](https://skills.sh/)） |

`gh` / `npx` がなくても一覧と発動率は動く。足りない CLI に依存する操作だけ無効になる。

## インストール / 起動

```bash
git clone <this-repo> skillui
cd skillui
cargo run --release
```

パスに入れる場合:

```bash
cargo install --path .
skillui
```

### CLI オプション

```bash
skillui                              # cwd を project root として起動
skillui --project-root /path/to/repo
skillui --window-days 30             # 発動率の集計窓（日）
skillui --max-sessions 200           # エージェントあたり読むセッション数（既定 80）
skillui --max-bytes 1048576          # セッションファイルあたりの読込上限バイト（既定 256KiB）
skillui --full-scan                  # 上限なし（大きいログツリーでは遅い）
skillui --dump-json                  # TUI なしでインベントリを JSON 出力
```

起動時はまずスキル一覧を出し、続けて発動率をサンプリング解析する（既定上限なら通常 1〜2 秒程度）。

## 画面

```
┌ skillui  scope:all  agents:all  sort:delete_score  window:30d ─┐
│ 170 skills | gh:ok npx:ok | /path/to/project                    │
├──────────────────────────────┬──────────────────────────────────┤
│ NAME         SCOPE  RATE SCORE│ detail                           │
│ brand        user   0.0%  85  │ brand                            │
│ ...                           │ agents: cursor,claude-code,codex │
│                               │ hits / rate / delete_score ...   │
└──────────────────────────────┴──────────────────────────────────┘
│ j/k / f s a d u r ? q                                           │
```

左がスキル一覧、右が詳細。デフォルトソートは `delete_score`（高いほど削除候補）。

## キーバインド

| キー | 動作 |
|------|------|
| `j` / `k` | 移動 |
| `Space` | 行の選択トグル |
| `*` | 表示中を全選択 / 全解除 |
| `x` | 選択クリア |
| `/` | 名前・説明のインクリメンタル検索 |
| `f` | フィルタパネル |
| `s` | ソート切替（`name` → `rate` → `delete_score` → `last_hit`） |
| `a` | 追加フロー |
| `d` | 削除確認（選択があれば一括、なければカーソル行） |
| `u` | `gh skill update`（同上。provenance があるものだけ） |
| `r` | 軽い再スキャン（一覧のみ、高速） |
| `R` | 発動率を再計算（直近セッションをサンプリング） |
| `?` | ヘルプ |
| `q` | 終了 |

起動時はまずスキル一覧を表示し、続けて発動率解析（エージェントあたり直近 80 セッション）を走らせる。

### フィルタ（`f`）

| キー | 内容 |
|------|------|
| `p` / `u` / `a` | project / user / 全スコープ |
| `1` / `2` / `3` / `0` | cursor / claude-code / codex / 全エージェント |
| `c` | フィルタクリア |

### 追加（`a`）

1. backend 選択: `1`/`g` = `gh skill`、`2`/`n` = `npx skills`
2. クエリ入力（gh: 検索語、npx: `owner/repo` または `owner/repo@skill`）
3. 結果選択（gh の場合）
4. エージェント（`1`/`2`/`3`）→ スコープ（`p`/`u`）で実行

### 削除（`d`）

- `y` で確認実行、`n` / `Esc` でキャンセル
- `1`/`2`/`3` で対象エージェントを絞ってから確認できる
- 共有実体（例: `~/.agents/skills`）を指す symlink は警告を出す

## スキャン対象

| Agent | project | user |
|-------|---------|------|
| cursor | `.agents/skills/` | `~/.cursor/skills/` |
| claude-code | `.claude/skills/` | `~/.claude/skills/` |
| codex | `.agents/skills/` | `~/.codex/skills/` |

同一スキルが複数ホストにある場合は 1 行に集約する。`gh skill list --json` があれば `source_url` / `version` / `pinned` を付与する。

スコープ語彙は `gh skill` 準拠（`project` / `user`）。`npx skills -g` は user と同一視。

## 発動率と削除スコア

直近 N 日（デフォルト 30）のセッションログを解析し、スキル名またはパス言及を **セッション単位でユニークカウント** する。

| ソース | パス |
|--------|------|
| Cursor | `~/.cursor/projects/**/agent-transcripts/**/*.jsonl` |
| Claude Code | `~/.claude/projects/**/*.jsonl` |
| Codex | `~/.codex/sessions/**/rollout-*.jsonl` |

- `activation_rate` = `hits / sessions_total`（セッション 0 のときは N/A）
- `delete_score` が高いほど削除候補（低発動・長期未使用・多ホスト・provenance なしなどを加点）
- 詳細のラベル: `keep` / `review` / `consider delete`

ログはヒューリスティック照合のため、厳密な「スキル実行回数」ではない。削除前の判断材料として使う。

速度のため、エージェントあたり **直近 80 セッション**・ファイルあたり **最大 256KB** だけ読む。古いセッションはスキップされ、ステータスに `sampled (-N older)` と出る。`r` は一覧のみ、`R` で発動率を再計算。

## 開発

```bash
cargo test --lib
cargo run --release -- --dump-json
```

設計メモ: [docs/superpowers/specs/2026-08-04-skillui-design.md](docs/superpowers/specs/2026-08-04-skillui-design.md)

## ライセンス

未定（リポジトリ作成時点では未設定）。

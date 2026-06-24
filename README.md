# open-dataset-cleaner (odc)

Web から収集した文書（WARC / HTML / プレーンテキスト / JSONL）を、LLM の事前学習・
ファインチューニング用データセットとして使える品質までクリーニング・スコアリング・
重複除去・フィルタリングするための CLI パイプラインツールです。

- 言語: Rust
- 配布形態: CLI バイナリ（`open-dataset-cleaner` / 短縮コマンド `odc`）
- 対応 OS: Windows / Linux / macOS (arm64)
- 処理方式: ストリーミング（全件メモリ展開しない）+ `rayon` による並列処理

何が・どれだけの理由で除外/採用されたかを統計レポートとして出力できる点が特徴です。

詳細な仕様は [要件定義書](.docs/requirements.md)、リリースごとの変更点は
[CHANGELOG](CHANGELOG.md) を参照してください。

## 特徴

- **入力**: WARC(`.warc`/`.warc.gz`)、HTML（単体/ディレクトリ/glob）、プレーンテキスト、
  JSONL をストリーミング読み込み
- **テキスト抽出**: HTML タグ除去・ボイラープレート除去（nav/footer/広告枠/サイドバー）、
  Markdown 変換オプション（見出し/リスト/リンク保持を設定可能）
- **品質スコアリング**
  - 言語検出（`lingua-rs`）・言語混合率・文字種比率（ひらがな/カタカナ/漢字/英数字/その他）
  - テキスト品質（重複行率・記号数字異常比率・平均文長/文長分散・残留 HTML/URL 検出）
  - コンテンツ品質（広告キーワード/正規表現検出・SEO スパムスコア・自然さスコア）
  - Perplexity スコアは `perplexity` cargo feature のスキャフォルドのみ（KenLM 未接続、
    デフォルト無効）
- **重複除去**: 完全一致（blake3 ハッシュ）＋ 近似重複（MinHash + LSH、類似度閾値設定可）。
  本文ではなくハッシュ/シグネチャのみを保持するため省メモリ
- **フィルタリング**: TOML/YAML 設定ファイルによる閾値設定、AND/OR/NOT を組み合わせた
  ルール、WASM プラグイン（`wasmtime` サンドボックス、タイムアウト/メモリ制限付き）
- **出力**: Parquet/JSONL、シャード分割（最大行数指定）、除外レコードの理由付き別出力
- **運用系**: `rayon` による並列処理、処理統計レポート（除外理由別件数・スコア分布等）、
  チェックポイントによる再開対応（JSONL 出力時）、ログレベル設定

## インストール

GitHub Releases からビルド済みバイナリを取得してインストールします。

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/tonrakun/open-dataset-cleaner/main/scripts/install.ps1 | iex
```

特定バージョンやインストール先を指定する場合:

```powershell
.\scripts\install.ps1 -Version v1.0.0 -InstallDir D:\tools\odc
```

### Linux / macOS

```sh
curl -fsSL https://raw.githubusercontent.com/tonrakun/open-dataset-cleaner/main/scripts/install.sh | bash
```

特定バージョンを指定する場合:

```sh
curl -fsSL .../install.sh | bash -s -- v1.0.0
```

> macOS は arm64 (`aarch64-apple-darwin`) のみ配布対象です。Intel (`x86_64-apple-darwin`)
> 向けバイナリは GitHub Actions の macos-13 ランナー割当問題により配布対象外です
> （詳細は [CHANGELOG](CHANGELOG.md) 参照）。

### ソースからビルド

```sh
git clone https://github.com/tonrakun/open-dataset-cleaner.git
cd open-dataset-cleaner
cargo build --release
# バイナリ: target/release/odc(.exe)
```

`perplexity` feature を有効にする場合は別途 KenLM のネイティブビルドが必要です
（デフォルトでは依存を持ちません）。

```sh
cargo build --release --features perplexity
```

## クイックスタート

1. 設定ファイルを用意します（[`config.example.toml`](config.example.toml) を参考に編集）。

   ```toml
   [input]
   format = "jsonl"
   paths = ["./data/**/*.jsonl"]
   text_field = "text"

   [output]
   format = "jsonl"
   path = "./out/dataset.jsonl"
   write_rejected = true
   rejected_path = "./out/dataset.rejected.jsonl"

   [scoring.language]
   allow = ["ja", "en"]
   max_mixed_ratio = 0.2
   ```

2. パイプラインを実行します。

   ```sh
   odc run --config config.toml
   ```

3. 出力を確認します。

   - `./out/dataset.jsonl` … 採用されたレコード（本文・メタデータ・各スコア付き）
   - `./out/dataset.rejected.jsonl` … 除外されたレコード（除外理由付き）
   - `./out/dataset.stats.json` … 件数・除外理由別件数・スコア分布などの統計レポート

## CLI コマンド

### `odc run`

設定ファイルに基づきパイプラインを実行します。

```sh
odc run --config <CONFIG> [OPTIONS]
```

| オプション | 説明 |
|---|---|
| `--config <CONFIG>` | 設定ファイルパス（必須） |
| `--input <INPUT>` | 入力パスを上書き（複数指定可） |
| `--input-format <FORMAT>` | 入力フォーマットを上書き（`text`/`jsonl`/`warc`/`html`） |
| `--output <OUTPUT>` | 出力パスを上書き |
| `--output-format <FORMAT>` | 出力フォーマットを上書き（`jsonl`/`parquet`） |
| `--threads <N>` | 並列スレッド数（`0` = 論理コア数） |
| `--batch-size <N>` | バッチサイズ |
| `--log-level <LEVEL>` | ログレベル（`trace`/`debug`/`info`/`warn`/`error`） |
| `--checkpoint-dir <DIR>` | チェックポイントディレクトリ（JSONL 出力時のみ再開対応） |
| `--stats-output <PATH>` | 統計レポート出力先を上書き |
| `--stats-format <FORMAT>` | 統計レポート形式を上書き |
| `--dry-run` | 出力ファイルを書き込まずに統計のみ確認 |

### `odc validate-extraction`

抽出済みテキストに HTML タグ・URL の残留がないかを検証します。

```sh
odc validate-extraction <PATH> [--report text|json]
```

## 設定ファイル

設定は TOML（`[input]`/`[output]`/`[extract]`/`[scoring.*]`/`[filters]`/`[dedup]`/
`[runtime]`/`[stats]`/`[[plugins]]`）で記述します。各項目の意味は
[`config.example.toml`](config.example.toml) にコメント付きで網羅されています。
WASM プラグインの入出力インターフェースについても同ファイルに記載があります。

## 開発

```sh
cargo test            # テスト実行
cargo build --release # リリースビルド
```

CI（`.github/workflows/release.yml`）は `main` への push と `v*.*.*` タグで
Windows / Linux / macOS(arm64) 向けにテスト・ビルド・リリースを実行します。

## 既知の制限

- Perplexity スコアは未接続（KenLM ネイティブビルドの確認待ち）
- 数百 GB〜TB 級入力でのメモリ線形性は未実測
- macOS Intel (`x86_64-apple-darwin`) 向けバイナリは配布対象外

詳細は [CHANGELOG](CHANGELOG.md) を参照してください。

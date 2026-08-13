# TsubuGallery

## 短いProcessing作品を保存・実行・鑑賞するマルチプラットフォーム・ギャラリー

**Version 0.2**  
**2026-08-13**

---

## 1. 目的

TsubuGalleryは、`#つぶやきProcessing`のような短いProcessingコードを複数保存し、その実行結果をスクリーンショットのギャラリーとして一覧し、選択した作品を全画面で高速に実行・鑑賞するためのアプリケーションである。

一般的なProcessing IDEのように「コードを書くこと」を中心に据えるのではなく、短いコードを一つの作品として蓄積し、画像ギャラリーを眺める感覚で作品を選び、すぐに実行結果へ移れる体験を中心に据える。

TsubuGalleryはProcessing開発環境の完全な代替を目指さない。重視するのは以下である。

- 短いProcessingコードを簡単に入力・保存できること
- 保存した作品をスクリーンショットで視覚的に管理できること
- ギャラリーから選択した作品をすぐに実行できること
- 全画面表示へ滑らかに移行できること
- 複数作品を高速に切り替えられること
- Windows、macOS、Linux、Android、iOSへ展開できること
- UI表示言語を切り替えられること
- JVMへ依存しない軽量なProcessing互換ランタイムを実現すること

---

## 2. アプリケーション名

正式名称は **TsubuGallery** とする。

名称は「つぶやきProcessing」の「つぶ」と、作品を一覧・鑑賞する「Gallery」を組み合わせたものである。

```text
TsubuGallery
```

日本語UIでは、必要に応じて補助表記として次を使用できる。

```text
TsubuGallery
つぶやきProcessingギャラリー
```

アプリ名そのものはローカライズせず、すべてのプラットフォームと言語で `TsubuGallery` に統一する。

---

## 3. プロダクトの位置づけ

TsubuGalleryは「IDE」ではなく、**コード・ライブラリ、ビジュアル・ギャラリー、高速ランタイムを統合した作品鑑賞アプリ**である。

ユーザーは短いProcessingコードを入力して保存する。コードを実行すると代表フレームのスクリーンショットが生成され、ギャラリー画面ではソースコードではなくスクリーンショットが主役となる。

| 項目 | 方針 |
|---|---|
| 主用途 | 短いProcessing作品の保存、ギャラリー管理、全画面実行、高速切り替え |
| 主画面 | スクリーンショットを並べたギャラリー |
| 対象ユーザー | #つぶやきProcessingの作成・収集・鑑賞を行う人 |
| 重視する特性 | 高速起動、即時切り替え、軽量、視覚的な一覧性 |
| 対応方針 | マルチプラットフォーム、マルチランゲージ |
| 初期段階で重視しないもの | 完全なIDE、Java 100%互換、巨大プロジェクト、外部ライブラリ完全互換 |

---

## 4. 基本ユーザー体験

TsubuGalleryを起動すると、最初にコード一覧ではなく作品のスクリーンショット一覧を表示する。

```text
TsubuGallery

┌────────────┐  ┌────────────┐  ┌────────────┐
│            │  │            │  │            │
│ Screenshot │  │ Screenshot │  │ Screenshot │
│            │  │            │  │            │
├────────────┤  ├────────────┤  ├────────────┤
│ Spiral     │  │ Circles    │  │ Noise      │
└────────────┘  └────────────┘  └────────────┘

┌────────────┐  ┌────────────┐  ┌────────────┐
│            │  │            │  │            │
│ Screenshot │  │ Screenshot │  │ Screenshot │
│            │  │            │  │            │
└────────────┘  └────────────┘  └────────────┘
```

作品を選択すると、詳細画面または直接Viewerへ移動する。

```text
Gallery
   ↓
作品を選択
   ↓
Fullscreen Viewer
   ↓
← 前の作品      次の作品 →
```

編集が必要な場合だけコード編集画面を開く。

このため、通常の利用ではコードエディタを意識せず、TsubuGalleryをデジタル作品集として利用できる。

---

## 5. 基本ユーザーフロー

### 5.1 新規作品

```text
新規作成
   ↓
コード入力
   ↓
保存
   ↓
構文解析
   ↓
Bytecode生成
   ↓
実行
   ↓
スクリーンショット生成
   ↓
Galleryへ追加
```

### 5.2 作品鑑賞

```text
Gallery
   ↓
サムネイル選択
   ↓
キャッシュ済みBytecodeをロード
   ↓
全画面実行
```

### 5.3 作品切り替え

```text
Sketch A
   ↓
次へ
   ↓
Sketch B
   ↓
次へ
   ↓
Sketch C
```

ViewerプロセスまたはRuntimeは作品切り替えのたびに再起動せず、次の作品を可能な限り先読みする。

---

## 6. ギャラリーUI

### 6.1 ギャラリーをホーム画面とする

アプリ起動直後のホーム画面はGalleryとする。

Galleryには各作品について以下を表示する。

- スクリーンショット
- タイトル
- お気に入り状態
- 必要に応じてタグ
- 実行可能／エラー状態

コード本文は通常表示しない。

### 6.2 表示方式

初期版ではグリッド表示を基本とする。

将来的には以下を追加できる。

- グリッド
- 大型カード
- リスト
- お気に入りのみ
- タグ別
- 最近追加した作品
- ランダム表示

### 6.3 レスポンシブレイアウト

画面幅に応じて列数を自動調整する。

```text
Phone       2 columns
Tablet      3〜4 columns
Desktop     4〜8 columns
Wide Screen 6〜10 columns
```

固定値ではなくカードの最小幅を基準としてレイアウトする。

---

## 7. スクリーンショット生成

スクリーンショットは単なる付属情報ではなく、TsubuGalleryにおける作品の主要メタデータとする。

### 7.1 自動生成

作品を正常に実行できた場合、Runtimeから代表フレームを取得してサムネイルを生成する。

初期案では以下のいずれかを利用する。

- `setup()`完了後の最初の安定フレーム
- 60フレーム目
- 実行開始から一定時間後
- ユーザーが指定したフレーム

初期版では「一定フレーム後に自動取得」を標準とする。

### 7.2 手動更新

ユーザーが任意のタイミングで、

```text
Update Thumbnail
```

を実行できるようにする。

アニメーション作品では、ユーザーが最も気に入った瞬間をサムネイルとして設定できる。

### 7.3 保存形式

サムネイルはDBへ直接格納せず、画像ファイルとしてキャッシュ／データ領域へ保存する。

例:

```text
data/
  sketches/
  thumbnails/
    01H....webp
    01J....webp
  cache/
    01H....bytecode
```

サムネイル表示用には、元画像とは別に小型キャッシュを生成してもよい。

---

## 8. 全画面Viewer

作品を選択すると、UIを消して実行結果を画面全体へ表示する。

Viewerではタイトルバー、メニュー、編集UIを通常表示しない。

### 8.1 基本操作

| 操作 | Desktop | Mobile |
|---|---|---|
| 次の作品 | 右矢印 / PageDown | 左方向へのスワイプ |
| 前の作品 | 左矢印 / PageUp | 右方向へのスワイプ |
| 全画面解除／戻る | Esc | 戻る操作／ジェスチャー |
| 一時停止 | Space | タップメニュー |
| ランダム | R | メニュー |
| 情報表示 | I | タップ |

Viewer自身の操作とスケッチへ渡す入力が衝突する場合は、Viewerの予約操作を優先する。

### 8.2 UI自動非表示

操作用オーバーレイは必要時だけ表示し、一定時間操作がなければ自動的に隠す。

---

## 9. マルチプラットフォーム方針

TsubuGalleryは次の5環境を正式なターゲットとする。

| Platform | 役割 |
|---|---|
| Windows | Desktop |
| macOS | Desktop |
| Linux | Desktop |
| Android | Mobile / Tablet |
| iOS | Mobile / Tablet |

アプリケーション全体をOSごとに別実装するのではなく、Rustで記述した共通Coreを中心に構成する。

```text
                    TsubuGallery Core
                         Rust
                          │
         ┌────────────────┼────────────────┐
         │                │                │
    Language Core     Runtime/VM       Renderer
         │                │                │
         └────────────────┼────────────────┘
                          │
                   Platform Adapter
                          │
        ┌─────────┬───────┼───────┬─────────┐
        ↓         ↓       ↓       ↓         ↓
     Windows    macOS   Linux   Android     iOS
```

### 9.1 共通化する部分

以下は原則としてRustの共通コードとする。

- Sketchデータモデル
- SQLiteアクセス層
- Lexer
- Parser
- AST
- Bytecode Compiler
- VM
- Processing Lite API
- 描画コマンド生成
- サムネイル生成
- キャッシュ管理
- 検索・タグ・お気に入りロジック

### 9.2 OS依存部分

以下はPlatform Adapterへ分離する。

- ウィンドウ生成
- GPU Surface
- 全画面制御
- ファイル保存場所
- キーボード
- マウス
- タッチ
- 画面回転
- DPI / Retina対応
- アプリライフサイクル
- モバイルの一時停止・復帰
- OS共有機能
- 将来的なロック画面／壁紙連携

---

## 10. DesktopとMobileの違い

DesktopとMobileで同一UIを無理に再現しない。

共通するのは情報構造と操作概念であり、画面レイアウトは各デバイスへ適応させる。

### Desktop

```text
Gallery + Keyboard + Mouse
       ↓
Fullscreen Viewer
```

### Mobile

```text
Touch Gallery
     ↓
Fullscreen Viewer
     ↓
Swipe navigation
```

Mobileではタップ領域、スワイプ、画面回転、バックグラウンド移行などを前提として設計する。

---

## 11. マルチランゲージ方針

TsubuGalleryはUI文字列をソースコードへ直接埋め込まず、すべて翻訳リソースから取得する。

初期対応言語は以下とする。

```text
ja-JP   日本語
en-US   English
```

その後、翻訳ファイルを追加するだけで対応言語を増やせる構造にする。

例:

```text
locales/
  ja-JP.json
  en-US.json
  de-DE.json
  fr-FR.json
```

### 11.1 翻訳キー

プログラム内では表示文章そのものではなくキーを使用する。

```text
gallery.title
gallery.new_sketch
gallery.favorite
viewer.next
viewer.previous
viewer.pause
editor.save
editor.compile_error
settings.language
```

例:

```json
{
  "gallery.title": "ギャラリー",
  "gallery.new_sketch": "新しい作品",
  "viewer.next": "次の作品",
  "editor.save": "保存"
}
```

英語版:

```json
{
  "gallery.title": "Gallery",
  "gallery.new_sketch": "New Sketch",
  "viewer.next": "Next Sketch",
  "editor.save": "Save"
}
```

### 11.2 言語選択

初回起動時はOSの言語設定を参照し、対応言語が存在すれば自動選択する。

ユーザーは設定画面から明示的に変更できる。

```text
Language

○ System Default
○ 日本語
○ English
```

### 11.3 UI設計上の注意

翻訳によって文字列長は大きく変化するため、ボタン幅を日本語文字数に合わせて固定しない。

UIは以下を前提とする。

- 可変長ラベル
- Unicode
- UTF-8
- CJK文字
- アクセント付き欧文
- RTL言語への将来的拡張
- 日付・時刻・数値のロケール依存表示

作品タイトル、タグ、コメントなどのユーザーデータはUI言語とは独立し、任意のUnicode文字列を保存できるようにする。

---

## 12. 技術方針

### 12.1 Rustを中心とする

Runtimeの中心実装にはRustを使用する。

目的はJava版Processingをそのまま内包することではなく、TsubuGalleryの用途に必要なProcessing互換機能を軽量に実行することである。

JVMを必須とせず、Parser、VM、Rendererを一つのネイティブCoreとして管理する。

### 12.2 完全互換より限定互換

Processing Java Modeとの完全互換は初期目標としない。

`#つぶやきProcessing`で頻繁に使われる構文とAPIを優先し、小さな互換層を構築する。

このランタイムを本文では **Processing Lite Runtime** と呼ぶ。

---

## 13. システムアーキテクチャ

```text
┌─────────────────────────────────┐
│          Gallery UI             │
│ Thumbnail / Search / Tags       │
│ Favorites / Editor / Settings   │
└────────────────┬────────────────┘
                 │
┌────────────────▼────────────────┐
│       TsubuGallery Core         │
│ Repository / Cache / Locale     │
└────────────────┬────────────────┘
                 │
┌────────────────▼────────────────┐
│ Processing Lite Compiler        │
│ Lexer → Parser → AST → Bytecode │
└────────────────┬────────────────┘
                 │
┌────────────────▼────────────────┐
│ Fast Viewer Runtime             │
│ VM + Processing API             │
└────────────────┬────────────────┘
                 │
┌────────────────▼────────────────┐
│ GPU Renderer                    │
│ Batch / Texture / Capture       │
└────────────────┬────────────────┘
                 │
┌────────────────▼────────────────┐
│ Platform Adapter                │
└───────┬──────┬──────┬──────┬────┘
        ↓      ↓      ↓      ↓      ↓
      Win    macOS  Linux  Android  iOS
```

UI、言語処理系、実行VM、GPU描画、OS依存処理を分離する。

これにより、Gallery UIの変更がProcessing Lite Runtimeへ影響しにくくなり、逆にRuntimeの高速化がUI実装へ依存しない構造になる。

---

## 14. Processing Lite言語

### 14.1 初期対応する言語要素

初期版では以下を中心に実装する。

- 変数
- 数値型
- 真偽値
- 算術演算
- 比較演算
- 代入
- `if`
- `for`
- `while`
- 関数呼び出し
- ユーザー定義関数
- `setup()`
- `draw()`

Java固有の高度な機能は初期対象外とする。

### 14.2 初期Processing API

| 分類 | 代表的なAPI |
|---|---|
| 画面 | `size()`, `width`, `height`, `frameCount` |
| 基本描画 | `point()`, `line()`, `rect()`, `ellipse()`, `circle()`, `triangle()` |
| 色と線 | `background()`, `fill()`, `stroke()`, `noFill()`, `noStroke()`, `strokeWeight()` |
| 座標変換 | `translate()`, `rotate()`, `scale()`, `pushMatrix()`, `popMatrix()` |
| 数学 | `sin()`, `cos()`, `tan()`, `abs()`, `min()`, `max()`, `map()`, `constrain()` |
| 乱数・ノイズ | `random()`, `noise()` |
| 入力 | `mouseX`, `mouseY`, `mousePressed`, `key`, `keyPressed` |

初期版は2Dを優先し、P3Dは後段階とする。

---

## 15. コンパイルと実行モデル

TsubuGalleryの高速化における基本原則は、

> 表示時にコンパイルしない。

である。

### 15.1 保存時

```text
source
  ↓
Lexer
  ↓
Parser
  ↓
AST
  ↓
Bytecode Compiler
  ↓
Bytecode Cache
```

保存時にコンパイルし、構文エラーがある場合は直前の正常なキャッシュを保持する。

### 15.2 表示時

```text
Gallery
  ↓
Sketch selected
  ↓
Bytecode Cache
  ↓
VM
  ↓
GPU
```

Galleryから作品を選択した時点ではParserを動かさない。

---

## 16. 軽量VM

Viewer Runtimeには小さなVMを組み込む。

初期実装はスタック型VMを基本とする。

例:

```text
PUSH_CONST 0
CALL_NATIVE background 1

LOAD_GLOBAL width
PUSH_CONST 2
DIV

LOAD_GLOBAL height
PUSH_CONST 2
DIV

CALL_NATIVE translate 2
```

Processing APIはVMのネイティブ関数として実装し、描画命令をRendererへ渡す。

---

## 17. 描画エンジン

描画層はGPUを使用する。

Processing Lite側ではOS固有Graphics APIを直接扱わず、共通の描画コマンドへ変換する。

```text
Processing API
     ↓
Draw Commands
     ↓
Batch Renderer
     ↓
GPU Backend
```

線、点、矩形、三角形などは可能な範囲でバッチ化する。

Viewer用描画とサムネイル生成は同一Rendererを利用する。

---

## 18. 高速切り替え

Viewerは作品ごとにアプリを再起動しない。

```text
表示中       Sketch A
先読み       Sketch B
キャッシュ   Sketch C / D / E
```

次へ移動すると、

```text
Sketch A
   ↓
Sketch B
```

を即時切り替えし、その後Sketch Cを先読みする。

Galleryから直接別作品を選んだ場合も、キャッシュ済みBytecodeがあれば即座にロードする。

---

## 19. データ管理

初期版ではSQLiteを使用する。

### 19.1 Sketch

| フィールド | 内容 |
|---|---|
| `id` | 内部識別子 |
| `title` | 作品名 |
| `source` | Processing Liteソース |
| `created_at` | 作成日時 |
| `updated_at` | 更新日時 |
| `favorite` | お気に入り |
| `compile_hash` | ソースハッシュ |
| `compile_status` | コンパイル状態 |
| `cache_path` | Bytecode |
| `thumbnail_path` | スクリーンショット |
| `thumbnail_frame` | キャプチャ位置 |
| `last_opened_at` | 最終表示 |

### 19.2 Tag

タグは多対多関係として管理する。

```text
Sketch
  │
  ├── abstract
  ├── circles
  ├── monochrome
  └── animation
```

---

## 20. 検索・整理

Galleryでは以下で作品を絞り込めるようにする。

- タイトル
- タグ
- お気に入り
- 最近追加
- 最近表示
- コンパイル状態

将来的にはサムネイルの色や画像特徴による視覚検索も検討できる。

---

## 21. 安全性

ユーザーコードから任意のOS APIへアクセスできない構造にする。

初期版では以下を禁止する。

- 任意ファイルアクセス
- ネットワーク
- 外部プロセス起動
- FFI
- 任意ネイティブコード
- OS設定変更

Processing Lite Runtimeで公開したAPIだけを利用可能とする。

### 21.1 無限ループ対策

VMにはフレームごとの実行予算を設定する。

一定以上の命令数または時間を消費した場合、当該フレームを停止しViewer UIへ制御を戻す。

これにより、一つの作品がGallery全体を停止させることを防ぐ。

---

## 22. 性能目標

| 項目 | 目標 |
|---|---|
| Desktopコールド起動 | 概ね1秒以内を目標 |
| Gallery表示 | サムネイルを段階ロードし即応性を優先 |
| キャッシュ済み作品切り替え | 100ms未満を目標 |
| 先読み済み作品 | 1〜数フレームで表示開始 |
| 通常2D作品 | 60fpsを基本 |
| Thumbnail | Galleryスクロールを妨げない非同期ロード |
| メモリ | ViewerとThumbnail Cacheに上限を設定 |

Mobileでは端末性能、発熱、バッテリーを考慮し、必要に応じてフレームレートや解像度を調整できるようにする。

---

## 23. マルチランゲージと作品コードの分離

TsubuGalleryにおける「マルチランゲージ」には二つの意味があり得るため、設計上明確に分離する。

### 23.1 UI Language

Version 1で正式対応する。

```text
Japanese
English
```

UI、メニュー、エラーメッセージ、設定画面などの表示言語を意味する。

### 23.2 Programming Language

Version 1ではProcessing Liteを対象とする。

将来的にはFrontendを交換することで、

```text
Processing Lite
p5.js subset
GLSL
独自Creative Coding Language
```

などへ拡張できる。

```text
Processing Lite ─┐
p5.js subset ────┼─ Frontend → Common IR → VM / GPU
Future Language ─┘
```

UIの多言語化と、実行できるプログラミング言語の多言語化は別機能として管理する。

---

## 24. 設定

初期設定項目は以下とする。

```text
General
  Language
  Theme
  Start Screen

Gallery
  Card Size
  Sort Order
  Show Titles

Viewer
  Fullscreen
  Frame Rate
  Navigation
  Preload

Thumbnail
  Capture Frame
  Image Quality

Runtime
  Execution Budget
```

設定キーは内部的には言語非依存とする。

---

## 25. MVP

最初の実用版で必要な機能は以下とする。

### Gallery

- スクリーンショット・グリッド
- 作品選択
- 新規作品
- 削除
- タイトル
- お気に入り

### Editor

- Processing Liteコード入力
- 保存
- 構文エラー表示
- 実行
- サムネイル更新

### Viewer

- 全画面実行
- 前後切り替え
- 一時停止
- 全画面解除

### Runtime

- Processing Lite Parser
- AST
- Bytecode
- VM
- 基本2D API
- GPU描画

### Platform

- Windows
- macOS
- Linux

### Localization

- 日本語
- 英語
- OS言語自動判定
- 設定から言語変更

Mobile版は同じCoreを使用し、Desktop MVP安定後にAndroid、iOSへ展開する。

---

## 26. Mobile MVP

Desktop版と共通Coreを使用しながら、Mobileでは以下を追加する。

- タッチ操作
- スワイプ切り替え
- 画面回転
- Safe Area
- アプリ中断・復帰
- モバイル向けGalleryレイアウト
- 端末性能に応じた描画解像度
- バッテリー／発熱を考慮したFPS制御

---

## 27. 将来拡張

TsubuGalleryは将来的に以下へ拡張できる。

- スライドショー
- ランダム再生
- プレイリスト
- コレクション
- GIF／動画プレビュー
- 作品のImport / Export
- QRコードによる作品共有
- p5.js subset
- P3D
- Shader
- クラウド同期
- ロック画面／壁紙連携
- スクリーンセーバーモード
- TV／大型ディスプレイ表示
- Web Galleryとの連携

---

## 28. 開発原則

TsubuGalleryでは、互換性の広さより次の体験を優先する。

> Galleryで作品を見つけ、選択すると、すぐに動く。

そのため、開発優先順位は以下とする。

```text
1. Viewer速度
2. Gallery操作性
3. Processing Lite互換性
4. Thumbnail品質
5. マルチプラットフォーム
6. 多言語UI
7. 高度な編集機能
```

Processing IDEを小型化するのではなく、短いCreative Coding作品を蓄積・鑑賞するための専用アプリとして設計する。

---

## 29. 最初に作るプロトタイプ

最初のプロトタイプでは、エディタやSQLiteより先にViewerとGalleryの成立性を確認する。

### Prototype A — Runtime

Rustで一つの固定スケッチを全画面60fpsで描画する。

### Prototype B — Switching

二つ以上のスケッチをメモリに保持し、瞬時に切り替える。

### Prototype C — Thumbnail

同一Rendererからフレームを取得し、画像として保存する。

### Prototype D — Gallery

保存したスクリーンショットをグリッド表示し、選択した作品をViewerへ渡す。

### Prototype E — Compiler

外部Processing LiteコードをParser、AST、Bytecode経由で実行する。

この順序で、TsubuGalleryの核である

```text
Gallery → Select → Instant Fullscreen
```

を早い段階で検証する。

---

## 30. 開発フェーズ

| Phase | 実装内容 |
|---|---|
| Phase 1 | Rust製GPU Viewer、固定スケッチ、全画面表示 |
| Phase 2 | 複数作品の高速切り替えと先読み |
| Phase 3 | スクリーンショット取得とThumbnail Cache |
| Phase 4 | Gallery UIとサムネイル選択 |
| Phase 5 | Processing Lite Lexer / Parser / AST |
| Phase 6 | Bytecode VM、基本2D API、保存時コンパイル |
| Phase 7 | SQLite、Editor、タグ、お気に入り、検索 |
| Phase 8 | 日本語／英語UIとLocalization基盤 |
| Phase 9 | Windows / macOS / Linuxの調整と配布 |
| Phase 10 | Android対応 |
| Phase 11 | iOS対応 |
| Phase 12 | スライドショー、コレクション、追加言語、追加Frontend |

---

## 31. プロジェクト構成案

```text
tsubugallery/
├─ core/
│  ├─ model/
│  ├─ repository/
│  ├─ cache/
│  └─ locale/
│
├─ processing-lite/
│  ├─ lexer/
│  ├─ parser/
│  ├─ ast/
│  ├─ compiler/
│  └─ vm/
│
├─ renderer/
│  ├─ draw/
│  ├─ batch/
│  ├─ texture/
│  └─ capture/
│
├─ gallery/
│  ├─ grid/
│  ├─ thumbnail/
│  ├─ search/
│  └─ view-model/
│
├─ editor/
│
├─ platform/
│  ├─ windows/
│  ├─ macos/
│  ├─ linux/
│  ├─ android/
│  └─ ios/
│
├─ locales/
│  ├─ ja-JP.json
│  └─ en-US.json
│
└─ app/
```

モジュール境界を明確にし、Processing Lite RuntimeをGallery UIから独立させる。

---

## 32. TsubuGalleryの最終イメージ

TsubuGalleryはコード管理ツールではなく、**短いプログラムによって生まれる動く作品のデジタルギャラリー**である。

ユーザーはコードを書き、保存する。

TsubuGalleryはコードを実行し、代表画像を作る。

次回からユーザーはコードを探す必要がない。Galleryに並んだ作品を目で見て選ぶ。

```text
        TsubuGallery

             Gallery
               │
       ┌───────┼───────┐
       ↓       ↓       ↓
     Image   Image    Image
       │       │       │
       └───────┼───────┘
               ↓
             Select
               ↓
        Processing Lite VM
               ↓
              GPU
               ↓
        Fullscreen Artwork
```

この「**コードを保存する → 画像で探す → 即座に動かす**」という流れをTsubuGalleryの中心体験とする。

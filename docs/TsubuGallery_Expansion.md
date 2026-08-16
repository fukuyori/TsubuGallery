# 対応範囲の拡張 — 検討

対象: TsubuGallery 0.3.3 / 記録日 2026-08-16

three.js・Canvas 2D・WGSL・CUDA へ広げる場合に何が要るかを調べた記録。
**この文書の時点では、いずれも実装していない。** 判断の材料を残すことが目的で、
着手を決めた時点で設計書 (`TsubuGallery_Design.md`) 側へ移す。

## 判断の要約

| 候補 | 追加規模の目安 | 既存資産の再利用 | 判断 |
|---|---|---|---|
| **WGSL** (フラグメント) | 数百行 / 1 日 | ほぼ全部 | やる。ただし前提を決めてから |
| **WGSL** (コンピュート) | 千行前後 / 数日 | 上に載る | やる |
| **Canvas 2D** | 数千行 / 2〜3 週 | JS VM と描画は使える | やる価値がある |
| **three.js** | 数万行 / 数か月 | ほとんど無い | 見送り |
| **CUDA** (追加として) | 相互運用だけで数週間 | 無い | 見送り |
| **CUDA** (Vulkan の代わり) | — | — | **成立しない** |

## いまの構造

拡張の話は、どこに差し込むかで難しさが決まる。現状の経路は 2 本ある。

```text
起動時   source → lexer → parser → ast → compiler → bytecode
実行時   bytecode → VM → Graphics API → 三角形 → wgpu

起動時   GLSL   → 前置き → naga (glsl-in) → 検証 → WGSL
実行時   WGSL   → wgpu のパイプライン → 画面いっぱいの三角形 1 枚
```

規模の目安 (0.3.3 時点)。

| 場所 | 行数 |
|---|---|
| `processing-lite/src` (VM と Processing 前段) | 9,777 |
| `processing-lite/src/js` (p5.js 前段) | 2,927 |
| `renderer/src` | 5,245 |
| `app/src` | 7,442 |
| `core/src` | 2,831 |

---

## WGSL

### 出口はもう WGSL になっている

`renderer/src/shader.rs` の `compile()` は GLSL を naga で読み、検証し、
`wgsl_out` で WGSL を書き出している。**WGSL を受けるとは、入口の `glsl-in` を
`wgsl-in` に差し替えることに等しい。**

`wgsl-in` フィーチャは既に有効になっている。naga 30.0.0 は内部モジュールが
`wgsl-in` に依存していて `glsl-in` 単独ではビルドが通らず、その回避のために
入れてあった (`Cargo.toml` のコメント)。**依存は 1 つも増えない。**

そのまま使えるもの:

| 資産 | 場所 |
|---|---|
| フルスクリーン頂点シェーダー | `renderer/src/shader.rs` の `FULLSCREEN_VS` |
| uniform (`r` `m` `t` `f`) の配置と書き込み | `renderer/src/canvas.rs` の `ShaderStage` |
| エラー位置を元ソースの行・列へ戻す仕組み | `renderer/src/shader.rs` の `Wrapped::locate` |
| `Sketch` として動かす殻 | `processing-lite/src/glsl_sketch.rs` |

新規に要るのは WGSL 用の前置き (uniform 構造体と `@fragment` の入口)、
`Dialect::Wgsl`、判定、翻訳キー。

### 引っかかる点 1 — 判定が GLSL と衝突する

`processing-lite/src/dialect.rs` の `GLSL_ONLY` に `"vec2"` が入っている。
WGSL の `vec2<f32>` もトークンとしては `vec2` を含むため、**いまのまま WGSL を
貼ると GLSL と誤判定され、naga の glsl-in に送られて意味の分からない
エラーになる。**

WGSL を先に見る必要がある。手がかりは `@fragment` `@vertex` `@group`
`fn ` `->` `vec2<` `vec2f` あたり。GLSL には無い。

### 引っかかる点 2 — 合わせる先が無い

つぶやき GLSL には twigl の geekest という共通の土台があった。`r` `m` `t` `f`
`o` `FC` を合わせたので、**実在する投稿がそのまま動いた**。

WGSL にはそれに当たるものがない。前置きの変数名をこちらで決めることになり、
決めた瞬間に「どこにも流通していない方言」になる。実装は簡単だが、**誰の作品が
動くようになるのかがはっきりしない。** 想定する投稿元を先に決めること。

### コンピュートシェーダー

`renderer/src` に `ComputePipeline` は 1 つも無い。GPGPU 的な作品
(パーティクル、ライフゲーム、反応拡散、流体) の受け皿が未整備。

フラグメント版の上に載る。ストレージバッファ / ストレージテクスチャと
ディスパッチの制御、それを画面へ出す組み合わせが要る。**CUDA でやりたくなる
ことの大半は、ここで、移植性を落とさずに吸収できる。**

---

## Canvas 2D

### 土台はある

VM には `Object` / `Array` / `Function` と `GetProp` / `SetProp` /
`CallMethod` / `CallValue` がある (`processing-lite/src/bytecode.rs`)。
`ctx.fillRect(...)` という形はそのまま通る。描画側も `bezier` / `curve` /
`arc` / `begin_shape` / `vertex` / `text` / `blend_mode` が既にある
(`renderer/src/draw.rs`)。

### 足りないもの

| Canvas 2D の要素 | 現状 | 重さ |
|---|---|---|
| パス (`beginPath` 〜 `fill` / `stroke`) | `begin_shape` / `vertex` が近い。複数サブパスと nonzero / evenodd の塗り分けが無い | 中 |
| `save()` / `restore()` の状態スタック | 無い | 小 |
| `clip()` | 無い。scissor か stencil の追加が要る | 中 |
| `createLinearGradient` / パターン | 無い | 中 |
| `lineCap` / `lineJoin` / `setLineDash` | 無い | 小〜中 |
| `drawImage` | **テクスチャを描く口が renderer に 1 つも無い** | 大 |
| `getImageData` / `putImageData` | **CPU 側にラスタライズ像が無い** | 大 |
| `measureText` | 無い | 小 |
| `requestAnimationFrame` / `getContext('2d')` | 無い。ホスト側の見せかけが要る | 小 |

下 2 つは Canvas 2D 固有ではなく、既知の「画素の読み取り」の壁と同じもの。
**逆に言えば、そこを片付けると p5 の `image()` / `get()` / `pixels[]` も同時に
埋まる。**

### 線引き

パス・塗り・線・変形・状態スタック・テキストまでの部分集合なら 2〜3 週間で
届く。`drawImage` 系は共通の土台 (下記) を作ってから。

---

## three.js

これだけ性質が違う。**言語ではなくライブラリ**で、数百のクラスからなる
1 MB 級のコード本体。道は 2 つしかない。

### (a) 本物の three.js を動かす

JS エンジンが足りない。`class` / `new` / `this` は **Java Mode 側には有る**
(`processing-lite/src/lexer.rs`、`compiler.rs` が `this` を第 0 引数として
差し込む) のに、**p5 / JS 側には実装が無い**。README も「p5.js 側: `class`」を
未実装として挙げている。

加えて `extends`、プロトタイプ、getter / setter、`Symbol.iterator`、
型付き配列、ES モジュールが要る。その上で three.js が呼ぶ **WebGL2 か WebGPU の
API を丸ごと真似る**必要がある。ブラウザエンジンを作る規模。

### (b) 部分集合を自前で実装する

`Scene` / `PerspectiveCamera` / `Mesh` / `BoxGeometry` /
`MeshStandardMaterial` / `WebGLRenderer` …。これは 3D エンジンを書くという
ことで、しかも**実装しなかったクラスを 1 つ使われた時点でその作品は動かない。**

現状の 3D は `box()` / `sphere()` と固定カメラだけ。光源モデルもマテリアルも
テクスチャも無い。

### そもそもの相性

three.js の作品は `import` や CDN 前提で、1 ファイルの「つぶやき」として
流通していない。

---

## CUDA

### 実測した数字 (Windows / CUDA 13.3)

| もの | サイズ | 備考 |
|---|---|---|
| `nvrtc64_130_0.dll` | 96.7 MB | 実行時にカーネルをコンパイルするのに必須 |
| `nvrtc-builtins64_133.dll` | 6.4 MB | 同上 |
| `nvcuda.dll` | 4.5 MB | ドライバ同梱。配らなくてよい |
| CUDA Toolkit 全体 | 4.2 GB | 開発側にのみ必要 |

いま配っているインストーラは **7.3 MB、中身は `tsubugallery.exe` 1 つだけ**。
CUDA を足すと **103 MB 増えて 15 倍**になる。その増加分は、NVIDIA GPU を
持たない利用者にはまったく使われない。

### 動く機械が限られる

| 環境 | CUDA |
|---|---|
| Windows + NVIDIA | 動く |
| Windows + AMD / Intel | 動かない |
| macOS | 不可能 (Apple が CUDA を切って久しい) |
| Linux + NVIDIA | 動く |
| Android / iOS (設計書 Phase 10, 11) | 不可能 |

**これはギャラリーという仕組みと相性が悪い。** 作品は共有される前提で、
サムネイルは全作品ぶん生成される。CUDA 作品は AMD 機や Mac ではカードが
灰色のまま並び、`--capture-all` にも穴が開く。Processing・p5・GLSL が
どの機械でも同じ絵を出すのとは、性質が根本的に違う。

### 画面へ出す経路が無い

```text
CUDA カーネル → デバイスメモリ
                    ↓  ← ここが繋がっていない
              wgpu のテクスチャ → 画面
```

**wgpu 30 に CUDA 相互運用は無い。** 選択肢は 2 つ。

- **外部メモリで共有**: Vulkan の `VK_KHR_external_memory` から生ハンドルを
  引き出して CUDA へ渡す。wgpu の抽象を貫通するので `wgpu-hal` に降りて
  unsafe を書くことになる。DX12 が選ばれた場合は別経路が要る
- **ホストメモリ経由**: 毎フレーム PCIe を往復。1102×780×4 ≒ 3.4 MB を
  60fps で送り返すことになり、GPU で描く意味が薄れる

### 合わせる先が無い

つぶやき GLSL には twigl があった。CUDA にそれに当たるものは無い。CUDA の
コードは 1 ファイルの短文として流通しておらず、貼って動かす文化が無い。
**実装しても、動かす対象の作品が世の中に無い。**

### Vulkan の代わりに使う、という案

**成立しない。前提が違う。**

CUDA はグラフィックス API ではない。ラスタライザも、レンダーパイプラインも、
スワップチェーンも無い。**CUDA には画面へ出す方法そのものが無い。** 画素を
ウィンドウへ載せるには、必ず Vulkan / DX12 / Metal / OpenGL のいずれかが要る。

補足すると、このアプリは Vulkan に固定されていない。`wgpu::Instance::default()`
が Vulkan / DX12 / GL の 3 つから選ぶ。`WGPU_BACKEND` で 1 つずつ強制して
確かめたところ、**3 通りとも単独でサムネイル 52 件の生成に成功した**。
つまり Vulkan を外しても DX12 が引き受ける。**「Vulkan の代わり」を探すなら、
その答えは既に手元にあり、追加の実装は要らない。**

仮に本気で置き換えるとすると、こうなる。

- **ラスタライザを CUDA で書く**。三角形の設定、クリップ、深度テスト、MSAA を
  自前で実装することになる。これらは固定機能の回路として GPU に載っており、
  CUDA で書き直したものはハードウェアより遅い
- **UI も道連れ**。egui は `egui-wgpu` を通して同じフレームバッファへ描いて
  いる。描画経路を替えるなら UI 層も作り直しになる (`app/src` 7,442 行、
  `renderer/src` 5,245 行が影響範囲)
- **それでも表示だけは Vulkan か DX12 が要る**。結局 Vulkan は消えない

「CUDA で計算して、表示だけグラフィックス API に任せる」形にしても、
Vulkan は残る。**これは置き換えではなく追加であり、追加としての評価は上のとおり。**

### 代わりに

「GPU で計算して絵を作る」がやりたいことなら、**WGSL のコンピュートシェーダー**で
ほぼ同じ表現力が、上の欠点なしで手に入る。

| | CUDA | WGSL compute |
|---|---|---|
| 追加バイナリ | 103 MB | 0 |
| 動く環境 | NVIDIA のみ | 全部 (Mac・Android 含む) |
| 画面への経路 | 相互運用を自作 | 同じデバイス上でそのまま |
| 実装量 | 相互運用だけで数週間 | パイプライン追加で数日 |

CUDA でしかできないこと (cuBLAS や Tensor Core を使いたい、既存の `.cu` 資産が
ある) を具体的に想定する場合は、この判断を見直す。

---

## 共通の土台

複数の穴が 1 つの工事で埋まる。**着手するならここが最初。**

| 土台 | これで埋まるもの |
|---|---|
| テクスチャを描く口 | Canvas 2D の `drawImage`、p5 の `image()` / `loadImage()`、three.js のテクスチャ |
| 画素の読み戻し (CPU 側のラスタライズ像) | Canvas 2D の `getImageData`、p5 の `get()` / `set()` / `pixels[]` |

現状 `renderer/src` にはテクスチャを描く公開関数が 1 つも無い。

## 順序の提案

```text
1. テクスチャと画素読み戻しの基盤   ← 最も効く。3 つの穴が同時に埋まる
2. WGSL (フラグメント)              ← 1 と独立。単体で 1 日
3. WGSL (コンピュート)              ← 2 の延長。GPGPU 的な作品はここで吸収
4. Canvas 2D の部分集合             ← 1 の上に載る
—— ここまでで打ち止め ——
   three.js / CUDA
```

WGSL は 1 に依存しないので先に片付けてよい。ただし**「どの慣習に合わせるか」が
決まらないと、実装しても使い手がいない。**

## 付随して見つかったもの

調査中に気づいた、この検討とは別の直し先。

- README の「未実装」に **`GLSL のシェーダ (twigl のような作品)` が残っている**。
  0.3.x で実装済みなので記述が古い
- 同じ節の `cargo test --workspace # 485 tests` も古い。いまは 528 件
- `dialect.rs` の `GLSL_ONLY` に `"vec2"` があるため、WGSL を足す前に
  判定順序の整理が要る (上記「引っかかる点 1」)

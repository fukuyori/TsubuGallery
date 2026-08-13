# TsubuGallery

[English](README.md) ・ 日本語

短い Processing 作品を保存・実行・鑑賞するマルチプラットフォーム・ギャラリー。

設計は [`docs/TsubuGallery_Design.md`](docs/TsubuGallery_Design.md) を参照。
このリポジトリは設計書 §29 の **Prototype A〜E** をすべて実装した段階にある。

| Prototype | 内容 | 状態 |
|---|---|---|
| A | Rust で固定スケッチを全画面 60fps 描画 | 実装済み |
| B | 複数スケッチをメモリ常駐させ瞬時に切り替え | 実装済み |
| C | 同一 Renderer からフレームを取得し画像保存 | 実装済み |
| D | Gallery グリッドから選択して Viewer へ | 実装済み |
| E | Processing Lite コードを Parser → Bytecode 経由で実行 | 実装済み |

Phase 7 (SQLite / お気に入り / タグ / 検索) も入っている。

```text
Gallery → 作品を選択 → Fullscreen Viewer → Esc → Gallery
   │
   └→ N 新規 / E 編集 → Editor → ⌘S 保存 → コンパイル → Gallery
```

## 動かす

```sh
cargo run --release
```

初回起動時に同梱作品がデータ領域へ書き出され、Gallery に並ぶ。以降そこに
`.pde` を置けば自分の作品が増える。

### 操作

**Gallery**

| キー | 動作 |
|---|---|
| `↑` `↓` `←` `→` | 選択を移動 |
| `Home` / `End` | 先頭 / 末尾 |
| クリック | 選択 |
| `Enter` / `Space` / ダブルクリック | Viewer で開く |
| `R` | ランダムな作品を開く |
| `N` | 新規作成 |
| `E` | 選択中の作品を編集 |
| `Delete` / `Backspace` | 削除 (確認あり) |
| `S` / ★クリック | お気に入り |
| `T` | 選択中のサムネイルを作り直す |
| `V` | 表示方式を切り替え (グリッド → 大型カード → リスト) |
| `C` | 選択中の作品をコレクションへ出し入れ |
| `P` | スライドショーの開始 / 停止 |
| 検索欄 | タイトル・id の部分一致 |
| `F` / `F11` | 全画面 |
| `L` | UI 言語を切り替え |
| `,` / `Settings` ボタン | 設定 |
| `Esc` | 終了 |

**Viewer** (設計書 §8.1)

| キー | 動作 |
|---|---|
| `→` / `PageDown` | 次の作品 |
| `←` / `PageUp` | 前の作品 |
| `Space` | 一時停止 / 再開 |
| `P` | スライドショーの開始 / 停止 |
| `R` | ランダム |
| `T` | サムネイル更新 |
| `E` | この作品を編集 |
| `I` | 情報表示 (fps / frameCount / 切り替え時間) |
| `F` / `F11` | 全画面 |
| `L` | UI 言語を切り替え |
| `Esc` | 全画面解除、または Gallery へ戻る |

**Editor**

| キー | 動作 |
|---|---|
| `⌘S` | 保存してコンパイル |
| `⌘Enter` | 保存して実行 |
| `⌘F` | 整形 (改行とインデントを入れる) |
| `⌘K` | 短縮 (空白とコメントを削る) |
| `Esc` | 閉じる (未保存なら確認) |

`⌘` `⌥` は macOS の表記。Windows と Linux では `Ctrl` `Alt` になり、画面の
説明もその刻印で出る。

編集そのものの操作。

| キー | 動作 |
|---|---|
| `Enter` | 改行して前の行に合わせて字下げ。`{` のあとは 1 段深く |
| `Tab` / `Shift+Tab` | 選択した行をまとめて字下げ / 戻す |
| `⌘/` | 選択した行のコメントを付け外し |
| `⌘D` | 行を複製 |
| `⌥↑` / `⌥↓` | 行を上下に移動 |
| `⌘Z` / `⌘⇧Z` | 元に戻す / やり直す |

下部のエラー表示を押すと、その行へカーソルが飛ぶ。

コード欄には行番号が付き、Processing Lite の文法で色分けされる。型・キーワード・
API 関数・組み込み変数・数値・コメントを区別する。コンパイルエラーが出た行は
背景と行番号が赤くなる。

色分けの語彙 (キーワードと API 名) は実行用の Lexer と `natives` テーブルから
引いているので、言語に語を足せば色も付く。両者がずれないことはテストで固定してある。

#### 入力中のエラーチェック

保存しなくても、手が止まって 0.4 秒たつと裏でコンパイルし、エラー行を赤くする。
ファイルにも実行中の作品にも触れないので、打ち間違えても動いている絵は止まらない。
コンパイルは同梱作品で 1 回 30〜45 µs なので、1 フレーム (16 ms) には響かない。

#### 方言の判定

コンパイルに失敗したときは、どちらの方言として読んだかと、その方言で**まだ対応して
いないもの**を行番号つきで並べる。エラー位置だけ見せられても直しようがないため。

```text
p5.js として読みました。ただし、対応していない書き方があります。
  2行  まだ無い API
  2行  文字列
```

判定は当て推量なので、**コンパイルが通ったコードには何も言わない**。

#### 整形と短縮

`#つぶやきProcessing` は文字数を詰めるために 1 行へ畳んであることが多い。読むときは
**整形**、投稿するときは **短縮** で行き来する。文字数はエディタ下部に常に出る。

```processing
// 短縮 (207 文字)
int t;void setup(){size(400,400);}void draw(){background(0);for(int i=0;i<100;i++){
float a=i*.1+t*.01;float r=i*2.;noStroke();fill(255,i,255-i);circle(200.+r*cos(a),
200.+r*sin(a),4.);}if(t>100)t=0;else t++;}
```

```processing
// 整形
void draw() {
  background(0);
  for (int i = 0; i < 100; i++) {
    float a = i * 0.1 + t * 0.01;
    ...
  }
  if (t > 100)
    t = 0;
  else
    t++;
}
```

整形では、文の区切りで改行して字下げしたあと、**96 桁を超える行を括弧の中で
折り返す**。縮めて書かれた作品は 1 文が数百文字になることがあり、開いただけでは
読めないため。

```js
// 折り返し前 (150 文字)
a = (y, d = mag(k = (5 + sin(y * 2 - t / 2) * 2) * cos(i / 29), e = y / 7 - 13) - 6) => point(…)

// 折り返し後
a = (
  y,
  d = mag(k = (5 + sin(y * 2 - t / 2) * 2) * cos(i / 29), e = y / 7 - 13) - 6
) => point((q = 3 * sin(k * 2) + cos(y)) * d + w, (cos(e) + sin(k)) * d + w)
```

いちばん外側の括弧を選び、その中のカンマで割る。入れ子はまだ長ければ再帰的に
折り返す。コメントの中の括弧は数えない。元のコードにあった空行は 1 行だけ残す。

短縮では空白とコメントを削り、数値も詰める (`0.5` → `.5`、`1.0` → `1.`、`2.0f` → `2.`)。
型が変わる縮め方 (`1.0` → `1`) はしない。**変数名の付け替えはしない** ので、
手で縮めたコードほどは小さくならない。

どちらもトークンを並べ替えないので意味は変わらない。同梱作品すべてについて、変換の
前後で Bytecode が一致すること、往復しても一致することをテストで固定してある。
パースできないコードでも、トークン列が変わらないことは常に成り立つ。

Viewer の操作オーバーレイは 2.6 秒操作がないと自動的に消える (§8.2)。

### 作品の追加・編集・削除

Gallery で `N` を押すとひな形から新しい作品を作る。名前は `sketch`, `sketch-2` と
重複しないものが選ばれ、コード欄の上で変更できる。保存すると
`<data>/sketches/<名前>.pde` になる。

保存するとその場でコンパイルし直し、Gallery のサムネイルも作り直す。
**コンパイルに失敗してもファイルは必ず書く** — 入力を失わせないため。このとき
実行中のインスタンスは直前の正常なコードのまま動き続ける (設計書 §15.1 の
「構文エラーがある場合は直前の正常なキャッシュを保持する」)。

削除は取り消せないので必ず確認を挟む。`.pde` とサムネイルの両方を消す。
同梱作品を消しても次回起動で復活しない (同梱の書き出しはライブラリが空のときだけ)。

アプリの外でファイルを置いても同じように読み込まれる。エディタを使わず
好きなテキストエディタで書いてもよい。

### 探す・整理する (設計書 §20)

Gallery の上部で絞り込みと並び替えができる。

| 操作 | 内容 |
|---|---|
| 検索欄 | タイトルと id の部分一致 (大文字小文字を区別しない) |
| お気に入り | ★を付けた作品だけ |
| エラー | コンパイルできない作品だけ |
| タグ | 選んだタグが付いた作品だけ |
| 並び替え | 名前順 / 最近追加 / 最近表示 |

タグはエディタの `タグ` 欄にカンマ区切りで書く。カードの右下に表示され、
絞り込みの候補にも出る。

お気に入り、タグ、追加日時、最終表示日時は次回起動時にも残る。

### サムネイルの一括生成

ウィンドウを開かずに全作品を実行して画像にする。CI の描画スモークテストにも使える。

```sh
cargo run --release -- --capture-all ./out
```

### 環境変数

| 変数 | 効果 |
|---|---|
| `TSUBU_DATA_DIR` | データ領域の差し替え |
| `TSUBU_START_SCREEN` | 起動画面を上書き: `gallery` / `viewer` / `editor` / `settings`。指定が無ければ設定に従う |

```text
<data>/
  sketches/          作品 (*.pde) — ソースの正
  thumbnails/        <id>.png
  library.sqlite3    メタデータ (お気に入り / タグ / コレクション / 設定)
  instance.lock      起動中の印 (下記「多重起動の防止」)
  cache/             Bytecode キャッシュ (未使用。下記「保留した最適化」を参照)
```

### 多重起動の防止

同じデータ領域は 1 つのプロセスしか開けない。二重に開くと SQLite の書き込みが
競り、サムネイルの生成も二重になる。2 つ目は理由を出して終了コード 1 で止まる。

```console
$ tsubugallery
TsubuGallery はすでに起動しています。 (pid 35013)
同時に開けるのは 1 つだけです。別のデータ領域で開くには TSUBU_DATA_DIR を指定してください。
```

`--capture-all` も同じ扱い。同じデータ領域へ書くので競合は変わらない。

実体は `instance.lock` に対する OS のファイルロック
(`std::fs::File::try_lock`)。**プロセスが死ねば必ず外れる**ので、強制終了しても
次の起動を妨げない。PID ファイルのように「前回の残骸かどうか」を判定する必要が
ないのが、この方式を選んだ理由。ファイルに書く PID は、誰が握っているかを人が
読むための目安でしかない。

データ領域が違えば同時に動く。`TSUBU_DATA_DIR` を分ければ並べて確かめられる。

ロックが取れない環境 (書き込めない場所にデータ領域を置いた場合など) では、
警告を出したうえで起動は続ける。

### 設定 (設計書 §24)

`,` キーか Gallery 右上の **Settings** から開く。値を変えた瞬間に効いて、
`library.sqlite3` の `setting` テーブルへ書かれる。

| グループ | 項目 |
|---|---|
| 全般 | 言語 / 配色 (暗い・明るい) / 起動時の画面 |
| ギャラリー | 表示方式 / カードの大きさ / 並び順 / 作品名を出す |
| ビューア | 全画面で開く / フレームレート / 次の作品の選び方 / 隣の作品を先に読む / スライドショーの間隔 / スクリーンセーバー |
| サムネイル | 撮るフレーム / 画質 |
| 実行 | 1 フレームの上限 |

設定キーと値はどちらも言語に依存しない ASCII 文字列で持つ。読めない値は既定値へ
倒すので、手で書き換えて壊しても起動はする。

### 表示方式 (設計書 §6.2)

一覧の並べ方を3つ用意した。`V` キーで順に切り替わる。設定にも残る。

| 方式 | 内容 |
|---|---|
| グリッド | 既定。画面幅に応じて 2〜10 列 |
| 大型カード | 最大 3 列。1 枚を大きく見せる。文字も大きくする |
| リスト | 1 行 1 作品。方言・タグ・エラーを絵に重ねず横に置ける |

列数は上下キーの移動幅でもあるので、どの方式でも実際に使った列数を返している
(リストは 1 列)。設計書が §6.2 に挙げる残り 4 つ — お気に入りのみ / タグ別 /
最近追加した作品 / ランダム — は絞り込みと並び替え (§20) として実装済み。

### 再生 — スライドショーとスクリーンセーバー (設計書 §27)

`P` で自動送りが始まる。間隔は設定で 2〜120 秒。送る順番は「次の作品の選び方」
設定に従う (順番 / ランダム)。

**再生範囲は Gallery の絞り込みそのもの。** 別に再生キューを持たず、いま一覧に
見えている作品をその順で回す。お気に入りだけ、このタグだけ、このコレクション
だけ、で絞ってから `P` を押せば、それがプレイリストになる。矢印キーの前後移動も
同じ範囲を動く。

スクリーンセーバーは設定で待ち時間を選ぶと有効になる (既定は使わない)。無操作が
続くと全画面のスライドショーが始まり、何か操作すると元の画面と全画面状態へ戻る。
セーバー中は操作の説明を一切重ねない。解除の一打は画面へ渡さないので、うっかり
作品を消してしまうことはない。

編集中と設定中は始まらない。手を止めて画面を読んでいるだけのことがあるため。

### コレクション (設計書 §27)

作品を選んで `C`。チェックを付け外しするだけで、その場で効く。新しい名前を打って
`Add` すれば、そのコレクションが作られて同時に入る。

絞り込みバーにコレクションの選択が出る (1 つも無ければ出さない)。選べばその
コレクションだけの一覧になり、そのまま `P` でプレイリスト再生になる。

コレクションを消しても作品は消えない。作品を消すと、所属も一緒に消える
(`ON DELETE CASCADE`)。作品を改名しても所属は付いて回る (`ON UPDATE CASCADE`)。

### ソースはファイル、メタデータは DB

設計書 §19.1 は `source` も `Sketch` テーブルに置いているが、ここでは `.pde` を
ソースの正とし、DB にはそれ以外の列だけを持たせている。理由は 3 つ。

- 好きなエディタで書ける。作品は短いテキストなので、アプリを起動しないと触れない
  ほうが不便になる
- DB が壊れてもユーザーの作品は失われない。作り直せるのは付随情報だけ
- サムネイルを DB へ入れない判断 (§7.3) と同じ理由が、ソースにも当てはまる

起動時にファイル一覧と DB を突き合わせ、初めて見る作品は行を作り、アプリの外で
消された作品の行は落とす。DB が開けなくても作品は動く (お気に入りとタグを諦めるだけ)。

### キャンバスはフレームをまたいで残る

`draw()` の中で `background()` を呼ばなければ、前のフレームの絵がそのまま残る。
Processing と p5.js の挙動に合わせてある。つぶやき Processing の定番である

```java
void draw() {
  background(0, 12);   // 半透明で塗り重ねる → 残像が尾を引く
  circle(...);
}
```

も、`background()` を一度も呼ばずに描き足していく書き方も、実物と同じ絵になる。

実装は 2 枚のテクスチャを交互に使う (`renderer/src/canvas.rs`)。MSAA の解決先へ
描くため「前のフレームを読みながら同じ場所へ書く」ができないので、読む側と書く側を
分けている。サムネイルも狙ったフレームまで 1 枚ずつ積み上げてから撮るので、残像を
使う作品でも実行結果と同じ絵になる。

一時停止中は積み増さない。同じ図形を毎フレーム重ねると、止めたはずの絵が濃くなって
いってしまう。

## 2 つの方言

`.pde` を置けば動く。**Processing (Java Mode)** と **p5.js** の両方を受ける。
どちらで書かれているかは自動で判定するので、書き分けの指定は要らない
(設計書 §23.2 の Frontend 交換)。

```text
Processing Lite ─┐
                 ├─ AST → Bytecode → VM → Renderer
p5.js subset ────┘
```

Bytecode から下は共通で、VM の値だけ配列・オブジェクト・関数まで広げてある。

## Processing Lite (Java Mode)

対応範囲は設計書 §14 に合わせた限定互換で、Java Mode の完全互換は目指していない。

```processing
// 黄金角に並べた粒が、全体としてゆっくり回る。
void draw() {
  background(10);
  float s = min(width, height);
  float t = frameCount * 0.008;

  noStroke();
  pushMatrix();
  translate(width * 0.5, height * 0.5);
  rotate(t);

  for (int i = 0; i < 320; i++) {
    float f = i / 320.0;
    float angle = i * 2.399963;
    float radius = sqrt(f) * s * 0.46;
    fill(60 + f * 190, 80 + f * 40, 255 - f * 70, 235);
    circle(radius * cos(angle), radius * sin(angle), map(f, 0, 1, s * 0.03, s * 0.004));
  }

  popMatrix();
}
```

### 言語

| 分類 | 対応 |
|---|---|
| 型 | `int` `float` `boolean` `void` `String` `PVector`、1 次元配列 (`float[]` `int[]` `boolean[]` `String[]` `PVector[]`) |
| 演算 | `+ - * / %`、`== != < <= > >=`、`&& \|\|`(短絡)、`!`、三項演算子 |
| ビット演算 | `& \| ^ ~ << >> >>>` |
| 代入 | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` `++` `--` |
| 制御 | `if` / `else` / `for` / `while` / `return` / `break` / `continue` / `switch` |
| 配列 | `new float[n]`、`new float[r][c]`、`{1,2,3}`、`a[i]`、`a[y][x]`、`a.length`、拡張 for (`for (int v : a)`) |
| クラス | `class P { ... }`、フィールド、コンストラクタ、メソッド、`this`、`new P(...)`、`P[]` |
| ベクトル | `new PVector(x, y)`、`v.x` の読み書き、`v.add(u)` などのメソッド |
| キャスト | `(int)x` `(float)x` `(boolean)x` |
| リテラル | 10 進、16 進 (`0xFF6B35`)、指数 (`1e3`)、`1.0f`、文字 (`'a'` = 文字コード)、文字列 (`"..."`) |
| その他 | ユーザー定義関数 (再帰可)、グローバル変数、ブロックスコープ |

`int` 同士の演算は Java と同じく整数演算になる (`7 / 2` は `3`)。ビット演算は
両辺を 32bit 整数へ寄せ、シフト量は下位 5bit だけを見る。演算子の強さも Java に
合わせてあるので、`a & 1 == 0` が `a & (1 == 0)` になる落とし穴もそのまま。

```processing
// クラスと PVector と 2 次元配列。
class Bird {
  PVector pos, vel;
  Bird(float x, float y) {
    pos = new PVector(x, y);
    vel = new PVector(random(-2, 2), random(-2, 2));
  }
  void step() { pos.add(vel); }
  void show() { circle(pos.x, pos.y, 4 + vel.mag()); }
}

Bird[] flock = new Bird[140];
float[][] grid = new float[12][12];

void setup() {
  size(600, 600);
  for (int i = 0; i < flock.length; i++) flock[i] = new Bird(random(600), random(600));
}

void draw() {
  background(12);
  for (Bird b : flock) { b.step(); b.show(); }
}
```

```processing
// 詰めた色を取り出して使う、よくある書き方がそのまま動く。
int[] pal = {0xFF6B35, 0x4ECDC4, 0xFFE66D};
float[] y = new float[64];

void draw() {
  for (int c : pal) {
    fill((c >> 16) & 255, (c >> 8) & 255, c & 255);
    for (int i = 0; i < y.length; i++) {
      if (i % 7 == 0) continue;
      if (i > 60) break;
      circle(i * 9, y[i], (int)(6 + (i & 7)));
    }
  }
}
```

`switch` は Java と同じく、`break` が無ければ次の `case` へ落ちる。落とす書き方は
短縮に使われるのでそのまま再現してある。`switch` の中の `break` は `switch` だけを
抜け、`continue` は外側のループへ届く。

ベクトルは p5 の `createVector()` と同じもので、`add()` などは自分を書き換えて
自分を返す (`v.mult(3).add(2,0)` と数珠つなぎに書ける)。`new PVector[n]` の要素は
1 本ずつ別の実体になる。使い回すと、1 つ動かしただけで全部が動いてしまう。

`int(x)` / `float(x)` は型名と同じ綴りだが関数として呼べる。`(int)x` のキャストとは
別物で、`(` が続くかどうかで見分けている。

クラスのメソッドは `this` を第 1 引数に取る普通の関数として組み、実体ごとに
プロパティとして持たせている。メソッドの中では、フィールド名を裸で書けば
`this.x` の意味になる。

配列は 2 次元まで。`new float[r][c]` の行は 1 本ずつ別の配列になる。使い回すと、
1 行の書き換えが全部の行へ及んでしまう。

**未対応**: 3 次元以上の配列、継承、`static`、import。

### API (設計書 §14.2)

| 分類 | 関数・変数 |
|---|---|
| 画面 | `size()` (受けるが無視)、`width` `height` `frameCount` |
| 基本描画 | `point() line() rect() ellipse() circle() triangle()` |
| 自由な形 | `beginShape() vertex() curveVertex() bezierVertex() endShape()`、`arc() quad() bezier() curve()` |
| 文字 | `text() textSize() textAlign() textWidth()`、`str() nf()` |
| 形の指定 | `rectMode() ellipseMode() angleMode()`、`square()` |
| ベクトル | `createVector()`、`add sub mult div set copy mag magSq normalize limit setMag heading rotate dist dot cross lerp angleBetween` |
| 進行 | `noLoop() loop()` |
| 色の値 | `color() lerpColor()` |
| 色と線 | `background() fill() stroke() noFill() noStroke() strokeWeight()` |
| 座標変換 | `translate() rotate() scale() pushMatrix() popMatrix()` |
| 数学 | `sin() cos() tan() atan() atan2() asin() acos() abs() min() max() map() norm() constrain() sqrt() sq() pow() exp() log() floor() ceil() round() dist() mag() lerp() radians() degrees() int() float() hypot() sign() cbrt() log2() log10()` |
| 乱数・ノイズ | `random() noise() randomSeed() millis()` |
| 入力 | `mouseX` `mouseY` `mousePressed` `keyPressed` |
| 定数 | `PI` `TWO_PI` `HALF_PI` `QUARTER_PI` `RGB` `HSB` `CLOSE` `POINTS` `LINES` `TRIANGLES` `TRIANGLE_STRIP` `TRIANGLE_FAN` `CORNER` `CORNERS` `CENTER` `RADIUS` `DEGREES` `RADIANS` `LEFT` `RIGHT` `TOP` `BOTTOM` `BASELINE` |

`background()` / `fill()` / `stroke()` は Processing と同じく引数の数で切り替わる。

`beginShape()` は凹んだ形も正しく塗る。頂点を扇状に分けると凹みが外へはみ出す
ので、耳切り法で三角形に分けている。`arc()` は塗ると扇形、線は弧そのもの
(Processing の既定 `OPEN` と同じ)。

`angleMode(DEGREES)` は三角関数・逆関数・`rotate()`・`arc()` のすべてに効く。
`rectMode()` は `rect()` と `square()` に、`ellipseMode()` は `ellipse()` に効く
(`circle()` は Processing と同じく常に中心指定)。

`noLoop()` を呼ぶと `frameCount` が進まなくなる。乱数で一度だけ絵を作る作品が、
毎フレーム描き直されてちらつくのを防ぐ。

### 文字 (`text()`)

日本語も出る。字形は OS のフォントから輪郭を取り出し、自前で塗り分けてから
1 枚のテクスチャ (アトラス) へ並べる。使った字だけを焼き、同じ字は使い回す。

塗り分けは巻き数で見ているので、`o` や `あ` のように穴のある字も正しく抜ける。

図形と文字は同じ描画経路を通る。アトラスの左上に不透明な白い点を置き、
図形の頂点はそこを指すようにしてある。パイプラインが 1 本で済む代わりに、
**アトラスを GPU へ送り忘れると図形まで透明になる**。忘れられないよう、
描画関数は [`Graphics`] ごと受け取る形にしてある。

フォントが見つからない環境では `text()` は何も描かない (落ちはしない)。

`color()` が返すのは `[r, g, b, a]` の配列で、専用の型は足していない。
`fill()` / `stroke()` / `background()` はこれを受け取ると変換を通さず直に使うので、
`colorMode(HSB)` のもとでも二重変換にならない。

`size()` / `createCanvas()` を書いた作品は、そのキャンバスを縦横比を保ったまま
拡大し、画面の中央へ置く。`width` / `height` は宣言したサイズを返すので、
`createCanvas(400,400)` と書いて座標をそのまま使う作品がそのまま動く。

書かなかった作品では `width` / `height` が実際の表示サイズになる。短辺
(`min(width, height)`) を基準に書けば、どの解像度でも同じ見た目になる。

`random()` は作品 id から決まる固定シードで動く。サムネイルが実行のたびに変わらない
ようにするため。

## p5.js subset

`#つぶやきProcessing` として流通しているコードの大半は p5.js なので、そのまま
貼って動くようにしてある。

```js
t=0
$=[]
draw=_⇒{t?colorMode(HSB):createCanvas(W=720,W)
background(0,.03)
for(i=2;i--;)$[t++%W]={x:t*1.5%W,y:t*4%W,s:25,c:t%360}
$.map(p⇒fill(p.c,90,W,.1)+circle(p.x+=cos(A=noise(p.x/180,p.y/180,t/W/W)*99),p.y+=sin(A),p.s*=.99))}
```

### 対応している書き方

| 分類 | 対応 |
|---|---|
| 変数 | 型を書かない代入、`let` / `const` / `var` |
| 関数 | アロー関数 (`=>` `⇒` `→`)、`function` 宣言、関数を値として持つ (`B=blendMode`) |
| 配列 | リテラル、添字の読み書き、`length`、`map` / `forEach` / `filter` / `push` / `keys` / `entries`、`Array(n)` |
| 展開 | `[...xs]`、`[...a, b, ...c]` |
| 文字列 | `"..."` `'...'` `` `...` ``、`${}` の展開、`+` で連結、`length charAt substring indexOf split repeat toUpperCase toLowerCase trim` |
| 分割代入 | `[a,b]=[1,2]`、入れ替え `[a,b]=[b,a]`、`[o.x,v[0]]=…` |
| オブジェクト | リテラル (`{x:1}`、略記 `{x}`)、`p.x` の読み書き、`p.x+=v` |
| 式 | 代入は式、カンマ演算子、三項演算子、短絡評価、`++` / `--` の前置と後置 |
| ビット演算 | `& \| ^ ~ << >> >>>`、複合代入 (`&=` `<<=` など) |
| 制御 | `if` / `else` / `for` / `while` / `return` / `break` / `continue` / `for...of` |
| リテラル | 10 進、16 進 (`0xFF6B35`)、指数 |
| その他 | セミコロン省略 (ASI)、数値の真偽値化 (`t?…`, `for(i=2;i--;)`) |
| p5 API | `createCanvas` `colorMode(HSB)` `blendMode(ADD)` `push` / `pop`、3 引数 `noise` |
| `Math` | `Math.sin` などを組み込みへ読み替える。`Math.PI` `Math.hypot` `Math.sign` も。`S=Math.sin` と値で持てる |
| 可変長 | `min()` / `max()` は引数をいくつでも取る |

数値は JavaScript と同じく 1 種類だけ (`7/2` は `3.5`)。

```javascript
// つぶやき p5 の定番がそのまま動く。
draw=_=>{t||createCanvas(W=600,W);t=(t||0)+.02;background(8);noStroke()
for(i of [...Array(120).keys()]){
  [x,y]=[W/2+cos(i*.13+t)*(40+i*1.6), W/2+sin(i*.19+t)*(40+i*1.6)]
  c=(i*0x030507)&0xFFFFFF
  fill((c>>16)&255,(c>>8)&255,c&255,200)
  if(i%9==0)continue
  circle(x,y,3+(i&7))}}
```

### まだできないこと

- **クロージャ**。アロー関数から見えるのは自分の引数とグローバルだけ。
  関数の引数以外はすべてグローバルとして扱う
- `class` / `new` / `async`

対応していないものを使っているコードは、エディタが行番号つきで挙げる (下記)。

### 安全性 (設計書 §21)

ユーザーコードから触れるのは上表の API だけ。ファイル、ネットワーク、外部プロセス、
FFI への入り口はランタイムに存在しない。

VM は 1 フレームあたりの命令数に上限を持つ (既定 2,000 万。設定で変えられる)。超えたフレームは打ち切って
Viewer へ制御を返し、3 フレーム続けて超えた作品は停止してエラー表示に切り替える。
無限ループを書いても Gallery 全体は止まらない。

### エラー表示

コンパイルエラーは位置つきで出る。

```
3行3列: `;` がありません
```

失敗した作品も一覧からは消えず、カードにエラーバッジが付く。Viewer で開くと理由が
表示される。

## 配布 (Phase 9)

```sh
cargo build --release
```

できあがる実行ファイルは**単体で動く**。翻訳・同梱作品・SQLite はすべてバイナリに
埋め込んであるので、置く場所も作業ディレクトリも問わない。

| 項目 | 内容 |
|---|---|
| 実行ファイル | `target/release/tsubugallery` (macOS arm64 で約 14 MB) |
| 実行時の依存 | OS の標準フレームワークのみ |
| 作るもの | `~/Library/Application Support/TsubuGallery/` などデータ領域だけ |

```sh
tsubugallery --help       # 使い方
tsubugallery --version    # 版
```

### 対応プラットフォーム

| OS | 状態 |
|---|---|
| macOS (arm64) | 実機で確認 |
| Windows / Linux | `renderer` と `processing-lite` は型検査を通した。実機未確認 |

Windows と Linux 向けのクロスビルドには、その OS の C ツールチェーンが要る
(SQLite を同梱ビルドするため)。その OS 上で `cargo build --release` するのが
確実。

## 構成

設計書 §31 のモジュール境界に対応する cargo workspace。

```text
core/              library / repository / locale / paths … UI とランタイムから独立した共通層
renderer/          draw / batch / texture / capture … Processing API → 三角形 → wgpu
processing-lite/   lexer → parser → ast → compiler ─┐
                   js/{lexer,parser,ast,compiler} ──┴→ bytecode → vm
                   natives / highlight / format / dialect / examples / sketch
gallery/           grid / model / view_model        … 列数計算・選択・取得順 (UI 非依存)
app/               ui / gallery_ui / viewer_ui / editor_ui / editor
                   viewer / gfx / loader / headless
locales/           ja-JP.json / en-US.json
```

依存の向きは `app → {gallery, processing-lite, renderer, core}`、
`processing-lite → renderer`、`gallery → core`。

- Renderer は Gallery UI も Processing Lite も知らない
- Viewer はスケッチの正体を知らない (`dyn Sketch` しか見ない)
- `gallery/` に egui は入っていないので、UI を開かずにレイアウトと選択をテストできる

### 実行の流れ

```text
起動時   source → lexer → parser → ast → compiler → bytecode
表示時   bytecode → vm → natives → Graphics → 三角形 → wgpu
```

設計書 §15.2 のとおり、Gallery から作品を選んだ時点では Parser を動かさない。
p5.js subset などのフロントエンドを足すときは AST に落とせば以降を共有できる (§23.2)。

## 技術選択

| 層 | 採用 | 理由 |
|---|---|---|
| GPU | wgpu 30 | Metal / Vulkan / DX12 を 1 コードで。Android / iOS も同じ経路 |
| ウィンドウ | winit 0.30 | 5 プラットフォーム共通のイベントループ |
| UI | egui 0.36 (egui-wgpu) | Viewer と wgpu サーフェスを 1 枚共有でき、切り替えが 1 フレーム |
| 描画 | 自前バッチレンダラ | 図形を全部三角形へ展開し原則 1 ドローコール。サムネイルと共用 |
| 言語処理系 | 自前 | 対応範囲が §14 に限られるので、依存を増やす理由がない |
| メタデータ | rusqlite (bundled) | 設計書 §19 の指定。bundled で 5 プラットフォーム共通 |

MSAA は 4x。Viewer もサムネイルも同じ `BatchRenderer` を通る。
合成方法 (`blendMode`) は区間ごとにパイプラインを切り替える。1 種類しか使わない
作品では区間が 1 つなので、実質 1 ドローコールのまま。
フレームバッファは非 sRGB を選ぶ。頂点色を sRGB のまま渡すためで、結果として
アルファ合成が Processing と同じ空間で起き、egui の色も正しくなる。

## 保留した最適化

**Bytecode のディスクキャッシュ (設計書 §15.1)。** 同梱作品のコンパイルは 1 本あたり
1000 命令未満で、6 本まとめても起動時間に現れない。設計書が求める「表示時に
コンパイルしない」は起動時の一括コンパイルで満たしているので、シリアライズ形式を
足すのは作品数が増えて実測で効くようになってからにした。`<data>/cache/` は
その置き場として空けてある。

## 未実装

- Processing Lite (Java Mode) 側の言語拡張: 継承、3 次元以上の配列、`static`
- p5.js 側: `class`、オブジェクトの分割代入
- 未実装の p5 API: 画像系 (`image` / `pixels`)、`strokeCap` / `strokeJoin`、
  `frameRate()`、3D (`box` / `sphere`)
- Import / Export、GIF・動画の書き出し (設計書 §27)
- OS のスクリーンセーバーとしての登録 (macOS `.saver` / Windows `.scr`)
- Android / iOS (Phase 10, 11)
- P3D とシェーダ

## 開発

```sh
cargo test --workspace      # 409 tests
cargo clippy --workspace --all-targets
```

日本語 UI には OS の CJK フォントを借りる (`app/src/fonts.rs`)。見つからない環境では
起動時に英語へ切り替わる。

egui の画面 (Gallery / Editor) はウィンドウを開かずにテストしている。合成した
`RawInput` を流して 1 フレーム組み立て、実際に頂点が出ているか、ショートカットが
繋がっているかを確認する (`app/src/editor_ui.rs`, `app/src/gallery_ui.rs` のテスト)。

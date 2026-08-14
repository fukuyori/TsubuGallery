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
| `O` / ↗クリック | リンクをブラウザで開く |
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
| `I` | 情報表示 (作者 / リンク / fps / **CPU 負荷** / 作品の実行時間 / フレームあたりの命令数と三角形 / frameCount / 切り替え時間) |
| `O` | リンクをブラウザで開く |
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
| 検索欄 | タイトル・id・作者の部分一致 (大文字小文字を区別しない) |
| お気に入り | ★を付けた作品だけ |
| エラー | コンパイルできない作品だけ |
| タグ | 選んだタグが付いた作品だけ |
| 並び替え | 名前順 / 最近追加 / 最近表示 |

タグはエディタの `タグ` 欄にカンマ区切りで書く。カードの右下に表示され、
絞り込みの候補にも出る。

### 作者とリンク

エディタの `作者` と `リンク` 欄に書く。設計書 §19.1 の表には無いが、つぶやきの
作品は「誰の、どの投稿か」が大事なので足した。作者はカードの右下とリスト表示、
ビューアの情報表示 (`I`) に出る。検索欄でも探せる。

リンクは `O` かリスト表示の ↗ ボタンで既定のブラウザが開く。

**開くのは `http://` と `https://` だけ。** リンクは作品と一緒に配られてくる値で、
それを外部のプログラムへ渡すことになる。`file:` や `javascript:` を開けてしまうと、
作品を配った側が受け取った側の環境で何かを起こせる。空白や制御文字が混ざったもの
も開かない (引数の切れ目を作られないため)。開けない形のリンクにはボタンを出さない。

Windows では `cmd /C start` を使わない。URL の `&` を cmd が自分で解釈してしまう
ので、`rundll32 url.dll,FileProtocolHandler` を通す。

作品のコードからリンクを開くことはできない。押したときだけ開く (設計書 §21 の
砂場はそのまま)。

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
| ビューア | 全画面で開く / **画面への収め方** / フレームレート / 次の作品の選び方 / 隣の作品を先に読む / スライドショーの間隔 / スクリーンセーバー |
| サムネイル | 撮るフレーム / 画質 |
| 実行 | 1 フレームの上限 |

**画面への収め方**は、作品が宣言したキャンバスと窓の形が違うときにどうするかを
決める。つぶやき系はたいてい正方形なので、横長の窓では左右に余白の帯が出る。
「収める」(既定) はキャンバス全体を見せ、「埋める」は窓が埋まるまで拡大して、
はみ出したぶんは切る。サムネイルにも同じ設定が効くので、ギャラリーのカードと
Viewer の見え方が揃う。

設定キーと値はどちらも言語に依存しない ASCII 文字列で持つ。読めない値は既定値へ
倒すので、手で書き換えて壊しても起動はする。

### `I` で出るもの

`I` を押すと、いま動いている作品の「値段」が Viewer の上に出る。

| 行 | 意味 |
|---|---|
| CPU 負荷 | 主スレッドが実時間のうちどれだけ働いているか。内訳の `仕事 / 間隔` も添える。画面の空きを待つ時間は仕事に数えないので、早く終わる作品は低く出る。100% で止まる — 糸 1 本はそれ以上使えない |
| 作品の実行 | `draw()` にかかった時間。VM と図形の組み立て |
| 命令数 / フレーム | 作品が実行したバイトコードの数。設定の実行予算 (Runtime) はこの単位 |
| 三角形 / フレーム | GPU へ渡した量 |

重い作品がどこで重いかは、この 4 つで分かる。命令数が多くて三角形が少なければ
言語側に無理をさせている。逆なら描画側。

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

### 描画状態は作品ごとに持つ

Viewer は切り替えで何も作り直さずに済むよう、全作品をインスタンス化したまま
抱える (設計書 §18)。一方で `Graphics` はギャラリー全体で 1 つを使い回す。
この組み合わせには罠がある。作品の `setup()` は一度きりしか走らないので、
そこで決めたこと — `stroke(-1)`、`size()`、`colorMode(HSB)`、`textSize()`、
3D かどうか — は、他の作品のために状態を初期化した瞬間に永久に失われる。
白い線を `setup()` でだけ決め、`draw()` が `clear()` で始まる作品は、
黒地に黒い線を引くことになり、画面が真っ黒のままになる。先読みはこれを
悪化させる。`setup()` が**別の** `Graphics` の側で走ってしまうため。

そこで、作品から離れるときにその状態を預け ([`GraphicsState`])、戻ってきたら
復す。先読みも、温めた結果を預けておく。捨てるのは 1 フレーム限りのもの —
座標変換のスタックや、閉じ忘れた `beginShape()` — だけ。

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

ただし、捨てられた積み重ねを描き戻す者が要る。作品の切り替えでも窓の大きさ変更でも
キャンバスは捨てられるが、静的モード (設計書 §14.1) の作品には描き直す `draw()` が
無い。絵は全部 `setup()` の中にあり、それは一度きりしか走らない。そこでこの手の
作品は、キャンバスが捨てられるたびに頭から動かし直す。乱数の出発点も戻すので、
さっき見た絵、そしてギャラリーのカードと同じ絵が出る。

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
| 代入 | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` `++` `--` (前置・後置とも)。式の中でも書ける (`line(x, y, x += dx, y)`) |
| 制御 | `if` / `else` / `for` / `while` / `return` / `break` / `continue` / `switch` |
| 配列 | `new float[n]`、`new float[r][c]`、`new int[]{1,2}`、`{1,2,3}`、`a[i]`、`a[y][x]`、`a.length`、拡張 for (`for (int v : a)`) |
| クラス | `class P { ... }`、フィールド、コンストラクタ、メソッド、`this`、`new P(...)`、`P[]` |
| ベクトル | `new PVector(x, y)`、`v.x` の読み書き、`v.add(u)` などのメソッド |
| キャスト | `(int)x` `(float)x` `(boolean)x` |
| リテラル | 10 進、16 進 (`0xFF6B35`)、指数 (`1e3`)、`1.0f`、文字 (`'a'` = 文字コード)、文字列 (`"..."`) |
| 宣言 | `float a = 1, b;` のように 1 文へ複数書ける |
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

### 静的モード

`setup()` も `draw()` も書かず、関数の外に文を並べただけの作品も動く。全体を
`setup()` の中身として 1 回だけ描く。Processing と同じ扱いで、つぶやきの短い
コードはこの形が多い。

```processing
float r, i, d;
size(720, 720);
strokeWeight(2);
for (d = 960; d > 9; d -= 80)
  for (r = 0; r < TAU; r += PI / d * 5) {
    resetMatrix();
    translate(cos(r) * d / 2 + 360, sin(r) * d / 2 + 360);
    ...
  }
```

`background()` を呼ばない作品の地は、方言に合わせる。Processing なら灰 204、
p5.js なら白 (p5 のキャンバスは透明で、後ろのページの白が透ける)。これは
見た目の趣味ではない。半透明を塗り重ねる作品は完全には濁りきらないので、
下地の色が絵全体の明るさの土台になる。p5 の作品を灰の上に置くと、淡い
色調が濁って別物になる。黒はどちらにとっても誤り — この種の作品は既定の
黒い線で描くので、何も見えなくなる。

### API (設計書 §14.2)

| 分類 | 関数・変数 |
|---|---|
| 画面 | `size()` / `createCanvas()` (宣言したキャンバスを表示領域へ収める)、`width` `height` `frameCount` |
| 基本描画 | `point() line() rect() ellipse() circle() triangle()`。`rect()` は 5 個目以降の引数で角が丸くなる。`point()` は丸い点で、太い線の端も丸い (どちらの本家もそう) |
| 自由な形 | `beginShape() vertex() curveVertex() bezierVertex() endShape()`、`arc() quad() bezier() curve()` |
| 文字 | `text() textSize() textAlign() textWidth()`、`str() nf()`、`String.fromCodePoint()` |
| 形の指定 | `rectMode() ellipseMode() angleMode()`、`square()` |
| ベクトル | `createVector()`、`add sub mult div set copy mag magSq normalize limit setMag heading rotate dist dot cross lerp angleBetween` |
| 進行 | `noLoop() loop()` `clear()` |
| 色の値 | `color() lerpColor()`、成分の取り出し `red() green() blue() alpha() hue() saturation() brightness()` |
| 色と線 | `background() fill() stroke() noFill() noStroke() strokeWeight()` |
| 座標変換 | `translate() rotate() scale() pushMatrix() popMatrix() pushStyle() popStyle() resetMatrix()`。`translate()` と `scale()` は 3 引数、`rotate()` は `(角度, x, y, z)` も取る |
| 3D | `size(w, h, P3D)`、`box() sphere() rotateX() rotateY() rotateZ() lights() noLights()` |
| 数学 | `sin() cos() tan() atan() atan2() asin() acos() abs() min() max() map() norm() constrain() sqrt() sq() pow() exp() log() floor() ceil() round() dist() mag() lerp() radians() degrees() int() float() hypot() sign() cbrt() log2() log10()` |
| 乱数・ノイズ | `random() randomGaussian() noise() randomSeed() millis()` |
| 入力 | `mouseX` `mouseY` `mousePressed` `keyPressed` |
| 定数 | `PI` `TWO_PI` `TAU` `HALF_PI` `QUARTER_PI` `RGB` `HSB` `CLOSE` `POINTS` `LINES` `TRIANGLES` `TRIANGLE_STRIP` `TRIANGLE_FAN` `CORNER` `CORNERS` `CENTER` `RADIUS` `DEGREES` `RADIANS` `LEFT` `RIGHT` `TOP` `BOTTOM` `BASELINE` |

`background()` / `fill()` / `stroke()` は Processing と同じく引数の数で切り替わる。
`stroke(-1)` のように **int をひとつ渡すと、詰めた色 (`0xAARRGGBB`) として読む**。
`-1` は不透明な白。Processing は型で見分けるので、こちらも `int` のときだけそう
読む (`fill(128.0)` は今までどおり明度)。

`clear()` は積んだ絵を捨てる。Processing では透明になるが、ここでは黒で塗る。
透明のままだと書き出したサムネイルが透けて、白い線の作品が見えなくなる。画面では
黒地に重ねて表示するので、見た目は変わらない。

`beginShape()` は凹んだ形も正しく塗る。頂点を扇状に分けると凹みが外へはみ出す
ので、耳切り法で三角形に分けている。`arc()` は塗ると扇形、線は弧そのもの
(Processing の既定 `OPEN` と同じ)。

`angleMode(DEGREES)` は三角関数・逆関数・`rotate()`・`arc()` のすべてに効く。
`rectMode()` は `rect()` と `square()` に、`ellipseMode()` は `ellipse()` に効く
(`circle()` は Processing と同じく常に中心指定)。

`noLoop()` を呼ぶと `frameCount` が進まなくなる。乱数で一度だけ絵を作る作品が、
毎フレーム描き直されてちらつくのを防ぐ。

`noise()` は Processing の実装そのものに合わせてある。乱数表を余弦で補間し、
4 オクターブを 0.5 の減衰で重ねる (Processing の既定 `noiseDetail(4, 0.5)`)。
効いてくる癖が 2 つあり、どちらも再現している。**負の座標は折り返す** ―
`noise(-3, 0)` は `noise(3, 0)` と同じ ― ので、原点をまたいで座標を振る作品は
左右対称に出る。そして 4 オクターブぶんの重みで値が 0.5 付近へ寄るため、
`noise(...) > .6` のような閾値が本家と同じ割合を拾う。乱数表そのものは違うので
模様は一致しない。揃うのは値の散らばり方と、この 2 つの癖。

### 3D (P3D)

`size(w, h, P3D)` で遠近のついたカメラに切り替わる。既定値は Processing と
同じで、視野角 60 度、視点は `z = (height/2) / tan(30°)`。この距離だと
`z = 0` の平面が 1 単位 1 ピクセルで写るので、2D のつもりで書いた `rect()` が
2D と同じ場所に出る。p5.js の `createCanvas(w, h, WEBGL)` も動く。違いは
原点がキャンバスの左上ではなく中央にあることだけ。

`resetMatrix()` はカメラごと消える。視点が世界の原点に移り `-Z` を向くので、
原点まわりに立体を並べる書き方がそのまま使える。つぶやき系の作品はこれを
よく使う。

`lights()` は Processing の既定と同じで、環境光 128 と、視点から差す平行光
128。陰影は面ごとに 1 色。`box()` / `sphere()` は `noStroke()` を書かない限り
いまの線の色で縁取られ、隠れる稜線は深度バッファが落とす。

変換はすべて CPU で行い、2D と同じ三角形の列にして GPU へ渡す。GPU 側に
増えるのは深度バッファだけ。その代わりの限界がいくつかある。

- 頂点色は画面空間で混ざるので、大きく傾いた三角形のグラデーションが
  Processing とわずかにずれる。`box()` のように面ごとに 1 色なら差は出ない
- 視点をまたぐ面は切らずに丸ごと落とすので、カメラが立体の中に入ると
  一部が消える
- 法線はモデル行列の左上 3x3 で移すため、軸ごとに違う倍率の `scale()` を
  かけると陰影がわずかにずれる

まだ無いもの: `camera()`、`perspective()`、`ortho()`、立方体と球以外の立体
(`cylinder` / `cone` / `torus` / `plane`)、`texture()`、`z` つきの `vertex()`。
`ambientLight()` / `directionalLight()` / `pointLight()` は受け付けるが、
既定の明かりを点けるだけ。

### 文字 (`text()`)

日本語も出る。字形は OS のフォントから輪郭を取り出し、自前で塗り分けてから
1 枚のテクスチャ (アトラス) へ並べる。使った字だけを焼き、同じ字は使い回す。

塗り分けは巻き数で見ているので、`o` や `あ` のように穴のある字も正しく抜ける。

図形と文字は同じ描画経路を通る。アトラスの左上に不透明な白い点を置き、
図形の頂点はそこを指すようにしてある。パイプラインが 1 本で済む代わりに、
**アトラスを GPU へ送り忘れると図形まで透明になる**。忘れられないよう、
描画関数は [`Graphics`] ごと受け取る形にしてある。

フォントは 1 本ではなく、前から順に探す。日本語のフォントに麻雀牌やトランプの
記号は入っていないことが多く、記号のフォントに日本語は入っていない。CJK →
記号 の順に試し、字形を持っているものを使う。

どれにも無い字は描かない。フォントが 1 本も見つからない環境でも落ちはしない。

**どの記号フォントを使うかで絵が変わる。** 同じ字でもフォントごとに大きさも
ベースラインからの位置も違い、字の後ろに図形を敷く作品は、作者の手元にあった
フォントに合わせて座標が決めてある。作品はたいてい Windows か Web ブラウザで
書かれているので、`seguisym.ttf` (Segoe UI Symbol) を最初に探し、Noto の記号
フォント、macOS の Apple Symbols と続く。macOS の Apple Symbols は
`textSize(99)` のとき麻雀牌を Segoe より 19px 低く描くので、後ろに敷いた
カードから牌がはみ出す。自分で入れたフォントも拾う — `~/Library/Fonts`、
`~/.fonts`、`~/.local/share/fonts`、`%LOCALAPPDATA%/Microsoft/Windows/Fonts` を
OS の置き場と一緒に探す。

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
| 配列 | リテラル、添字の読み書き、`length`、`Array(n)` |
| 配列のメソッド | `push pop shift unshift at slice splice concat reverse fill flat join indexOf lastIndexOf includes sort keys entries`、コールバックを取る `map forEach filter flatMap find findLast findIndex some every reduce` |
| 展開 | `[...xs]`、`[...a, b, ...c]`、引数の並びでも: `stroke(...c, 9)`、`Math.max(...xs)` |
| 文字列 | `"..."` `'...'` `` `...` ``、`${}` の展開、`+` で連結、`length charAt substring indexOf split repeat toUpperCase toLowerCase trim` |
| 分割代入 | `[a,b]=[1,2]`、入れ替え `[a,b]=[b,a]`、`[o.x,v[0]]=…` |
| オブジェクト | リテラル (`{x:1}`、略記 `{x}`)、`p.x` の読み書き、`p.x+=v` |
| 式 | 代入は式、カンマ演算子、三項演算子、短絡評価、`++` / `--` の前置と後置 |
| ビット演算 | `& \| ^ ~ << >> >>>`、複合代入 (`&=` `<<=` など) |
| 制御 | `if` / `else` / `for` / `while` / `return` / `break` / `continue` / `for...of` |
| リテラル | 10 進、16 進 (`0xFF6B35`)、指数 |
| その他 | セミコロン省略 (ASI)、数値の真偽値化 (`t?…`, `for(i=2;i--;)`) |
| p5 API | `createCanvas` (`WEBGL` も) `colorMode(HSB)` `blendMode(ADD)`、3 引数 `noise`、`drawingContext` の影 |
| `push` / `pop` | p5 と同じく座標変換**と**見た目の両方を退避する。座標変換だけの Processing の `pushMatrix()` とは違う。`pushStyle()` / `popStyle()` もある |
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

p5.js の `text()` は塗りだけでなく線でも描く。Processing の `text()` は塗り
だけ。この違いは効いてくる — 白いカードに白い字を置くと、縁が無ければ何も
見えない。字形は塗りつぶした形しか持っていないので、縁は小さな円の上に 8 回
線の色で重ねて作り、そのうえに塗りを置いている。

### 影 (`drawingContext`)

`drawingContext` はブラウザのキャンバスそのもので、ここには無い。代わりに、
影の指定だけを読み返せる入れ物を渡す。`shadowBlur`、`shadowColor`、
`shadowOffsetX`、`shadowOffsetY` が効く。白い地に白いカードを並べるような、
影だけで成り立っている作品があり、これが無いと何も見えない。

ぼけは本物のガウスぼかしではない。同じ形を影の色で何十枚か、外へいくほど
薄くなる輪の上にずらして重ねている。やわらかい影として読める程度には近く、
図形側のコードを一切変えずに済む — 角の丸い `rect()` も、文字も、`box()` も
同じように影がつく。代わりに、影のついた図形は三角形が 30 倍ほどに増えるので、
1 フレームに何千個も影を落とす作品では効いてくる。

### `createCanvas()` はキャンバスを作り直す

p5 の `createCanvas()` は、呼ぶたびにキャンバスの要素を作り直す。描いてあった
ものは消え、描画の文脈も初期化されて、塗りと線は既定へ、線の太さは 1 へ、
座標変換は単位行列へ戻る。`noFill()` と `noStroke()` は残る。あれは p5 側の
旗で、キャンバスに載っていないため。

作品はこれを使う。`draw()` の頭で `createCanvas()` を呼ぶのが画面を消す手口で、
毎フレーム `colorMode()` や `noStroke()` を呼び直しているのもそのため。ここを
違えると、半透明を重ねる作品が消えずに積もり、数フレームで彩度が振り切れて
まったく別の絵になる。

Processing の `size()` にこの働きは無いので、そちらはそのまま。

### 受け付けるが効かないもの

`drawingContext` のそれ以外 (`filter`、`globalCompositeOperation`、グラデーション)
は黙って捨てる。書いた作品も止まらずに動く。

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
- **画素の読み取り** (`get()` / `set()` / `pixels[]`)。描いた絵は GPU にしか無く、
  CPU 側にラスタライズされた像を持っていない。同じフレームの中で読み書きする
  作品 (砂が積もる、成長する、当たり判定を取る) を動かすには、CPU 側の
  ラスタライザを別に持つ必要がある
- 未実装のその他の p5 API: 画像 (`image` / `loadImage`)、`strokeCap` / `strokeJoin`、
  `frameRate()`
- Import / Export、GIF・動画の書き出し (設計書 §27)
- OS のスクリーンセーバーとしての登録 (macOS `.saver` / Windows `.scr`)
- Android / iOS (Phase 10, 11)
- **GLSL のシェーダ** (twigl のような作品)

## 開発

```sh
cargo test --workspace      # 473 tests
cargo clippy --workspace --all-targets
```

日本語 UI には OS の CJK フォントを借りる (`app/src/fonts.rs`)。見つからない環境では
起動時に英語へ切り替わる。

egui の画面 (Gallery / Editor) はウィンドウを開かずにテストしている。合成した
`RawInput` を流して 1 フレーム組み立て、実際に頂点が出ているか、ショートカットが
繋がっているかを確認する (`app/src/editor_ui.rs`, `app/src/gallery_ui.rs` のテスト)。

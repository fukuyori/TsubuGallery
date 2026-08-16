"""アプリアイコンを描く。

生成物は 2 つ。

    app/assets/icon.ico   exe とインストーラへ埋める Windows 用
    app/assets/icon.png   winit のウィンドウアイコン (256px)

描いた PNG をリポジトリへ置くのではなく、描く手順のほうを置いている。色や
粒の数を変えたくなったとき、元絵を探さずにここだけ直せばよいため。

    python scripts/make-icon.py

図案は「つぶ」が渦を描いて外へ散っていくところ。同梱作品の spiral.pde から
取った。16px まで縮むことを考えて粒は 9 つに絞り、外へ行くほど大きくしている。
数を増やすと小さい寸法で潰れて、ただの染みになる。
"""

import colorsys
import math
import pathlib

# 実寸の 8 倍で描いてから縮める。円の縁を滑らかにするのがねらいで、
# PIL の描画そのものにアンチエイリアスは無い。
SUPERSAMPLE = 8
BASE = 256
CANVAS = BASE * SUPERSAMPLE

# Gallery の地と同じ色 (app/src/main.rs の GALLERY_BACKGROUND)。
BACKGROUND = (18, 18, 21)

# 粒の数。増やすと小さい寸法で潰れる。
GRAINS = 7

# 図案のまわりに残す余白 (辺の長さに対する割合)。
MARGIN = 0.13

# 渦の開き方。1 つ外側の粒は、中心からの距離も直径もこの倍率になる。
GROWTH = 1.42
# 1 つ進むごとに回す角。tau/5 なら 1 周に 5 粒で、7 粒なら 1 周半に近い。
TURN = math.tau / 5
# 隣どうしの空き。1.0 でちょうど接する。少し離すほうが粒の数を数えやすい。
GAP = 1.16

# ICO へ入れる寸法。Windows はエクスプローラの表示倍率ごとに選び分ける。
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASSETS = ROOT / "app" / "assets"


def rounded_rect_mask(size: int, radius: float):
    """角を丸めた正方形の透過マスク。"""
    from PIL import Image, ImageDraw

    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [(0, 0), (size - 1, size - 1)], radius=radius, fill=255
    )
    return mask


def draw() -> "Image.Image":
    from PIL import Image, ImageDraw

    image = Image.new("RGB", (CANVAS, CANVAS), BACKGROUND)
    draw = ImageDraw.Draw(image)

    # 対数螺旋。1 つ進むごとに中心からの距離を GROWTH 倍し、粒の大きさも同じ
    # 割合で大きくする。全体が相似形になるので、どの 2 つを取っても隣どうしの
    # 空き具合が等しくなり、粒が「つながった 1 本の渦」として読める。
    #
    # 粒の大きさは、隣どうしがちょうど接する値から GAP のぶんだけ縮めて決める。
    # 隣の中心間の距離は余弦定理で出る。
    chord = math.sqrt(1 + GROWTH**2 - 2 * GROWTH * math.cos(TURN))
    size_ratio = chord / ((1 + GROWTH) * GAP)

    grains = []
    for i in range(GRAINS):
        t = i / (GRAINS - 1)
        angle = i * TURN - math.pi * 0.35
        distance = GROWTH**i
        r = distance * size_ratio
        # 色相は水色から紫を通って桃色へ。暗い地の上でどれも明るく見える範囲。
        hue = (0.50 + 0.42 * t) % 1.0
        rgb = colorsys.hsv_to_rgb(hue, 0.68 - 0.20 * t, 1.0)
        grains.append((math.cos(angle) * distance, math.sin(angle) * distance, r, rgb))

    # 外接矩形を測って、余白 MARGIN を残して目一杯に収める。渦は左右非対称なので
    # 原点を画面中心に置くだけでは寄って見えるし、上の係数をいじるたびに大きさが
    # 変わってしまう。ここで正規化しておけば、形だけを気にすればよくなる。
    left = min(x - r for x, _, r, _ in grains)
    right = max(x + r for x, _, r, _ in grains)
    top = min(y - r for _, y, r, _ in grains)
    bottom = max(y + r for _, y, r, _ in grains)
    scale = CANVAS * (1.0 - 2 * MARGIN) / max(right - left, bottom - top)
    offset_x = CANVAS / 2 - (left + right) / 2 * scale
    offset_y = CANVAS / 2 - (top + bottom) / 2 * scale

    for x, y, r, rgb in grains:
        x = x * scale + offset_x
        y = y * scale + offset_y
        r = r * scale
        color = tuple(int(round(c * 255)) for c in rgb)
        draw.ellipse([(x - r, y - r), (x + r, y + r)], fill=color)

    image = image.resize((BASE, BASE), Image.LANCZOS)

    # 角丸は縮めたあとで抜く。先に抜くと縮小で縁が濁る。
    out = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
    out.paste(image, (0, 0), rounded_rect_mask(BASE, BASE * 0.22))
    return out


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    icon = draw()

    png = ASSETS / "icon.png"
    icon.save(png)

    # ICO の各寸法は 256px からの縮小で作る。Pillow の save(sizes=...) に任せると
    # 縮小方法を選べないので、こちらで LANCZOS を指定して焼く。
    ico = ASSETS / "icon.ico"
    frames = [icon.resize((s, s), Image.LANCZOS) for s in ICO_SIZES]
    frames[-1].save(ico, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])

    print(f"{png}  {png.stat().st_size:,} bytes")
    print(f"{ico}  {ico.stat().st_size:,} bytes")


if __name__ == "__main__":
    from PIL import Image  # noqa: F401  (draw() の型注釈のために先に触れておく)

    main()

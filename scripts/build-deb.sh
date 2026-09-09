#!/usr/bin/env bash
# Debian / Ubuntu 上で TsubuGallery の .deb を作る (sudo 不要)。
set -euo pipefail

usage() {
    cat <<'EOF'
使い方: scripts/build-deb.sh [--skip-build]

release ビルドから target/installer/tsubugallery_<version>_<arch>.deb を作ります。
  --skip-build  target/release/tsubugallery を使い、ビルドを省略
  -h, --help    この説明を表示

必要なもの: Rust、C/C++ ビルド環境、dpkg-dev、binutils
実行した Debian / Ubuntu の amd64 / arm64 用に作成します (クロスビルド非対応)。
EOF
}

die() { echo "error: $*" >&2; exit 1; }
SKIP_BUILD=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --skip-build) SKIP_BUILD=1 ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "知らないオプション: $1" ;;
    esac
    shift
done

[ "$(uname -s)" = Linux ] || die 'Debian / Ubuntu 上で実行してください。'
for tool in dpkg dpkg-deb dpkg-shlibdeps readelf awk install mktemp du sed; do
    command -v "$tool" >/dev/null || die "$tool がありません。dpkg-dev / binutils をインストールしてください。"
done
if [ "$SKIP_BUILD" -eq 0 ]; then
    command -v cargo >/dev/null || die 'cargo がありません。Rust をインストールしてください。'
    command -v rustc >/dev/null || die 'rustc がありません。Rust をインストールしてください。'
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="$ROOT/target/installer"
BIN="$ROOT/target/release/tsubugallery"
ARCH="$(dpkg --print-architecture)"
case "$ARCH" in
    amd64) MACHINE='Advanced Micro Devices X86-64' ;;
    arm64) MACHINE='AArch64' ;;
    *) die "未対応のアーキテクチャ: $ARCH (amd64 / arm64 に対応)" ;;
esac
VERSION="$(awk '
    /^\[workspace\.package\]/ { section = 1; next }
    /^\[/ { section = 0 }
    section && /^version[[:space:]]*=/ { gsub(/[",]/, "", $3); print $3; exit }
' "$ROOT/Cargo.toml")"
[ -n "$VERSION" ] || die 'Cargo.toml から version を読めませんでした。'
# SemVer のプレリリースは Debian でも正式版より前に並べる。
VERSION="${VERSION/-/~}"
dpkg --validate-version "$VERSION"

if [ "$SKIP_BUILD" -eq 0 ]; then
    HOST="$(rustc -vV | sed -n 's/^host: //p')"
    [ -n "$HOST" ] || die 'Rust のホストターゲットを取得できませんでした。'
    (cd "$ROOT" && cargo build --locked --release -p tsubu-app --bin tsubugallery \
        --target "$HOST" --target-dir "$ROOT/target")
    BIN="$ROOT/target/$HOST/release/tsubugallery"
fi
[ -x "$BIN" ] || die "$BIN がありません。ネイティブの release ビルドが必要です。"
ACTUAL_MACHINE="$(LC_ALL=C readelf -h "$BIN" | sed -n 's/^[[:space:]]*Machine:[[:space:]]*//p')"
[ "$ACTUAL_MACHINE" = "$MACHINE" ] || die "バイナリの CPU ($ACTUAL_MACHINE) が $ARCH と一致しません。"

mkdir -p "$OUTPUT_DIR"
BUILD="$(mktemp -d "$OUTPUT_DIR/.deb-build.XXXXXX")"
trap 'rm -rf -- "$BUILD"' EXIT
STAGE="$BUILD/root"
install -Dm755 "$BIN" "$STAGE/usr/bin/tsubugallery"
install -Dm644 "$ROOT/app/assets/icon.png" \
    "$STAGE/usr/share/icons/hicolor/256x256/apps/tsubugallery.png"
install -Dm644 "$ROOT/README.md" "$STAGE/usr/share/doc/tsubugallery/README.md"
install -Dm644 "$ROOT/README.ja.md" "$STAGE/usr/share/doc/tsubugallery/README.ja.md"
install -d "$STAGE/usr/share/applications" "$STAGE/DEBIAN" "$BUILD/debian"
cat > "$STAGE/usr/share/applications/tsubugallery.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=TsubuGallery
Comment=Create and browse Processing, p5.js and GLSL sketches
Comment[ja]=Processing・p5.js・GLSL の作品を作成・閲覧
Exec=tsubugallery
Icon=tsubugallery
Terminal=false
Categories=Graphics;
EOF

# dpkg-shlibdeps は debian/control を必要とする。作業ディレクトリ内だけで処理。
cat > "$BUILD/debian/control" <<'EOF'
Source: tsubugallery
Section: graphics
Priority: optional
Maintainer: fukuyori <fukuyori@users.noreply.github.com>

Package: tsubugallery
Architecture: any
Description: Creative coding sketch gallery
EOF
SHLIBS="$(cd "$BUILD" && dpkg-shlibdeps -O -e"$STAGE/usr/bin/tsubugallery")"
DEPENDS="$(printf '%s\n' "$SHLIBS" | sed -n 's/^shlibs:Depends=//p')"
[ -n "$DEPENDS" ] || die '共有ライブラリの依存関係を検出できませんでした。'
# dlopen で読み込むウィンドウ / GPU ライブラリは ELF の依存検出には出ない。
DEPENDS+=', libxkbcommon0, libwayland-client0, libx11-6, libxcursor1, libxi6, libxrandr2, libvulkan1'
SIZE="$(du -sk "$STAGE/usr" | awk '{print $1}')"
cat > "$STAGE/DEBIAN/control" <<EOF
Package: tsubugallery
Version: $VERSION
Architecture: $ARCH
Section: graphics
Priority: optional
Maintainer: fukuyori <fukuyori@users.noreply.github.com>
Homepage: https://github.com/fukuyori/TsubuGallery
Installed-Size: $SIZE
Depends: $DEPENDS
Recommends: fonts-noto-cjk, mesa-vulkan-drivers, xdg-desktop-portal
Description: Creative coding sketch gallery
 Create, edit and browse Processing, p5.js and GLSL sketches.
EOF
chmod 644 "$STAGE/DEBIAN/control" "$STAGE/usr/share/applications/tsubugallery.desktop"
DEB="$OUTPUT_DIR/tsubugallery_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "$STAGE" "$BUILD/package.deb"
mv "$BUILD/package.deb" "$DEB"
printf '\n作成しました: %s\n' "$DEB"

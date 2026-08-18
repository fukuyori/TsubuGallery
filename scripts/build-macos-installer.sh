#!/usr/bin/env bash
#
# TsubuGallery の macOS インストーラ (.pkg) を作る。
#
#   release ビルド → .app へ組む → 電子署名 → .pkg → 公証へ提出 → staple
#
# の順に流す。版番号は Cargo.toml の [workspace.package] から読むので、渡す
# 必要はない。できあがりは target/installer/ へ置く (Windows 版と同じ場所)。
#
#   scripts/build-macos-installer.sh
#
# 公証の資格情報は、一度だけ次で keychain へ入れておく。名前を notarytool に
# しておけば、以後スクリプトへ渡さなくてよい。App 用パスワードは
# https://appleid.apple.com のセキュリティ欄で作る。--password を省くと訊かれる。
#
#   xcrun notarytool store-credentials notarytool \
#       --apple-id you@example.com --team-id XXXXXXXXXX
#
# keychain を使いたくない場合は --apple-id / --team-id / --password を直接渡す
# こともできる。CI ならこちら。
#
# 手順の順序には意味がある。
#
#   * 署名は .app が完成してから。あとから中身を差し替えると署名が壊れる。
#   * 公証は .pkg を提出する。中の .app も一緒に検査され、両方に切符が発行される。
#   * staple は公証が通ってから。切符を .pkg へ貼り付けるので、貼ったあとで
#     .pkg を作り直すと剥がれる。
#
# staple するのは .pkg だけでよい。Installer.app が展開したファイルには
# com.apple.quarantine が付かないので、入れたあとの初回起動で Gatekeeper に
# 止められることはない。

set -euo pipefail

# --- 既定値 -----------------------------------------------------------------

# 逆 DNS。配布元の github.io に合わせる。一度配ったら変えない。変えると
# 別アプリ扱いになり、古い版が残ったまま二重に入る。
readonly APP_ID='io.github.fukuyori.TsubuGallery'
readonly PKG_ID='io.github.fukuyori.TsubuGallery.pkg'
readonly APP_NAME='TsubuGallery'
readonly BIN_NAME='tsubugallery'
readonly PUBLISHER='fukuyori'

# wgpu の Metal 実装が要求する下限より上で、Apple Silicon が出た版に揃える。
# ここを動かしたら Info.plist と cargo のビルドの両方に効く。
readonly MIN_MACOS='11.0'

# 公証の資格情報を入れておく keychain の名前。何も渡されなければこれを使う。
# 名前を毎回打たせないためだけのもので、無ければその場で分かるように、
# ビルドへ入る前に一度問い合わせて確かめる。
readonly DEFAULT_KEYCHAIN_PROFILE='notarytool'

ARCH='universal'
IDENTITY=''
INSTALLER_IDENTITY=''
KEYCHAIN_PROFILE="${AC_KEYCHAIN_PROFILE:-}"
APPLE_ID="${AC_APPLE_ID:-}"
TEAM_ID="${AC_TEAM_ID:-}"
PASSWORD="${AC_PASSWORD:-}"
SKIP_BUILD=0
SKIP_NOTARIZE=0
NO_SIGN=0

usage() {
    # 冒頭のコメントをそのまま説明として出す。二重に書くと片方が古くなる。
    awk 'NR < 3 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"
    cat <<'EOF'

オプション:
  --arch universal|arm64|x86_64   組む対象 (既定: universal)
  --identity NAME                 「Developer ID Application: ...」。省略時は自動で探す
  --installer-identity NAME       「Developer ID Installer: ...」。省略時は自動で探す
  --keychain-profile NAME         notarytool store-credentials で作った名前 (既定: notarytool)
  --apple-id ADDRESS              Apple ID (keychain を使わない場合)
  --team-id ID                    チーム ID (同上)
  --password PASSWORD             App 用パスワード (同上)
  --skip-build                    cargo build を飛ばし、既にある成果物を使う
  --skip-notarize                 署名と .pkg だけ作り、提出しない (手元の確認用)
  --no-sign                       署名もしない (手元の確認用。配ってはいけない)
  -h, --help                      これ
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --arch)                ARCH="$2"; shift 2 ;;
        --identity)            IDENTITY="$2"; shift 2 ;;
        --installer-identity)  INSTALLER_IDENTITY="$2"; shift 2 ;;
        --keychain-profile)    KEYCHAIN_PROFILE="$2"; shift 2 ;;
        --apple-id)            APPLE_ID="$2"; shift 2 ;;
        --team-id)             TEAM_ID="$2"; shift 2 ;;
        --password)            PASSWORD="$2"; shift 2 ;;
        --skip-build)          SKIP_BUILD=1; shift ;;
        --skip-notarize)       SKIP_NOTARIZE=1; shift ;;
        --no-sign)             NO_SIGN=1; SKIP_NOTARIZE=1; shift ;;
        -h|--help)             usage; exit 0 ;;
        *) echo "知らないオプション: $1" >&2; usage >&2; exit 2 ;;
    esac
done

die() { echo "error: $*" >&2; exit 1; }
step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

[ "$(uname -s)" = 'Darwin' ] || die 'macOS でしか動きません。'

# このスクリプトの 1 つ上がリポジトリのルート。どこから呼ばれても動くように。
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/target/macos-installer"
OUTPUT_DIR="$ROOT/target/installer"

case "$ARCH" in
    universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin); HOST_ARCHS='arm64,x86_64' ;;
    arm64)     TARGETS=(aarch64-apple-darwin);                     HOST_ARCHS='arm64' ;;
    x86_64)    TARGETS=(x86_64-apple-darwin);                      HOST_ARCHS='x86_64' ;;
    *) die "--arch は universal / arm64 / x86_64 のいずれか (指定: $ARCH)" ;;
esac

# --- 版番号 -----------------------------------------------------------------
# ワークスペース共通の version を 1 か所から読む。ここを二重管理すると、
# インストーラの版だけ古いまま配る事故が起きる。
VERSION="$(awk '
    /^\[workspace\.package\]/ { in_section = 1; next }
    /^\[/                     { in_section = 0 }
    in_section && /^version[[:space:]]*=/ { gsub(/[",]/, "", $3); print $3; exit }
' "$ROOT/Cargo.toml")"
[ -n "$VERSION" ] || die 'Cargo.toml から version を読めませんでした。'
echo "version = $VERSION  arch = $ARCH"

BASE_NAME="$APP_NAME-$VERSION-macos-$ARCH"
PKG="$OUTPUT_DIR/$BASE_NAME.pkg"

# --- 署名に使う証明書 -------------------------------------------------------
# 名前を毎回打たせない。Developer ID の証明書は、たいてい各種類 1 枚しか
# 入っていないので、入っているものをそのまま使う。2 枚以上あるときだけ訊く。
find_identity() {
    local kind="$1" found
    found="$(security find-identity -v | grep -F "Developer ID $kind:" || true)"
    local count
    count="$(printf '%s' "$found" | grep -c . || true)"
    [ "$count" -eq 1 ] || return 1
    # 「  1) HASH "名前"」から名前だけ取り出す。
    printf '%s' "$found" | sed -n 's/.*"\(.*\)".*/\1/p'
}

if [ "$NO_SIGN" -eq 0 ]; then
    if [ -z "$IDENTITY" ]; then
        IDENTITY="$(find_identity Application)" \
            || die '「Developer ID Application」の証明書を 1 枚に絞れません。--identity で指定してください。'
    fi
    if [ -z "$INSTALLER_IDENTITY" ]; then
        INSTALLER_IDENTITY="$(find_identity Installer)" \
            || die '「Developer ID Installer」の証明書を 1 枚に絞れません。--installer-identity で指定してください。'
    fi
    echo "署名 (app): $IDENTITY"
    echo "署名 (pkg): $INSTALLER_IDENTITY"
    # 「Developer ID Application: 名前 (TEAMID)」の丸かっこがチーム ID。
    # 公証の提出に要るので、渡されていなければここから取る。
    if [ -z "$TEAM_ID" ]; then
        TEAM_ID="$(printf '%s' "$IDENTITY" | sed -n 's/.*(\([A-Z0-9]*\))$/\1/p')"
    fi
else
    echo '警告: --no-sign。署名も公証もしません。手元の確認だけに使ってください。' >&2
fi

# --- 公証の資格情報 ---------------------------------------------------------
# 提出まで走るなら、先に確かめる。1 時間かけてビルドしたあとで「認証情報が
# ありません」と言われるのがいちばん困る。
NOTARY_ARGS=()
if [ "$SKIP_NOTARIZE" -eq 0 ]; then
    if [ -n "$KEYCHAIN_PROFILE" ]; then
        NOTARY_ARGS=(--keychain-profile "$KEYCHAIN_PROFILE")
    elif [ -n "$APPLE_ID" ] && [ -n "$TEAM_ID" ] && [ -n "$PASSWORD" ]; then
        NOTARY_ARGS=(--apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$PASSWORD")
    else
        NOTARY_ARGS=(--keychain-profile "$DEFAULT_KEYCHAIN_PROFILE")
    fi
    # 実際に問い合わせて確かめる。1 秒ほど。ここを省くと、資格情報の間違いに
    # 気づくのがビルドと署名を終えたあとになる。
    if ! xcrun notarytool history "${NOTARY_ARGS[@]}" >/dev/null 2>&1; then
        echo "公証の資格情報を使えません (${NOTARY_ARGS[*]})。" >&2
        echo '一度だけ次を実行して keychain へ入れてください:' >&2
        echo "  xcrun notarytool store-credentials $DEFAULT_KEYCHAIN_PROFILE \\" >&2
        echo '      --apple-id you@example.com --team-id XXXXXXXXXX' >&2
        die '別の名前で入れてあるなら --keychain-profile で渡してください (--skip-notarize で飛ばせます)。'
    fi
fi

# --- ビルド -----------------------------------------------------------------
# Info.plist の LSMinimumSystemVersion と食い違わせない。こちらを下げ忘れると、
# 動かない OS でもインストーラだけは通ってしまう。
export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"

if [ "$SKIP_BUILD" -eq 0 ]; then
    for target in "${TARGETS[@]}"; do
        step "cargo build --release --target $target"
        (cd "$ROOT" && cargo build --release --target "$target")
    done
fi

BINS=()
for target in "${TARGETS[@]}"; do
    bin="$ROOT/target/$target/release/$BIN_NAME"
    [ -f "$bin" ] || die "$bin がありません。--skip-build を外して実行してください。"
    BINS+=("$bin")
done

# --- .app へ組む ------------------------------------------------------------
step 'TsubuGallery.app'

rm -rf "$BUILD"
STAGE="$BUILD/root"
APP="$STAGE/$APP_NAME.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$OUTPUT_DIR"

# 複数アーキテクチャなら 1 つの実行ファイルへまとめる。Rosetta を挟まずに
# Intel でも Apple Silicon でも動くようにするため。
if [ "${#BINS[@]}" -gt 1 ]; then
    lipo -create -output "$APP/Contents/MacOS/$BIN_NAME" "${BINS[@]}"
else
    cp "${BINS[0]}" "$APP/Contents/MacOS/$BIN_NAME"
fi
chmod 755 "$APP/Contents/MacOS/$BIN_NAME"
lipo -info "$APP/Contents/MacOS/$BIN_NAME"

# アイコンは .icns を都度焼く。リポジトリに置いてあるのは元の PNG だけで、
# 変換結果は成果物として扱う (scripts/make-icon.py と同じ考え方)。
#
# 1024px の絵があればそれを使う。無ければ 256px から引き伸ばすが、Dock や
# Finder の大きい表示ではぼやける。make-icon.py を流し直せば作られる。
ICON_SRC="$ROOT/app/assets/icon-1024.png"
[ -f "$ICON_SRC" ] || ICON_SRC="$ROOT/app/assets/icon.png"
ICONSET="$BUILD/$APP_NAME.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil --convert icns "$ICONSET" --output "$APP/Contents/Resources/$APP_NAME.icns"

# CFBundleVersion は「配った回数」を数える番号で、本来は版番号と別物。ここでは
# 同じ値を入れている。同じ版番号で 2 度配ることを想定していないため。
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleInfoDictionaryVersion</key>   <string>6.0</string>
    <key>CFBundlePackageType</key>             <string>APPL</string>
    <key>CFBundleSignature</key>               <string>????</string>
    <key>CFBundleName</key>                    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>             <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>              <string>$APP_ID</string>
    <key>CFBundleExecutable</key>              <string>$BIN_NAME</string>
    <key>CFBundleIconFile</key>                <string>$APP_NAME</string>
    <key>CFBundleShortVersionString</key>      <string>$VERSION</string>
    <key>CFBundleVersion</key>                 <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>          <string>$MIN_MACOS</string>
    <key>LSApplicationCategoryType</key>       <string>public.app-category.graphics-design</string>
    <key>NSHighResolutionCapable</key>         <true/>
    <key>NSHumanReadableCopyright</key>        <string>© $PUBLISHER. MIT OR Apache-2.0</string>
</dict>
</plist>
EOF

# 古い Finder のための 8 バイト。今は Info.plist だけで足りるが、無いと
# ときどき「壊れた bundle」と見なす道具がある。
printf 'APPL????' > "$APP/Contents/PkgInfo"

# --- 署名 -------------------------------------------------------------------
# --options runtime が Hardened Runtime。公証を通すには必須。
#
# entitlements は渡していない。この作品は JIT も dlopen もせず、フォントを
# 読むだけなので、緩める必要がない。緩めるほど公証の審査は通りにくくなる。
#
# --timestamp を落とすと、証明書の期限が切れた時点で、過去に配ったものまで
# 起動しなくなる。
if [ "$NO_SIGN" -eq 0 ]; then
    step '署名 (app)'
    codesign --force --timestamp --options runtime --sign "$IDENTITY" "$APP"
    codesign --verify --strict --verbose=2 "$APP"
fi

# --- .pkg -------------------------------------------------------------------
step '.pkg を組む'

COMPONENT="$BUILD/component.pkg"
COMPONENT_PLIST="$BUILD/component.plist"

# 既定では「同じ bundle id のアプリが他所にあれば、そこへ上書きする」ので、
# 一度 ~/Applications や Downloads へ置かれると、以後ずっとそちらが更新される。
# /Applications へ入れると言った以上、そこへ入れる。
pkgbuild --analyze --root "$STAGE" "$COMPONENT_PLIST" >/dev/null
/usr/libexec/PlistBuddy -c 'Set :0:BundleIsRelocatable false' "$COMPONENT_PLIST"

pkgbuild \
    --root "$STAGE" \
    --component-plist "$COMPONENT_PLIST" \
    --identifier "$PKG_ID" \
    --version "$VERSION" \
    --install-location /Applications \
    "$COMPONENT" >/dev/null

# 配布用の体裁を付ける。ここで対応 OS と対応 CPU を宣言しておくと、合わない
# 機械では「インストールできません」と先に出る。入れてから起動しないより良い。
#
# enable_currentUserHome を立てているので、管理者権限が無くても
# ~/Applications へ入れられる (Windows 版のユーザー単位インストールと同じ)。
DIST="$BUILD/distribution.xml"
cat > "$DIST" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>$APP_NAME</title>
    <options customize="never" require-scripts="false" hostArchitectures="$HOST_ARCHS"/>
    <volume-check>
        <allowed-os-versions>
            <os-version min="$MIN_MACOS"/>
        </allowed-os-versions>
    </volume-check>
    <domains enable_anywhere="false" enable_currentUserHome="true" enable_localSystem="true"/>
    <choices-outline>
        <line choice="default">
            <line choice="$PKG_ID"/>
        </line>
    </choices-outline>
    <choice id="default"/>
    <choice id="$PKG_ID" visible="false">
        <pkg-ref id="$PKG_ID"/>
    </choice>
    <pkg-ref id="$PKG_ID" version="$VERSION" onConclusion="none">component.pkg</pkg-ref>
</installer-gui-script>
EOF

PRODUCT_ARGS=(--distribution "$DIST" --package-path "$BUILD")
if [ "$NO_SIGN" -eq 0 ]; then
    PRODUCT_ARGS+=(--sign "$INSTALLER_IDENTITY" --timestamp)
fi
rm -f "$PKG"
productbuild "${PRODUCT_ARGS[@]}" "$PKG" >/dev/null
[ -f "$PKG" ] || die "$PKG が作られていません。"

# --- 公証 -------------------------------------------------------------------
if [ "$SKIP_NOTARIZE" -eq 1 ]; then
    echo
    echo "公証は飛ばしました。このままでは配れません: $PKG"
    exit 0
fi

step '公証へ提出 (数分かかります)'

LOG="$BUILD/notarytool.txt"
# --wait で結果が出るまで待つ。待たないと、次の staple が「切符がまだ無い」で
# 失敗する。提出そのものが失敗したら、ここで止まる (set -e + pipefail)。
xcrun notarytool submit "$PKG" "${NOTARY_ARGS[@]}" --wait | tee "$LOG"

SUBMISSION_ID="$(sed -n 's/^ *id: \([0-9a-f-]\{36\}\).*/\1/p' "$LOG" | head -1)"
if ! grep -q 'status: Accepted' "$LOG"; then
    echo
    echo '公証が通りませんでした。理由は次のとおり:' >&2
    if [ -n "$SUBMISSION_ID" ]; then
        xcrun notarytool log "$SUBMISSION_ID" "${NOTARY_ARGS[@]}" >&2 || true
    fi
    exit 1
fi

# --- staple -----------------------------------------------------------------
# 切符を .pkg に貼る。貼っておけば、入れる機械がネットに繋がっていなくても
# Gatekeeper が通す。
step 'staple'
xcrun stapler staple "$PKG"
xcrun stapler validate "$PKG"

# 実際に Gatekeeper へ訊いてみる。"source=Notarized Developer ID" と出れば、
# 配ったときに他人の機械で起きることと同じ判定になっている。
echo
spctl --assess --type install --verbose=2 "$PKG" 2>&1 || \
    echo '警告: spctl が通しませんでした。上の出力を確認してください。' >&2

echo
echo "できました: $PKG"
echo "  $(du -h "$PKG" | cut -f1)"

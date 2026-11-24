#!/bin/sh
set -eu
if [ "$(uname -s)" != Darwin ]; then
  echo '需要在 macOS 上打包' >&2
  exit 1
fi
if [ "$#" -ne 2 ]; then
  echo '用法: package-macos.sh 已编译的-malema 输出.app' >&2
  exit 1
fi
binary=$1
app=$2
project_dir=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^version = "\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)"$/\1/p' "$project_dir/Cargo.toml")
bundle_version=$(sed -n 's/^bundle-version = \([1-9][0-9]*\)$/\1/p' "$project_dir/Cargo.toml")
identity=${MALEMA_SIGNING_IDENTITY:--}
if [ -z "$version" ] || [ -z "$bundle_version" ]; then
  echo 'Cargo.toml needs the release version and a positive bundle-version.' >&2
  exit 1
fi
framework="$app/Contents/Frameworks/Sparkle.framework"
if [ -e "$framework" ]; then
  echo 'Use a fresh app path when repackaging a Sparkle build.' >&2
  exit 1
fi
bundle_id=local.malema.desktop
display_name=骂了吗
if [ "${MALEMA_BUNDLE_VARIANT:-}" = leaderboard-beta ]; then
  bundle_id=local.malema.desktop.beta
  display_name='骂了吗 · 榜单内测'
fi
if [ ! -x "$binary" ]; then
  echo '请先运行 cargo build --locked --release' >&2
  exit 1
fi
mkdir -p "$app/Contents/MacOS"
install -m 755 "$binary" "$app/Contents/MacOS/malema"
lexicon="$project_dir/lexicon"
notices="$app/Contents/Resources/lexicon"
mkdir -p "$notices"
install -m 644 "$lexicon/LICENSE-LDNOOBW.txt" "$lexicon/LICENSE-dsojevic.txt" "$lexicon/README.md" "$notices/"
install -m 644 "$(dirname "$0")/../src/review-prompt.txt" "$app/Contents/Resources/review-prompt.txt"
icon_work=$(mktemp -d "${TMPDIR:-/tmp}/malema-icon.XXXXXX")
trap 'rm -rf "$icon_work"' EXIT
icon="$icon_work/AppIcon.png"
clang -std=c11 -Wall -Wextra -framework CoreFoundation -framework CoreGraphics -framework ImageIO \
  "$(dirname "$0")/draw-icon-tile.c" -o "$icon_work/draw-icon-tile"
"$icon_work/draw-icon-tile" "$(dirname "$0")/../assets/app-icon.png" "$icon"
iconset="$icon_work/AppIcon.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
  double=$((size * 2))
  sips -z "$size" "$size" "$icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$double" "$double" "$icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>malema</string>
  <key>CFBundleIdentifier</key><string>$bundle_id</string>
  <key>CFBundleName</key><string>骂了吗</string>
  <key>CFBundleDisplayName</key><string>$display_name</string>
  <key>CFBundleIconFile</key><string>AppIcon.icns</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$bundle_version</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plist="$app/Contents/Info.plist"
sign() {
  if [ "$identity" = "-" ]; then
    codesign --force --sign - "$@"
  else
    codesign --force --options runtime --timestamp --sign "$identity" "$@"
  fi
}

if [ -n "${MALEMA_SPARKLE_DIR:-}" ]; then
  case "${MALEMA_SPARKLE_FEED_URL:-}" in
    https://?*) ;;
    *) echo 'Sparkle requires an HTTPS appcast URL.' >&2; exit 1 ;;
  esac
  if ! printf '%s' "${MALEMA_SPARKLE_PUBLIC_KEY:-}" | grep -Eq '^[A-Za-z0-9+/]{43}=$'; then
    echo 'Sparkle requires a base64 Ed25519 public key.' >&2
    exit 1
  fi
  mkdir -p "$(dirname "$framework")"
  ditto "$MALEMA_SPARKLE_DIR/Sparkle.framework" "$framework"
  install -m 644 "$MALEMA_SPARKLE_DIR/LICENSE" "$app/Contents/Resources/Sparkle-LICENSE"
  plutil -insert SUFeedURL -string "$MALEMA_SPARKLE_FEED_URL" "$plist"
  plutil -insert SUPublicEDKey -string "$MALEMA_SPARKLE_PUBLIC_KEY" "$plist"
  plutil -insert SUEnableAutomaticChecks -bool YES "$plist"
  plutil -insert SUAutomaticallyUpdate -bool NO "$plist"
  plutil -insert SUVerifyUpdateBeforeExtraction -bool YES "$plist"
  plutil -insert SURequireSignedFeed -bool YES "$plist"
  if [ "$identity" != "-" ]; then
    runtime="$framework/Versions/Current"
    for component in "$runtime/Autoupdate" "$runtime/Updater.app" "$runtime"/XPCServices/*.xpc; do
      sign --preserve-metadata=identifier,entitlements "$component"
    done
    sign "$framework"
  fi
fi

plutil -lint "$plist"
sign "$app"
codesign --verify --deep --strict "$app"

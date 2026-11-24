#!/bin/bash
set -euo pipefail

if [[ $# != 3 ]]; then
  echo 'Usage: release-macos.sh BINARY SPARKLE_DIRECTORY OUTPUT_DIRECTORY' >&2
  exit 2
fi
binary=$1
export MALEMA_SPARKLE_DIR=$2
dist=$3
: "${GITHUB_REPOSITORY:?Missing GitHub repository}"
: "${GITHUB_REF_NAME:?Missing GitHub release tag}"
: "${APPLE_ID:?Missing APPLE_ID}"
: "${APPLE_APP_SPECIFIC_PASSWORD:?Missing APPLE_APP_SPECIFIC_PASSWORD}"
: "${APPLE_TEAM_ID:?Missing APPLE_TEAM_ID}"
: "${MALEMA_SPARKLE_PUBLIC_KEY:?Missing SPARKLE_PUBLIC_KEY}"
: "${SPARKLE_PRIVATE_KEY:?Missing SPARKLE_PRIVATE_KEY}"

export MALEMA_SPARKLE_FEED_URL="https://github.com/$GITHUB_REPOSITORY/releases/latest/download/appcast.xml"
MALEMA_SIGNING_IDENTITY=$(security find-identity -v -p codesigning |
  awk -v team="($APPLE_TEAM_ID)" '/"Developer ID Application:/ && index($0, team) && !found { found = 1; print $2 }')
: "${MALEMA_SIGNING_IDENTITY:?No valid Developer ID Application identity for APPLE_TEAM_ID}"
export MALEMA_SIGNING_IDENTITY

mkdir -p "$dist"
app="$dist/骂了吗.app"
archive="$dist/malema-macos-arm64.zip"
if [[ -e "$app" || -e "$archive" || -e "$dist/appcast.xml" ]]; then
  echo 'Release output directory already contains an app, archive, or appcast.' >&2
  exit 1
fi
scratch=$(mktemp -d "${TMPDIR:-/tmp}/malema-notarize.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

"$(dirname "$0")/package-macos.sh" "$binary" "$app"
ditto -c -k --sequesterRsrc --keepParent "$app" "$scratch/notarize.zip"

if ! xcrun notarytool submit "$scratch/notarize.zip" --apple-id "$APPLE_ID" \
  --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_SPECIFIC_PASSWORD" \
  --wait --output-format json > "$scratch/submission.json"; then
  cat "$scratch/submission.json" >&2
  exit 1
fi
python3 -c 'import json,sys; status=json.load(open(sys.argv[1])); print("Notary status:",status.get("status"),"ID:",status.get("id")); sys.exit(status.get("status") != "Accepted")' \
  "$scratch/submission.json"
xcrun stapler staple "$app"
xcrun stapler validate "$app"
codesign --verify --deep --strict "$app"
spctl --assess --type execute "$app"
ditto -c -k --sequesterRsrc --keepParent "$app" "$archive"

download="https://github.com/$GITHUB_REPOSITORY/releases/download/$GITHUB_REF_NAME/"
printf '%s\n' "$SPARKLE_PRIVATE_KEY" | "$MALEMA_SPARKLE_DIR/bin/generate_appcast" \
  --ed-key-file - --maximum-deltas 0 --download-url-prefix "$download" "$dist"
printf '%s\n' "$SPARKLE_PRIVATE_KEY" | "$MALEMA_SPARKLE_DIR/bin/sign_update" \
  --verify --ed-key-file - "$dist/appcast.xml"
python3 - "$dist/appcast.xml" "$app/Contents/Info.plist" "$archive" "$download" <<'PY'
import base64
import plistlib
import sys
from pathlib import Path
from xml.etree import ElementTree

feed, info, archive = map(Path, sys.argv[1:4])
with info.open("rb") as file:
    version = plistlib.load(file)["CFBundleVersion"]
ns = "{http://www.andymatuschak.org/xml-namespaces/sparkle}"
item = ElementTree.parse(feed).find("channel/item")
if item is None or item.findtext(ns + "version") != version:
    sys.exit("The appcast is missing this build.")
asset = item.find("enclosure")
if asset is None or asset.get("url") != sys.argv[4] + archive.name:
    sys.exit("The appcast points to the wrong archive.")
if int(asset.get("length", "-1")) != archive.stat().st_size:
    sys.exit("The appcast archive size differs from the verified archive.")
if len(base64.b64decode(asset.get(ns + "edSignature", ""), validate=True)) != 64:
    sys.exit("The appcast is missing its update signature.")
PY

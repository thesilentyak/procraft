#!/usr/bin/env bash
# Signs the unsigned .ipa ios/build-ipa.sh made, on a Mac (CI: a macOS runner, on the Release
# workflow's manual runs), into
#
#   procraft-<version>-ipados.ipa   signed with the team's certificate and provisioning profile:
#                                   installs as it is on the devices the profile lists (testers'),
#                                   so it is never released
#
# when the signing secrets are given; without them (forks, dry runs) it says so and signs nothing:
#
#   IOS_CERTIFICATE_P12        base64 of a .p12 holding the team's signing certificate and its key
#   IOS_CERTIFICATE_PASSWORD   that .p12's password
#   IOS_PROVISIONING_PROFILE   base64 of a .mobileprovision whose App ID covers the app's bundle id
#
# and, for an app built with its Recent artwork widget (PROCRAFT_WIDGET=YES, see ios/widget.yml):
#
#   IOS_WIDGET_PROVISIONING_PROFILE   base64 of the widget's own .mobileprovision (its App ID
#                                     io.github.artcraftandroid.procraft.RecentWidget); without it
#                                     the widget is left out and the app signed alone
#
# Usage: ios/sign-ipa.sh   (VERSION and DIST as ios/build-ipa.sh has them; the signed .ipa goes
# to SIGNED_DIST, DIST if unset)
#
# The entitlements are the profile's own, its App ID made the app's. The key lives in a keychain
# of its own, made here and deleted on the way out, with the files the secrets were written to.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROPERTIES="$ROOT/android/gradle.properties"
VERSION="${VERSION:-$(sed -n 's/^procraft\.version=//p' "$PROPERTIES" | tr -d '\r')}"
DIST="$(mkdir -p "${DIST:-$ROOT/dist}" && cd "${DIST:-$ROOT/dist}" && pwd)"
UNSIGNED="$DIST/procraft-$VERSION-ipados-unsigned.ipa"
SIGNED_DIST="$(mkdir -p "${SIGNED_DIST:-$DIST}" && cd "${SIGNED_DIST:-$DIST}" && pwd)"
SIGNED="$SIGNED_DIST/procraft-$VERSION-ipados.ipa"

if [ -z "${IOS_CERTIFICATE_P12:-}" ] || [ -z "${IOS_CERTIFICATE_PASSWORD:-}" ] || [ -z "${IOS_PROVISIONING_PROFILE:-}" ]; then
  echo "no signing secrets: the unsigned .ipa alone"
  exit 0
fi
[ -f "$UNSIGNED" ] || { echo "error: no $UNSIGNED (run ios/build-ipa.sh first)" >&2; exit 1; }

WORK="$(mktemp -d)"
KEYCHAIN="$WORK/signing.keychain-db"
KEYCHAIN_PASSWORD="$(uuidgen)"
cleanup() {
  security delete-keychain "$KEYCHAIN" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# the certificate and its key, in a keychain codesign may use without asking
printf '%s' "$IOS_CERTIFICATE_P12" | base64 --decode >"$WORK/signing.p12"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 3600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$WORK/signing.p12" -k "$KEYCHAIN" -P "$IOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
# (searched with the others, first)
security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | tr -d '"')
IDENTITY="$(security find-identity -v -p codesigning "$KEYCHAIN" | awk 'NR == 1 && $2 ~ /^[0-9A-F]{40}$/ { print $2 }')"
[ -n "$IDENTITY" ] || { echo "error: the certificate has no identity to sign with" >&2; exit 1; }

# the profile, and the entitlements it grants
printf '%s' "$IOS_PROVISIONING_PROFILE" | base64 --decode >"$WORK/profile.mobileprovision"
security cms -D -i "$WORK/profile.mobileprovision" >"$WORK/profile.plist"
plist() { /usr/libexec/PlistBuddy -c "$1" "$2"; }
TEAM="$(plist 'Print :TeamIdentifier:0' "$WORK/profile.plist")"
APP_ID="$(plist 'Print :Entitlements:application-identifier' "$WORK/profile.plist")"
/usr/libexec/PlistBuddy -x -c 'Print :Entitlements' "$WORK/profile.plist" >"$WORK/entitlements.plist"

# the app, out of its Payload folder
mkdir -p "$WORK/ipa"
(cd "$WORK/ipa" && unzip -q "$UNSIGNED")
APP="$(find "$WORK/ipa/Payload" -maxdepth 1 -name '*.app' | head -1)"
[ -d "$APP" ] || { echo "error: the .ipa has no app" >&2; exit 1; }
BUNDLE="$(plist 'Print :CFBundleIdentifier' "$APP/Info.plist")"
# the profile's App ID must cover the bundle id: the same, or a wildcard ending in .*
case "$APP_ID" in
  "$TEAM.$BUNDLE") ;;
  *'.*') [[ "$TEAM.$BUNDLE" == "${APP_ID%\*}"* ]] || { echo "error: the profile ($APP_ID) doesn't cover $BUNDLE" >&2; exit 1; } ;;
  *) echo "error: the profile ($APP_ID) doesn't cover $BUNDLE" >&2; exit 1 ;;
esac
# a wildcard App ID is made the app's own; a keychain group the app doesn't use is left out
plist "Set :application-identifier $TEAM.$BUNDLE" "$WORK/entitlements.plist"
plist 'Delete :keychain-access-groups' "$WORK/entitlements.plist" 2>/dev/null || true

cp "$WORK/profile.mobileprovision" "$APP/embedded.mobileprovision"
# the Recent artwork widget, an extension with a profile of its own: signed with it, or left out
WIDGET="$APP/PlugIns/RecentWidget.appex"
if [ -d "$WIDGET" ]; then
  if [ -n "${IOS_WIDGET_PROVISIONING_PROFILE:-}" ]; then
    printf '%s' "$IOS_WIDGET_PROVISIONING_PROFILE" | base64 --decode >"$WORK/widget.mobileprovision"
    security cms -D -i "$WORK/widget.mobileprovision" >"$WORK/widget.plist"
    WIDGET_APP_ID="$(plist 'Print :Entitlements:application-identifier' "$WORK/widget.plist")"
    WIDGET_BUNDLE="$(plist 'Print :CFBundleIdentifier' "$WIDGET/Info.plist")"
    case "$WIDGET_APP_ID" in
      "$TEAM.$WIDGET_BUNDLE") ;;
      *'.*') [[ "$TEAM.$WIDGET_BUNDLE" == "${WIDGET_APP_ID%\*}"* ]] || { echo "error: the widget's profile ($WIDGET_APP_ID) doesn't cover $WIDGET_BUNDLE" >&2; exit 1; } ;;
      *) echo "error: the widget's profile ($WIDGET_APP_ID) doesn't cover $WIDGET_BUNDLE" >&2; exit 1 ;;
    esac
    /usr/libexec/PlistBuddy -x -c 'Print :Entitlements' "$WORK/widget.plist" >"$WORK/widget-entitlements.plist"
    plist "Set :application-identifier $TEAM.$WIDGET_BUNDLE" "$WORK/widget-entitlements.plist"
    plist 'Delete :keychain-access-groups' "$WORK/widget-entitlements.plist" 2>/dev/null || true
    cp "$WORK/widget.mobileprovision" "$WIDGET/embedded.mobileprovision"
    codesign --force --sign "$IDENTITY" --keychain "$KEYCHAIN" --timestamp=none \
      --entitlements "$WORK/widget-entitlements.plist" "$WIDGET"
  else
    echo "no IOS_WIDGET_PROVISIONING_PROFILE: the app signed without its widget"
    rm -rf "$WIDGET"
    rmdir "$APP/PlugIns" 2>/dev/null || true
  fi
fi
# what is inside first (the widget is signed above), then the app with its entitlements
find "$APP" -depth \( -name '*.framework' -o -name '*.dylib' -o \( -name '*.appex' ! -name RecentWidget.appex \) \) -print0 |
  while IFS= read -r -d '' nested; do
    codesign --force --sign "$IDENTITY" --keychain "$KEYCHAIN" --timestamp=none "$nested"
  done
codesign --force --sign "$IDENTITY" --keychain "$KEYCHAIN" --timestamp=none \
  --entitlements "$WORK/entitlements.plist" "$APP"
codesign --verify --deep --strict "$APP"

rm -f "$SIGNED"
(cd "$WORK/ipa" && zip -qry "$SIGNED" Payload)
DEVICES="$(plist 'Print :ProvisionedDevices' "$WORK/profile.plist" 2>/dev/null | grep -c '^ ' || true)"
EXPIRES="$(plist 'Print :ExpirationDate' "$WORK/profile.plist")"
echo "wrote $SIGNED (team $TEAM, $DEVICES device(s) in the profile, which expires $EXPIRES)"

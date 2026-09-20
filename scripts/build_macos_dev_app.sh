#!/bin/zsh
set -euo pipefail

repo_root=${0:A:h:h}
cef_dist=${SWITCHBOARD_CEF_DIST:-}
cef_stage=""
build_smoke=${SWITCHBOARD_BUILD_SMOKE:-0}

if [[ "$build_smoke" == "1" || "$build_smoke" == "true" ]]; then
  app_name="Switchboard Smoke"
  bundle_id="dev.switchboard.browser.smoke"
else
  app_name="Switchboard Dev"
  bundle_id="dev.switchboard.browser"
fi

cleanup_cef_stage() {
  if [[ -n "$cef_stage" && -d "$cef_stage" ]]; then
    rm -rf "$cef_stage"
  fi
}
trap cleanup_cef_stage EXIT

if [[ -z "$cef_dist" ]]; then
  bundled_framework="$repo_root/target/dev-bundle/Switchboard Dev.app/Contents/Frameworks/Chromium Embedded Framework.framework"
  if [[ ! -x "$bundled_framework/Chromium Embedded Framework" ]]; then
    print -u2 "SWITCHBOARD_CEF_DIST must point at the pinned CEF 145 distribution (no reusable dev-bundle framework was found)"
    exit 2
  fi
  cef_stage=$(mktemp -d /private/tmp/switchboard-cef.XXXXXX)
  staged_framework="$cef_stage/Release/Chromium Embedded Framework.framework"
  mkdir -p "$staged_framework"
  COPYFILE_DISABLE=1 /usr/bin/tar --exclude='./.DS_Store' \
    -C "$bundled_framework" -cf "$cef_stage/framework.tar" .
  COPYFILE_DISABLE=1 /usr/bin/tar -C "$staged_framework" \
    -xf "$cef_stage/framework.tar"
  cef_dist="$cef_stage"
  print -r -- "Reusing the CEF framework from the existing dev bundle"
fi

cef_framework="$cef_dist/Release/Chromium Embedded Framework.framework"
if [[ ! -x "$cef_framework/Chromium Embedded Framework" ]]; then
  print -u2 "CEF framework is incomplete: $cef_framework"
  exit 2
fi

cargo build --manifest-path "$repo_root/Cargo.toml" -p switchboard-app

bundle_root="$repo_root/target/dev-bundle"
app="$bundle_root/$app_name.app"
contents="$app/Contents"
frameworks="$contents/Frameworks"
macos="$contents/MacOS"
resources="$contents/Resources"

rm -rf "$app"
mkdir -p "$frameworks" "$macos" "$resources"
cp "$repo_root/packaging/macos/Info.plist" "$contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $app_name" "$contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName $app_name" "$contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $bundle_id" "$contents/Info.plist"
cp "$repo_root/target/debug/switchboard-app" "$macos/switchboard-app"
ditto "$cef_framework" "$frameworks/Chromium Embedded Framework.framework"

helper_names=(
  "Switchboard Helper"
  "Switchboard Helper (Renderer)"
  "Switchboard Helper (GPU)"
  "Switchboard Helper (Plugin)"
)

for helper_name in $helper_names; do
  helper_app="$frameworks/$helper_name.app"
  helper_contents="$helper_app/Contents"
  mkdir -p "$helper_contents/MacOS"
  cp "$repo_root/packaging/macos/Helper-Info.plist" "$helper_contents/Info.plist"
  cp "$repo_root/target/debug/switchboard-app" "$helper_contents/MacOS/$helper_name"
  /usr/libexec/PlistBuddy -c "Set :CFBundleExecutable $helper_name" "$helper_contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $helper_name" "$helper_contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleName $helper_name" "$helper_contents/Info.plist"
  helper_id=$(print -r -- "$helper_name" | tr '[:upper:] ()' '[:lower:].--')
  /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $bundle_id.$helper_id" "$helper_contents/Info.plist"
done

codesign --force --sign - "$frameworks/Chromium Embedded Framework.framework"
for helper_name in $helper_names; do
  codesign --force --sign - --entitlements "$repo_root/packaging/macos/helper.entitlements" "$frameworks/$helper_name.app"
done
codesign --force --sign - --entitlements "$repo_root/packaging/macos/app.entitlements" "$app"
codesign --verify --deep --strict "$app"

print -r -- "$app"

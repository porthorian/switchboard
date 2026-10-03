#!/bin/zsh
set -euo pipefail

repo_root=${0:A:h:h}
qa_app="$repo_root/target/dev-bundle/Switchboard Extension QA.app"
qa_executable="$qa_app/Contents/MacOS/switchboard-app"
session_root=${1:-}

if [[ ! -x "$qa_executable" ]]; then
  print -u2 "Password-extension QA bundle not found: $qa_app"
  print -u2 "Build it with SWITCHBOARD_BUILD_EXTENSION_QA=1 ./scripts/build_macos_dev_app.sh"
  exit 2
fi

if [[ -z "$session_root" ]]; then
  session_root=$(mktemp -d /private/tmp/switchboard-extension-qa.XXXXXX)
elif [[ "$session_root" != /* || ! -d "$session_root" ]]; then
  print -u2 "Session root must be an existing absolute directory"
  exit 2
fi

mkdir -p "$session_root/cef" "$session_root/tmp"
print -r -- "Switchboard password-extension QA session"
print -r -- "  session_root : $session_root"
print -r -- "  state_db     : $session_root/state.sqlite3"
print -r -- "  cef_cache    : $session_root/cef"
print -r -- "  fixture_url  : http://127.0.0.1:18765/password-extension.html"
print -r -- "  keychain     : real macOS Keychain"
print -r -- "Restart with the same session root to verify persistence:"
print -r -- "  $0 $session_root"

unset SWITCHBOARD_CEF_USE_MOCK_KEYCHAIN
unset SWITCHBOARD_CEF_PASSWORD_STORE
unset SWITCHBOARD_CEF_DIST
unset SWITCHBOARD_CEF_LIBRARY
unset SWITCHBOARD_CEF_FRAMEWORK_DIR
unset SWITCHBOARD_CEF_RESOURCES_DIR
unset SWITCHBOARD_CEF_BROWSER_SUBPROCESS
unset SWITCHBOARD_CEF_MAIN_BUNDLE_PATH

SWITCHBOARD_STATE_DB_PATH="$session_root/state.sqlite3" \
SWITCHBOARD_CEF_ROOT_CACHE_PATH="$session_root/cef" \
SWITCHBOARD_CEF_TMPDIR="$session_root/tmp" \
SWITCHBOARD_CEF_API_VERSION=14500 \
SWITCHBOARD_CEF_VERBOSE_ERRORS=1 \
"$qa_executable"

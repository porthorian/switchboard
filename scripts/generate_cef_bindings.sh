#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   ./scripts/generate_cef_bindings.sh <cef_capi_header> [include_dir]
#
# Example:
#   ./scripts/generate_cef_bindings.sh \
#     third_party/cef/Chromium\ Embedded\ Framework.framework/Headers/capi/cef_app_capi.h \
#     third_party/cef/Chromium\ Embedded\ Framework.framework/Headers

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <cef_header> [include_dir]" >&2
  exit 1
fi

if ! command -v bindgen >/dev/null 2>&1; then
  echo "bindgen CLI not found. Install with: cargo install bindgen-cli" >&2
  exit 1
fi

HEADER="$1"
INCLUDE_DIR="${2:-}"

export SWITCHBOARD_CEF_GENERATE_BINDINGS=1
export SWITCHBOARD_CEF_HEADER="$HEADER"
if [[ -n "$INCLUDE_DIR" ]]; then
  export SWITCHBOARD_CEF_INCLUDE_DIR="$INCLUDE_DIR"
fi

cargo check -p switchboard-cef-sys

echo "Generated CEF bindings into build output for switchboard-cef-sys."
echo "Use the same env vars when building app targets that depend on generated symbols."

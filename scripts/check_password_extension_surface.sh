#!/bin/zsh
set -euo pipefail

repo_root=${0:A:h:h}
cef_dist=${1:-$repo_root/target/cef-sdk-145}
pinned_version="145.0.26+g6ed7554+chromium-145.0.7632.110"
cef_readme="$cef_dist/README.txt"
request_context_header="$cef_dist/include/capi/cef_request_context_capi.h"
host_source="$repo_root/crates/switchboard-app/src/host.rs"
ui_source="$repo_root/crates/switchboard-app/src/ui_shell.html"

for required_path in "$cef_readme" "$request_context_header" "$host_source" "$ui_source"; do
  if [[ ! -f "$required_path" ]]; then
    print -u2 "password-extension preflight: missing required file: $required_path"
    exit 2
  fi
done

if ! /usr/bin/grep -Fq "CEF Version:      $pinned_version" "$cef_readme"; then
  print -u2 "password-extension preflight: CEF distribution is not pinned version $pinned_version"
  exit 2
fi

extension_api="absent"
if /usr/bin/grep -Eq 'load_extension|has_extension|get_extensions|did_load_extension' "$request_context_header"; then
  extension_api="present"
fi

if ! /usr/bin/grep -Fq 'window_info.runtime_style = CEF_RUNTIME_STYLE_ALLOY;' "$host_source"; then
  print -u2 "password-extension preflight: Switchboard content runtime is no longer explicitly Alloy; review the gate before continuing"
  exit 2
fi

if ! /usr/bin/grep -Fq 'Unavailable in this alpha' "$ui_source"; then
  print -u2 "password-extension preflight: fail-closed password-extension UI notice is missing"
  exit 2
fi

print -r -- "password-extension preflight"
print -r -- "  cef_version                         : $pinned_version"
print -r -- "  content_runtime_style               : alloy"
print -r -- "  alloy_request_context_extension_api : $extension_api"
print -r -- "  product_controls                    : disabled"
print -r -- "  next_gate                           : user-initiated Web Store install in the isolated QA bundle"

if [[ "$extension_api" == "present" ]]; then
  print -r -- "  note                                : the CEF surface changed; rerun the full security review"
else
  print -r -- "  note                                : no custom-chrome extension loader is exposed by the pinned CEF request-context API"
fi

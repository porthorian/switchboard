// Compile with clang -std=c11 -fsyntax-only -DCEF_API_VERSION=14500 -Itarget/cef-sdk-145 scripts/check_cef_favicon_abi.c
#include <stddef.h>
#include "include/capi/cef_browser_capi.h"
#include "include/capi/cef_image_capi.h"
#include "include/capi/cef_request_capi.h"
_Static_assert(sizeof(cef_download_image_callback_t) == 48, "download callback ABI");
_Static_assert(sizeof(cef_image_t) == 144, "image ABI");
_Static_assert(offsetof(cef_image_t, get_as_png) == 128, "PNG export ABI");
_Static_assert(sizeof(cef_binary_value_t) == 104, "binary value ABI");
_Static_assert(offsetof(cef_binary_value_t, get_size) == 88, "binary size ABI");
_Static_assert(offsetof(cef_request_t, get_url) == 48, "request URL ABI");
_Static_assert(offsetof(cef_request_t, get_method) == 64, "request method ABI");
_Static_assert(offsetof(cef_browser_host_t, download_image) == 184, "browser image download ABI");

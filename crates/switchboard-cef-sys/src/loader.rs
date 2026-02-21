use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::mem::transmute_copy;
use std::path::Path;
use std::ptr::NonNull;

use crate::raw::{
    cef_api_hash_fn, cef_api_version_fn, cef_browser_host_create_browser_fn, cef_currently_on_fn,
    cef_do_message_loop_work_fn, cef_execute_process_fn, cef_initialize_fn, cef_post_task_fn,
    cef_quit_message_loop_fn, cef_run_message_loop_fn, cef_shutdown_fn, cef_string_utf16_clear_fn,
    cef_string_utf16_set_fn,
};

const RTLD_LAZY: i32 = 0x1;
const RTLD_LOCAL: i32 = 0x4;

#[derive(Debug)]
pub enum CefLoadError {
    NotFound(String),
    SymbolMissing(&'static str, String),
    DlError(String),
    UnsupportedPlatform,
}

impl Display for CefLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::NotFound(path) => write!(f, "CEF library not found at path: {path}"),
            Self::SymbolMissing(symbol, detail) => {
                write!(f, "missing CEF symbol {symbol}: {detail}")
            }
            Self::DlError(detail) => write!(f, "dynamic loader error: {detail}"),
            Self::UnsupportedPlatform => {
                write!(f, "dynamic CEF loading not supported on this platform")
            }
        }
    }
}

impl std::error::Error for CefLoadError {}

#[derive(Clone, Copy)]
pub struct CefApi {
    pub cef_api_hash: cef_api_hash_fn,
    pub cef_api_version: cef_api_version_fn,
    pub cef_execute_process: cef_execute_process_fn,
    pub cef_initialize: cef_initialize_fn,
    pub cef_shutdown: cef_shutdown_fn,
    pub cef_run_message_loop: cef_run_message_loop_fn,
    pub cef_do_message_loop_work: cef_do_message_loop_work_fn,
    pub cef_quit_message_loop: cef_quit_message_loop_fn,
    pub cef_currently_on: cef_currently_on_fn,
    pub cef_post_task: cef_post_task_fn,
    pub cef_browser_host_create_browser: cef_browser_host_create_browser_fn,
    pub cef_string_utf16_set: cef_string_utf16_set_fn,
    pub cef_string_utf16_clear: cef_string_utf16_clear_fn,
}

pub struct CefLibrary {
    handle: NonNull<c_void>,
    pub api: CefApi,
}

impl CefLibrary {
    pub unsafe fn open<P: AsRef<Path>>(path: P) -> Result<Self, CefLoadError> {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let path_string = path.as_ref().to_string_lossy().to_string();
            if !path.as_ref().exists() {
                return Err(CefLoadError::NotFound(path_string));
            }

            let path_cstr = CString::new(path_string.clone()).map_err(|_| {
                CefLoadError::DlError("library path contained interior NUL".to_owned())
            })?;

            let raw = dlopen(path_cstr.as_ptr(), RTLD_LAZY | RTLD_LOCAL);
            let handle = NonNull::new(raw).ok_or_else(|| CefLoadError::DlError(last_dl_error()))?;

            let load = |symbol: &'static str| -> Result<*mut c_void, CefLoadError> {
                let symbol_cstr = CString::new(symbol).map_err(|_| {
                    CefLoadError::DlError("symbol name contained interior NUL".to_owned())
                })?;
                let ptr = dlsym(handle.as_ptr(), symbol_cstr.as_ptr());
                if ptr.is_null() {
                    return Err(CefLoadError::SymbolMissing(symbol, last_dl_error()));
                }
                Ok(ptr)
            };

            let api = CefApi {
                cef_api_hash: load_symbol::<cef_api_hash_fn>(&load, "cef_api_hash")?,
                cef_api_version: load_symbol::<cef_api_version_fn>(&load, "cef_api_version")?,
                cef_execute_process: load_symbol::<cef_execute_process_fn>(
                    &load,
                    "cef_execute_process",
                )?,
                cef_initialize: load_symbol::<cef_initialize_fn>(&load, "cef_initialize")?,
                cef_shutdown: load_symbol::<cef_shutdown_fn>(&load, "cef_shutdown")?,
                cef_run_message_loop: load_symbol::<cef_run_message_loop_fn>(
                    &load,
                    "cef_run_message_loop",
                )?,
                cef_do_message_loop_work: load_symbol::<cef_do_message_loop_work_fn>(
                    &load,
                    "cef_do_message_loop_work",
                )?,
                cef_quit_message_loop: load_symbol::<cef_quit_message_loop_fn>(
                    &load,
                    "cef_quit_message_loop",
                )?,
                cef_currently_on: load_symbol::<cef_currently_on_fn>(&load, "cef_currently_on")?,
                cef_post_task: load_symbol::<cef_post_task_fn>(&load, "cef_post_task")?,
                cef_browser_host_create_browser: load_symbol::<cef_browser_host_create_browser_fn>(
                    &load,
                    "cef_browser_host_create_browser",
                )?,
                cef_string_utf16_set: load_symbol::<cef_string_utf16_set_fn>(
                    &load,
                    "cef_string_utf16_set",
                )?,
                cef_string_utf16_clear: load_symbol::<cef_string_utf16_clear_fn>(
                    &load,
                    "cef_string_utf16_clear",
                )?,
            };

            Ok(Self { handle, api })
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = path;
            Err(CefLoadError::UnsupportedPlatform)
        }
    }

    #[cfg(target_os = "macos")]
    pub unsafe fn open_default_macos() -> Result<Self, CefLoadError> {
        Self::open("Chromium Embedded Framework.framework/Chromium Embedded Framework")
    }

    #[cfg(not(target_os = "macos"))]
    pub unsafe fn open_default_macos() -> Result<Self, CefLoadError> {
        Err(CefLoadError::UnsupportedPlatform)
    }
}

impl Drop for CefLibrary {
    fn drop(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        unsafe {
            let _ = dlclose(self.handle.as_ptr());
        }
    }
}

unsafe fn load_symbol<F>(
    load: &impl Fn(&'static str) -> Result<*mut c_void, CefLoadError>,
    symbol: &'static str,
) -> Result<F, CefLoadError>
where
    F: Copy,
{
    let ptr = load(symbol)?;
    Ok(transmute_copy::<*mut c_void, F>(&ptr))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
unsafe fn last_dl_error() -> String {
    let err = dlerror();
    if err.is_null() {
        return "unknown error".to_owned();
    }
    CStr::from_ptr(err).to_string_lossy().to_string()
}

#[cfg(target_os = "linux")]
#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
    fn dlerror() -> *const c_char;
}

#[cfg(target_os = "macos")]
#[link(name = "System")]
extern "C" {
    fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
    fn dlerror() -> *const c_char;
}

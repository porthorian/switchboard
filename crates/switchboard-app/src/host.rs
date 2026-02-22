use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use switchboard_core::TabId;

use crate::bridge::UiCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiViewId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentViewId(pub u64);

#[cfg(any(test, not(target_os = "macos")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    WindowCreated {
        window_id: WindowId,
        title: String,
    },
    UiViewCreated {
        window_id: WindowId,
        view_id: UiViewId,
        url: String,
    },
    ContentViewCreated {
        window_id: WindowId,
        view_id: ContentViewId,
        tab_id: TabId,
        url: String,
    },
    ContentNavigated {
        view_id: ContentViewId,
        tab_id: TabId,
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    InvalidUiUrl(String),
    InvalidContentUrl(String),
    Native(String),
    #[cfg(not(target_os = "macos"))]
    UnsupportedPlatform,
}

impl Display for HostError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::InvalidUiUrl(url) => write!(f, "ui view must load app://ui, got {url}"),
            Self::InvalidContentUrl(url) => {
                write!(f, "content view cannot load privileged app url: {url}")
            }
            Self::Native(message) => write!(f, "{message}"),
            #[cfg(not(target_os = "macos"))]
            Self::UnsupportedPlatform => write!(f, "native window host is only available on macOS"),
        }
    }
}

impl Error for HostError {}

pub trait CefHost {
    type Error;

    fn create_window(&mut self, title: &str) -> Result<WindowId, Self::Error>;

    fn create_ui_view(&mut self, window_id: WindowId, url: &str) -> Result<UiViewId, Self::Error>;

    fn create_content_view(
        &mut self,
        window_id: WindowId,
        tab_id: TabId,
        url: &str,
    ) -> Result<ContentViewId, Self::Error>;

    fn navigate_content_view(
        &mut self,
        view_id: ContentViewId,
        tab_id: TabId,
        url: &str,
    ) -> Result<(), Self::Error>;

    fn set_content_view_visible(
        &mut self,
        view_id: ContentViewId,
        visible: bool,
    ) -> Result<(), Self::Error>;

    fn clear_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error>;

    fn run_event_loop(&mut self) -> Result<(), Self::Error>;
}

pub type UiCommandHandler = Box<dyn FnMut(UiCommand) + 'static>;
pub type UiStateProvider = Box<dyn FnMut() -> String + 'static>;
pub type ContentEventHandler = Box<dyn FnMut(ContentEvent) + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentEvent {
    UrlChanged { tab_id: TabId, url: String },
    TitleChanged { tab_id: TabId, title: String },
    LoadingChanged { tab_id: TabId, is_loading: bool },
}

thread_local! {
    static UI_COMMAND_HANDLER: RefCell<Option<UiCommandHandler>> = RefCell::new(None);
    static UI_STATE_PROVIDER: RefCell<Option<UiStateProvider>> = RefCell::new(None);
    static CONTENT_EVENT_HANDLER: RefCell<Option<ContentEventHandler>> = RefCell::new(None);
    static ACTIVE_CONTENT_URI: RefCell<Option<String>> = const { RefCell::new(None) };
    static ACTIVE_CONTENT_TAB: RefCell<Option<TabId>> = const { RefCell::new(None) };
    #[cfg(target_os = "macos")]
    static ACTIVE_CONTENT_BROWSER: RefCell<*mut cef_browser_t> = const { RefCell::new(std::ptr::null_mut()) };
}

pub fn install_ui_command_handler(handler: Option<UiCommandHandler>) {
    UI_COMMAND_HANDLER.with(|slot| {
        *slot.borrow_mut() = handler;
    });
}

pub fn install_ui_state_provider(provider: Option<UiStateProvider>) {
    UI_STATE_PROVIDER.with(|slot| {
        *slot.borrow_mut() = provider;
    });
}

pub fn install_content_event_handler(handler: Option<ContentEventHandler>) {
    CONTENT_EVENT_HANDLER.with(|slot| {
        *slot.borrow_mut() = handler;
    });
}

fn emit_ui_command(command: UiCommand) {
    UI_COMMAND_HANDLER.with(|slot| {
        if let Some(handler) = slot.borrow_mut().as_mut() {
            handler(command);
        }
    });
}

fn query_ui_shell_state() -> String {
    UI_STATE_PROVIDER.with(|slot| {
        let mut slot_ref = slot.borrow_mut();
        if let Some(provider) = slot_ref.as_mut() {
            return provider();
        }
        "{\"revision\":0,\"active_profile_id\":null,\"profiles\":[],\"workspaces\":[],\"tabs\":[]}"
            .to_owned()
    })
}

fn emit_content_event(event: ContentEvent) {
    CONTENT_EVENT_HANDLER.with(|slot| {
        if let Some(handler) = slot.borrow_mut().as_mut() {
            handler(event);
        }
    });
}

#[cfg(target_os = "macos")]
fn set_active_content_uri(url: String) {
    ACTIVE_CONTENT_URI.with(|slot| {
        *slot.borrow_mut() = Some(url);
    });
}

#[cfg(target_os = "macos")]
fn set_active_content_tab(tab_id: Option<TabId>) {
    ACTIVE_CONTENT_TAB.with(|slot| {
        *slot.borrow_mut() = tab_id;
    });
}

#[cfg(target_os = "macos")]
fn active_content_tab() -> Option<TabId> {
    ACTIVE_CONTENT_TAB.with(|slot| *slot.borrow())
}

#[cfg(target_os = "macos")]
fn active_content_uri() -> String {
    ACTIVE_CONTENT_URI
        .with(|slot| slot.borrow().clone())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn set_active_content_browser(browser: *mut cef_browser_t) {
    ACTIVE_CONTENT_BROWSER.with(|slot| {
        *slot.borrow_mut() = browser;
    });
}

#[cfg(target_os = "macos")]
fn active_content_browser() -> *mut cef_browser_t {
    ACTIVE_CONTENT_BROWSER.with(|slot| *slot.borrow())
}

#[cfg(any(test, not(target_os = "macos")))]
#[derive(Debug, Default)]
pub struct MockCefHost {
    next_window_id: u64,
    next_ui_view_id: u64,
    next_content_view_id: u64,
    events: Vec<HostEvent>,
}

#[cfg(any(test, not(target_os = "macos")))]
impl MockCefHost {
    pub fn events(&self) -> &[HostEvent] {
        &self.events
    }
}

#[cfg(any(test, not(target_os = "macos")))]
impl CefHost for MockCefHost {
    type Error = HostError;

    fn create_window(&mut self, title: &str) -> Result<WindowId, Self::Error> {
        self.next_window_id += 1;
        let window_id = WindowId(self.next_window_id);
        self.events.push(HostEvent::WindowCreated {
            window_id,
            title: title.to_owned(),
        });
        Ok(window_id)
    }

    fn create_ui_view(&mut self, window_id: WindowId, url: &str) -> Result<UiViewId, Self::Error> {
        if !url.starts_with("app://ui") {
            return Err(HostError::InvalidUiUrl(url.to_owned()));
        }

        self.next_ui_view_id += 1;
        let view_id = UiViewId(self.next_ui_view_id);
        self.events.push(HostEvent::UiViewCreated {
            window_id,
            view_id,
            url: url.to_owned(),
        });
        Ok(view_id)
    }

    fn create_content_view(
        &mut self,
        window_id: WindowId,
        tab_id: TabId,
        url: &str,
    ) -> Result<ContentViewId, Self::Error> {
        self.next_content_view_id += 1;
        let view_id = ContentViewId(self.next_content_view_id);
        self.events.push(HostEvent::ContentViewCreated {
            window_id,
            view_id,
            tab_id,
            url: url.to_owned(),
        });
        Ok(view_id)
    }

    fn navigate_content_view(
        &mut self,
        view_id: ContentViewId,
        tab_id: TabId,
        url: &str,
    ) -> Result<(), Self::Error> {
        self.events.push(HostEvent::ContentNavigated {
            view_id,
            tab_id,
            url: url.to_owned(),
        });
        Ok(())
    }

    fn set_content_view_visible(
        &mut self,
        _view_id: ContentViewId,
        _visible: bool,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn clear_content_view(&mut self, _view_id: ContentViewId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn run_event_loop(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub type DefaultHost = NativeMacHost;
#[cfg(not(target_os = "macos"))]
pub type DefaultHost = MockCefHost;

#[cfg(target_os = "macos")]
use std::ffi::{c_char, c_int, c_void, CStr, CString};
#[cfg(target_os = "macos")]
use std::mem::{size_of, zeroed};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::slice;

#[cfg(target_os = "macos")]
use switchboard_cef_sys::loader::CefLibrary;
#[cfg(target_os = "macos")]
use switchboard_cef_sys::raw::{
    cef_app_t, cef_base_ref_counted_t, cef_browser_host_create_browser_fn, cef_browser_settings_t,
    cef_browser_t, cef_callback_t, cef_client_t, cef_display_handler_t, cef_frame_t,
    cef_jsdialog_callback_t, cef_jsdialog_handler_t, cef_main_args_t, cef_rect_t, cef_request_t,
    cef_resource_handler_t, cef_response_t, cef_scheme_handler_factory_t, cef_scheme_registrar_t,
    cef_settings_t, cef_string_t, cef_string_utf16_t, cef_window_info_t, CEF_RUNTIME_STYLE_ALLOY,
    CEF_SCHEME_OPTION_CORS_ENABLED, CEF_SCHEME_OPTION_DISPLAY_ISOLATED,
    CEF_SCHEME_OPTION_FETCH_ENABLED, CEF_SCHEME_OPTION_SECURE, CEF_SCHEME_OPTION_STANDARD,
    JSDIALOGTYPE_PROMPT,
};

#[cfg(target_os = "macos")]
type ObjcId = *mut c_void;
#[cfg(target_os = "macos")]
type ObjcSel = *mut c_void;

#[cfg(target_os = "macos")]
const NIL: ObjcId = std::ptr::null_mut();
#[cfg(target_os = "macos")]
const YES: i8 = 1;
#[cfg(target_os = "macos")]
const NO: i8 = 0;

#[cfg(target_os = "macos")]
const WINDOW_WIDTH: f64 = 1280.0;
#[cfg(target_os = "macos")]
const WINDOW_HEIGHT: f64 = 840.0;
#[cfg(target_os = "macos")]
const UI_TOP_HEIGHT: f64 = 44.0;
#[cfg(target_os = "macos")]
const UI_LEFT_WIDTH: f64 = 320.0;
#[cfg(target_os = "macos")]
const CONTENT_WIDTH: f64 = WINDOW_WIDTH - UI_LEFT_WIDTH;
#[cfg(target_os = "macos")]
const CONTENT_HEIGHT: f64 = WINDOW_HEIGHT - UI_TOP_HEIGHT;
#[cfg(target_os = "macos")]
const DEFAULT_BACKGROUND_COLOR: u32 = 0xFF_FF_FF_FF;
#[cfg(target_os = "macos")]
const ENV_CEF_DIST: &str = "SWITCHBOARD_CEF_DIST";
#[cfg(target_os = "macos")]
const ENV_CEF_LIBRARY: &str = "SWITCHBOARD_CEF_LIBRARY";
#[cfg(target_os = "macos")]
const ENV_CEF_FRAMEWORK_DIR: &str = "SWITCHBOARD_CEF_FRAMEWORK_DIR";
#[cfg(target_os = "macos")]
const ENV_CEF_RESOURCES_DIR: &str = "SWITCHBOARD_CEF_RESOURCES_DIR";
#[cfg(target_os = "macos")]
const ENV_CEF_BROWSER_SUBPROCESS: &str = "SWITCHBOARD_CEF_BROWSER_SUBPROCESS";
#[cfg(target_os = "macos")]
const ENV_CEF_MAIN_BUNDLE_PATH: &str = "SWITCHBOARD_CEF_MAIN_BUNDLE_PATH";
#[cfg(target_os = "macos")]
const ENV_CEF_VERBOSE_ERRORS: &str = "SWITCHBOARD_CEF_VERBOSE_ERRORS";
#[cfg(target_os = "macos")]
const ENV_CEF_ROOT_CACHE_PATH: &str = "SWITCHBOARD_CEF_ROOT_CACHE_PATH";
#[cfg(target_os = "macos")]
const ENV_CEF_TMPDIR: &str = "SWITCHBOARD_CEF_TMPDIR";
#[cfg(target_os = "macos")]
const ENV_CEF_API_VERSION: &str = "SWITCHBOARD_CEF_API_VERSION";
#[cfg(target_os = "macos")]
const ENV_CEF_USE_MOCK_KEYCHAIN: &str = "SWITCHBOARD_CEF_USE_MOCK_KEYCHAIN";
#[cfg(target_os = "macos")]
const ENV_CEF_PASSWORD_STORE: &str = "SWITCHBOARD_CEF_PASSWORD_STORE";
#[cfg(target_os = "macos")]
const DEFAULT_CEF_API_VERSION: i32 = 14500;

#[cfg(target_os = "macos")]
const STYLE_TITLED: u64 = 1 << 0;
#[cfg(target_os = "macos")]
const STYLE_CLOSABLE: u64 = 1 << 1;
#[cfg(target_os = "macos")]
const STYLE_MINIATURIZABLE: u64 = 1 << 2;
#[cfg(target_os = "macos")]
const STYLE_RESIZABLE: u64 = 1 << 3;
#[cfg(target_os = "macos")]
const STYLE_FULL_SIZE_CONTENT_VIEW: u64 = 1 << 15;
#[cfg(target_os = "macos")]
const BACKING_STORE_BUFFERED: u64 = 2;
#[cfg(target_os = "macos")]
const APP_ACTIVATION_POLICY_REGULAR: i64 = 0;
#[cfg(target_os = "macos")]
const WINDOW_TITLE_HIDDEN: i64 = 1;
#[cfg(target_os = "macos")]
const NS_VIEW_WIDTH_SIZABLE: u64 = 2;
#[cfg(target_os = "macos")]
const NS_VIEW_HEIGHT_SIZABLE: u64 = 16;
#[cfg(target_os = "macos")]
const UI_VIEW_AUTORE_SIZE_MASK: u64 = NS_VIEW_WIDTH_SIZABLE | NS_VIEW_HEIGHT_SIZABLE;
#[cfg(target_os = "macos")]
const CONTENT_VIEW_AUTORE_SIZE_MASK: u64 = NS_VIEW_WIDTH_SIZABLE | NS_VIEW_HEIGHT_SIZABLE;
#[cfg(target_os = "macos")]
const UI_SCHEME: &str = "app";
#[cfg(target_os = "macos")]
const UI_INTENT_PROMPT_MARKER: &str = "__switchboard_intent__";
#[cfg(target_os = "macos")]
const UI_SCHEME_OPTIONS: u32 = CEF_SCHEME_OPTION_STANDARD
    | CEF_SCHEME_OPTION_SECURE
    | CEF_SCHEME_OPTION_CORS_ENABLED
    | CEF_SCHEME_OPTION_FETCH_ENABLED
    | CEF_SCHEME_OPTION_DISPLAY_ISOLATED;
#[cfg(target_os = "macos")]
const UI_SHELL_TEMPLATE_HTML: &str = include_str!("ui_shell.html");
#[cfg(target_os = "macos")]
const UI_SHELL_CSS: &str = include_str!("ui_shell.css");
#[cfg(target_os = "macos")]
const UI_SHELL_JS: &str = include_str!("ui_shell.js");
#[cfg(target_os = "macos")]
static UI_SHELL_BODY_BYTES: OnceLock<Vec<u8>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn ui_shell_body() -> &'static [u8] {
    UI_SHELL_BODY_BYTES
        .get_or_init(|| {
            UI_SHELL_TEMPLATE_HTML
                .replace("/* __SWITCHBOARD_UI_SHELL_CSS__ */", UI_SHELL_CSS)
                .replace("// __SWITCHBOARD_UI_SHELL_JS__", UI_SHELL_JS)
                .into_bytes()
        })
        .as_slice()
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

#[cfg(target_os = "macos")]
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> ObjcId;
    fn sel_registerName(name: *const c_char) -> ObjcSel;
    fn objc_msgSend();
    fn class_addMethod(
        cls: ObjcId,
        name: ObjcSel,
        imp: *const c_void,
        types: *const c_char,
    ) -> i8;
}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    fn NSApplicationLoad() -> i8;
}

#[cfg(target_os = "macos")]
#[link(name = "WebKit", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
static NSAPP_HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static CEF_QUIT_MESSAGE_LOOP_FN: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
enum ContentBackend {
    WebKit(ObjcId),
    Cef(ObjcId),
}

#[cfg(target_os = "macos")]
struct CefRuntime {
    library: CefLibrary,
    config: CefConfig,
    _app: *mut cef_app_t,
    _ui_scheme_factory: *mut cef_scheme_handler_factory_t,
    ui_client: *mut cef_client_t,
}

#[cfg(target_os = "macos")]
struct CefConfig {
    library_path: PathBuf,
    framework_dir_path: PathBuf,
    resources_dir_path: Option<PathBuf>,
    browser_subprocess_path: Option<PathBuf>,
    main_bundle_path: Option<PathBuf>,
    root_cache_path: PathBuf,
    cache_path: PathBuf,
    temp_dir: PathBuf,
}

#[cfg(target_os = "macos")]
struct CefString {
    value: cef_string_t,
    clear: switchboard_cef_sys::raw::cef_string_utf16_clear_fn,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardCefApp {
    app: cef_app_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardUiSchemeFactory {
    factory: cef_scheme_handler_factory_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardUiResourceHandler {
    handler: cef_resource_handler_t,
    offset: usize,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardUiJsDialogHandler {
    handler: cef_jsdialog_handler_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardUiClient {
    client: cef_client_t,
    jsdialog_handler: *mut cef_jsdialog_handler_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardContentDisplayHandler {
    handler: cef_display_handler_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardContentClient {
    client: cef_client_t,
    display_handler: *mut cef_display_handler_t,
}

#[cfg(target_os = "macos")]
impl CefConfig {
    fn from_env() -> Result<Option<Self>, HostError> {
        if let Some(library_path) = env_path(ENV_CEF_LIBRARY) {
            return Ok(Some(Self::from_library_path(library_path)?));
        }

        if let Some(dist_root) = env_path(ENV_CEF_DIST) {
            let release_dir = dist_root.join("Release");
            let framework_dir_path = env_path(ENV_CEF_FRAMEWORK_DIR)
                .unwrap_or_else(|| release_dir.join("Chromium Embedded Framework.framework"));
            let library_path = framework_dir_path.join("Chromium Embedded Framework");
            let resources_dir_path =
                env_path(ENV_CEF_RESOURCES_DIR).or(Some(framework_dir_path.join("Resources")));
            let browser_subprocess_path = env_path(ENV_CEF_BROWSER_SUBPROCESS)
                .or_else(|| detect_cef_helper_binary(&release_dir))
                .or(Some(default_subprocess_path()?));
            let main_bundle_path = env_path(ENV_CEF_MAIN_BUNDLE_PATH);
            let root_cache_path =
                env_path(ENV_CEF_ROOT_CACHE_PATH).unwrap_or(default_root_cache_path()?);
            let cache_path = root_cache_path.join("default");
            let temp_dir = env_path(ENV_CEF_TMPDIR).unwrap_or(default_temp_dir());
            return Ok(Some(Self {
                library_path,
                framework_dir_path,
                resources_dir_path,
                browser_subprocess_path,
                main_bundle_path,
                root_cache_path,
                cache_path,
                temp_dir,
            }));
        }

        Ok(None)
    }

    fn from_library_path(library_path: PathBuf) -> Result<Self, HostError> {
        let framework_dir_path = env_path(ENV_CEF_FRAMEWORK_DIR)
            .or_else(|| library_path.parent().map(Path::to_path_buf))
            .ok_or_else(|| {
                HostError::Native(
                    "cannot infer CEF framework directory from SWITCHBOARD_CEF_LIBRARY".to_owned(),
                )
            })?;
        let resources_dir_path =
            env_path(ENV_CEF_RESOURCES_DIR).or(Some(framework_dir_path.join("Resources")));
        let browser_subprocess_path =
            env_path(ENV_CEF_BROWSER_SUBPROCESS).or(Some(default_subprocess_path()?));
        let main_bundle_path = env_path(ENV_CEF_MAIN_BUNDLE_PATH);
        let root_cache_path =
            env_path(ENV_CEF_ROOT_CACHE_PATH).unwrap_or(default_root_cache_path()?);
        let cache_path = root_cache_path.join("default");
        let temp_dir = env_path(ENV_CEF_TMPDIR).unwrap_or(default_temp_dir());

        Ok(Self {
            library_path,
            framework_dir_path,
            resources_dir_path,
            browser_subprocess_path,
            main_bundle_path,
            root_cache_path,
            cache_path,
            temp_dir,
        })
    }

    fn summary(&self) -> String {
        format!(
            "  library           : {}\n  framework_dir     : {}\n  resources_dir     : {}\n  browser_subprocess: {}\n  main_bundle       : {}\n  root_cache_path   : {}\n  cache_path        : {}\n  tmp_dir           : {}",
            describe_path(&self.library_path),
            describe_path(&self.framework_dir_path),
            describe_optional_path(&self.resources_dir_path),
            describe_optional_path(&self.browser_subprocess_path),
            describe_optional_path(&self.main_bundle_path),
            describe_path(&self.root_cache_path),
            describe_path(&self.cache_path),
            describe_path(&self.temp_dir),
        )
    }
}

#[cfg(target_os = "macos")]
impl CefString {
    fn new(library: &CefLibrary, value: &str) -> Result<Self, HostError> {
        let utf16: Vec<u16> = value.encode_utf16().collect();
        let mut cef_value: cef_string_utf16_t = unsafe { zeroed() };
        let ok = unsafe {
            (library.api.cef_string_utf16_set)(utf16.as_ptr(), utf16.len(), &mut cef_value, 1)
        };
        if ok == 0 {
            return Err(HostError::Native(format!(
                "failed to convert string into CEF UTF-16 buffer: {value}"
            )));
        }
        Ok(Self {
            value: cef_value,
            clear: library.api.cef_string_utf16_clear,
        })
    }

    fn value(&self) -> cef_string_t {
        self.value
    }

    fn as_ptr(&self) -> *const cef_string_t {
        &self.value as *const cef_string_t
    }
}

#[cfg(target_os = "macos")]
impl Drop for CefString {
    fn drop(&mut self) {
        unsafe {
            (self.clear)(&mut self.value);
        }
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_add_ref_noop(_self_: *mut cef_base_ref_counted_t) {}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_release_noop(_self_: *mut cef_base_ref_counted_t) -> c_int {
    0
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_has_one_ref_true(
    _self_: *mut cef_base_ref_counted_t,
) -> c_int {
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_has_at_least_one_ref_true(
    _self_: *mut cef_base_ref_counted_t,
) -> c_int {
    1
}

#[cfg(target_os = "macos")]
fn ref_counted_base<T>() -> cef_base_ref_counted_t {
    cef_base_ref_counted_t {
        size: size_of::<T>(),
        add_ref: Some(cef_ref_counted_add_ref_noop),
        release: Some(cef_ref_counted_release_noop),
        has_one_ref: Some(cef_ref_counted_has_one_ref_true),
        has_at_least_one_ref: Some(cef_ref_counted_has_at_least_one_ref_true),
    }
}

#[cfg(target_os = "macos")]
unsafe fn cef_string_to_owned(value: *const cef_string_t) -> String {
    if value.is_null() {
        return String::new();
    }
    if (*value).str_.is_null() || (*value).length == 0 {
        return String::new();
    }
    let units = slice::from_raw_parts((*value).str_, (*value).length);
    String::from_utf16_lossy(units)
}

#[cfg(target_os = "macos")]
fn with_stack_cef_string(value: &str, callback: impl FnOnce(*const cef_string_t)) {
    let utf16: Vec<u16> = value.encode_utf16().collect();
    let cef_value = cef_string_t {
        str_: utf16.as_ptr() as *mut u16,
        length: utf16.len(),
        dtor: None,
    };
    callback(&cef_value);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_app_on_register_custom_schemes(
    _self_: *mut cef_app_t,
    registrar: *mut cef_scheme_registrar_t,
) {
    if registrar.is_null() {
        return;
    }
    let Some(add_custom_scheme) = (*registrar).add_custom_scheme else {
        return;
    };
    with_stack_cef_string(UI_SCHEME, |scheme_name| unsafe {
        let _ = add_custom_scheme(registrar, scheme_name, UI_SCHEME_OPTIONS as c_int);
    });
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_is_handling_send_event(
    _self: ObjcId,
    _cmd: ObjcSel,
) -> i8 {
    if NSAPP_HANDLING_SEND_EVENT.load(Ordering::Relaxed) {
        YES
    } else {
        NO
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_set_handling_send_event(
    _self: ObjcId,
    _cmd: ObjcSel,
    value: i8,
) {
    NSAPP_HANDLING_SEND_EVENT.store(value != NO, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn install_nsapplication_event_shim() -> Result<(), HostError> {
    unsafe {
        let app_class = objc_class("NSApplication")?;
        let is_handling_selector = selector("isHandlingSendEvent")?;
        let set_handling_selector = selector("setHandlingSendEvent:")?;
        let is_encoding = CString::new("c@:").expect("static signature should be valid");
        let set_encoding = CString::new("v@:c").expect("static signature should be valid");

        let _ = class_addMethod(
            app_class,
            is_handling_selector,
            switchboard_nsapp_is_handling_send_event as *const c_void,
            is_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            set_handling_selector,
            switchboard_nsapp_set_handling_send_event as *const c_void,
            set_encoding.as_ptr(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_cef_quit_message_loop_hook(quit: unsafe extern "C" fn()) {
    CEF_QUIT_MESSAGE_LOOP_FN.store(quit as usize, Ordering::Release);
}

#[cfg(target_os = "macos")]
fn clear_cef_quit_message_loop_hook() {
    CEF_QUIT_MESSAGE_LOOP_FN.store(0, Ordering::Release);
}

#[cfg(target_os = "macos")]
fn quit_cef_message_loop_if_available() {
    let raw = CEF_QUIT_MESSAGE_LOOP_FN.load(Ordering::Acquire);
    if raw == 0 {
        return;
    }
    let quit: unsafe extern "C" fn() = unsafe { std::mem::transmute(raw) };
    unsafe {
        quit();
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_window_should_close(
    self_: ObjcId,
    _cmd: ObjcSel,
    _window: ObjcId,
) -> i8 {
    quit_cef_message_loop_if_available();
    let terminate_sel = sel_registerName(b"terminate:\0".as_ptr() as *const c_char);
    if terminate_sel != NIL {
        msg_send_void_id(self_, terminate_sel, NIL);
        return NO;
    }
    YES
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_application_will_terminate(
    _self: ObjcId,
    _cmd: ObjcSel,
    _notification: ObjcId,
) {
    quit_cef_message_loop_if_available();
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_should_handle_reopen(
    self_: ObjcId,
    _cmd: ObjcSel,
    _application: ObjcId,
    has_visible_windows: i8,
) -> i8 {
    if has_visible_windows == NO {
        let windows_sel = sel_registerName(b"windows\0".as_ptr() as *const c_char);
        let count_sel = sel_registerName(b"count\0".as_ptr() as *const c_char);
        let object_at_index_sel =
            sel_registerName(b"objectAtIndex:\0".as_ptr() as *const c_char);
        let make_key_and_order_front_sel =
            sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as *const c_char);
        if windows_sel != NIL
            && count_sel != NIL
            && object_at_index_sel != NIL
            && make_key_and_order_front_sel != NIL
        {
            let windows = msg_send_id(self_, windows_sel);
            if windows != NIL && msg_send_usize(windows, count_sel) > 0 {
                let window = msg_send_id_usize(windows, object_at_index_sel, 0);
                if window != NIL {
                    msg_send_void_id(window, make_key_and_order_front_sel, NIL);
                }
            }
        }
    }
    let activate_sel = sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as *const c_char);
    if activate_sel != NIL {
        msg_send_void_bool(self_, activate_sel, YES);
    }
    YES
}

#[cfg(target_os = "macos")]
fn install_nsapplication_suspend_shim() -> Result<(), HostError> {
    unsafe {
        let app_class = objc_class("NSApplication")?;
        let window_should_close_selector = selector("windowShouldClose:")?;
        let should_handle_reopen_selector =
            selector("applicationShouldHandleReopen:hasVisibleWindows:")?;
        let will_terminate_selector = selector("applicationWillTerminate:")?;
        let window_close_encoding = CString::new("c@:@").expect("static signature should be valid");
        let reopen_encoding = CString::new("c@:@c").expect("static signature should be valid");
        let will_terminate_encoding =
            CString::new("v@:@").expect("static signature should be valid");

        let _ = class_addMethod(
            app_class,
            window_should_close_selector,
            switchboard_nsapp_window_should_close as *const c_void,
            window_close_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            should_handle_reopen_selector,
            switchboard_nsapp_should_handle_reopen as *const c_void,
            reopen_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            will_terminate_selector,
            switchboard_nsapp_application_will_terminate as *const c_void,
            will_terminate_encoding.as_ptr(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn allocate_cef_app() -> *mut cef_app_t {
    let app = Box::new(SwitchboardCefApp {
        app: cef_app_t {
            base: ref_counted_base::<cef_app_t>(),
            on_before_command_line_processing: None,
            on_register_custom_schemes: Some(switchboard_app_on_register_custom_schemes),
            get_resource_bundle_handler: None,
            get_browser_process_handler: None,
            get_render_process_handler: None,
        },
    });
    let app_ptr = Box::into_raw(app);
    unsafe { &mut (*app_ptr).app as *mut cef_app_t }
}

#[cfg(target_os = "macos")]
enum UiPromptAction {
    Intent(UiCommand),
    QueryActiveUri,
    QueryShellState,
    UiReady,
}

#[cfg(target_os = "macos")]
fn parse_ui_prompt_payload(payload: &str) -> Result<UiPromptAction, &'static str> {
    let trimmed = payload.trim();
    if trimmed == "query_active_uri" {
        return Ok(UiPromptAction::QueryActiveUri);
    }
    if trimmed == "query_shell_state" {
        return Ok(UiPromptAction::QueryShellState);
    }
    if let Some(raw_name) = trimmed.strip_prefix("new_workspace ") {
        let name = raw_name.trim();
        if name.is_empty() {
            return Err("workspace name cannot be empty");
        }
        return Ok(UiPromptAction::Intent(UiCommand::NewWorkspace {
            name: name.to_owned(),
        }));
    }
    if trimmed == "new_workspace" {
        return Ok(UiPromptAction::Intent(UiCommand::NewWorkspace {
            name: "New Workspace".to_owned(),
        }));
    }
    if let Some(rest) = trimmed.strip_prefix("rename_workspace ") {
        let mut parts = rest.trim().splitn(2, ' ');
        let workspace_id = parts
            .next()
            .ok_or("rename_workspace requires workspace id")?
            .trim()
            .parse::<u64>()
            .map_err(|_| "rename_workspace requires a numeric workspace id")?;
        let name = parts
            .next()
            .ok_or("rename_workspace requires a name")?
            .trim();
        if name.is_empty() {
            return Err("workspace name cannot be empty");
        }
        return Ok(UiPromptAction::Intent(UiCommand::RenameWorkspace {
            workspace_id,
            name: name.to_owned(),
        }));
    }
    if let Some(value) = trimmed.strip_prefix("delete_workspace ") {
        let workspace_id = value
            .trim()
            .parse::<u64>()
            .map_err(|_| "delete_workspace requires a numeric workspace id")?;
        return Ok(UiPromptAction::Intent(UiCommand::DeleteWorkspace {
            workspace_id,
        }));
    }
    if let Some(value) = trimmed.strip_prefix("switch_workspace ") {
        let workspace_id = value
            .trim()
            .parse::<u64>()
            .map_err(|_| "switch_workspace requires a numeric workspace id")?;
        return Ok(UiPromptAction::Intent(UiCommand::SwitchWorkspace {
            workspace_id,
        }));
    }
    if let Some(value) = trimmed.strip_prefix("activate_tab ") {
        let tab_id = value
            .trim()
            .parse::<u64>()
            .map_err(|_| "activate_tab requires a numeric tab id")?;
        return Ok(UiPromptAction::Intent(UiCommand::ActivateTab { tab_id }));
    }
    if let Some(value) = trimmed.strip_prefix("close_tab ") {
        let tab_id = value
            .trim()
            .parse::<u64>()
            .map_err(|_| "close_tab requires a numeric tab id")?;
        return Ok(UiPromptAction::Intent(UiCommand::CloseTab { tab_id }));
    }
    if let Some(value) = trimmed.strip_prefix("new_tab ") {
        let workspace_id = value
            .trim()
            .parse::<u64>()
            .map_err(|_| "new_tab requires a numeric workspace id")?;
        return Ok(UiPromptAction::Intent(UiCommand::NewTab {
            workspace_id,
            url: None,
            make_active: true,
        }));
    }
    if let Some(url) = trimmed.strip_prefix("navigate ") {
        let normalized = url.trim();
        if normalized.starts_with("https://") || normalized.starts_with("http://") {
            return Ok(UiPromptAction::Intent(UiCommand::NavigateActive {
                url: normalized.to_owned(),
            }));
        }
        return Err("navigate intents only allow http/https URLs");
    }
    if trimmed.starts_with("ui_ready ") {
        return Ok(UiPromptAction::UiReady);
    }
    Err("prompt payload is not in the allowlist")
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_ui_on_jsdialog(
    _self_: *mut cef_jsdialog_handler_t,
    _browser: *mut cef_browser_t,
    _origin_url: *const cef_string_t,
    dialog_type: switchboard_cef_sys::raw::cef_jsdialog_type_t,
    message_text: *const cef_string_t,
    default_prompt_text: *const cef_string_t,
    callback: *mut cef_jsdialog_callback_t,
    suppress_message: *mut c_int,
) -> c_int {
    if dialog_type != JSDIALOGTYPE_PROMPT {
        return 0;
    }

    let marker = cef_string_to_owned(message_text);
    if marker != UI_INTENT_PROMPT_MARKER {
        return 0;
    }

    let payload = cef_string_to_owned(default_prompt_text);
    match parse_ui_prompt_payload(&payload) {
        Ok(UiPromptAction::Intent(command)) => {
            if !suppress_message.is_null() {
                *suppress_message = 1;
            }
            eprintln!("switchboard-app: accepted UI intent `{payload}`");
            emit_ui_command(command);
            0
        }
        Ok(UiPromptAction::UiReady) => {
            if !suppress_message.is_null() {
                *suppress_message = 1;
            }
            0
        }
        Ok(UiPromptAction::QueryShellState) => {
            if callback.is_null() {
                if !suppress_message.is_null() {
                    *suppress_message = 1;
                }
                return 0;
            }
            let Some(cont) = (*callback).cont else {
                if !suppress_message.is_null() {
                    *suppress_message = 1;
                }
                return 0;
            };
            let json = query_ui_shell_state();
            with_stack_cef_string(&json, |value| unsafe {
                cont(callback, 1, value);
            });
            1
        }
        Ok(UiPromptAction::QueryActiveUri) => {
            if callback.is_null() {
                if !suppress_message.is_null() {
                    *suppress_message = 1;
                }
                return 0;
            }
            let Some(cont) = (*callback).cont else {
                if !suppress_message.is_null() {
                    *suppress_message = 1;
                }
                return 0;
            };
            let url = active_content_uri();
            with_stack_cef_string(&url, |value| unsafe {
                cont(callback, 1, value);
            });
            if env_flag(ENV_CEF_VERBOSE_ERRORS) {
                eprintln!("switchboard-app: served active URI query -> {url}");
            }
            1
        }
        Err(reason) => {
            if !suppress_message.is_null() {
                *suppress_message = 1;
            }
            eprintln!("switchboard-app: rejected UI prompt `{payload}` ({reason})");
            0
        }
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_ui_client_get_jsdialog_handler(
    self_: *mut cef_client_t,
) -> *mut cef_jsdialog_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardUiClient;
    (*client).jsdialog_handler
}

#[cfg(target_os = "macos")]
fn allocate_ui_jsdialog_handler() -> *mut cef_jsdialog_handler_t {
    let handler = Box::new(SwitchboardUiJsDialogHandler {
        handler: cef_jsdialog_handler_t {
            base: ref_counted_base::<cef_jsdialog_handler_t>(),
            on_jsdialog: Some(switchboard_ui_on_jsdialog),
            on_before_unload_dialog: None,
            on_reset_dialog_state: None,
            on_dialog_closed: None,
        },
    });
    let ptr = Box::into_raw(handler);
    unsafe { &mut (*ptr).handler as *mut cef_jsdialog_handler_t }
}

#[cfg(target_os = "macos")]
fn allocate_ui_cef_client() -> *mut cef_client_t {
    let jsdialog_handler = allocate_ui_jsdialog_handler();
    let client = Box::new(SwitchboardUiClient {
        client: cef_client_t {
            base: ref_counted_base::<cef_client_t>(),
            get_audio_handler: None,
            get_command_handler: None,
            get_context_menu_handler: None,
            get_dialog_handler: None,
            get_display_handler: None,
            get_download_handler: None,
            get_drag_handler: None,
            get_find_handler: None,
            get_focus_handler: None,
            get_frame_handler: None,
            get_permission_handler: None,
            get_jsdialog_handler: Some(switchboard_ui_client_get_jsdialog_handler),
            get_keyboard_handler: None,
            get_life_span_handler: None,
            get_load_handler: None,
            get_print_handler: None,
            get_render_handler: None,
            get_request_handler: None,
            on_process_message_received: None,
        },
        jsdialog_handler,
    });
    let ptr = Box::into_raw(client);
    unsafe { &mut (*ptr).client as *mut cef_client_t }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_open(
    _self_: *mut cef_resource_handler_t,
    _request: *mut cef_request_t,
    handle_request: *mut c_int,
    _callback: *mut cef_callback_t,
) -> c_int {
    if !handle_request.is_null() {
        *handle_request = 1;
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_process_request(
    _self_: *mut cef_resource_handler_t,
    _request: *mut cef_request_t,
    callback: *mut cef_callback_t,
) -> c_int {
    if !callback.is_null() {
        if let Some(cont) = (*callback).cont {
            cont(callback);
        }
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_get_response_headers(
    _self_: *mut cef_resource_handler_t,
    response: *mut cef_response_t,
    response_length: *mut i64,
    _redirect_url: *mut cef_string_t,
) {
    if !response_length.is_null() {
        *response_length = ui_shell_body().len() as i64;
    }
    if response.is_null() {
        return;
    }
    if let Some(set_status) = (*response).set_status {
        set_status(response, 200);
    }
    if let Some(set_status_text) = (*response).set_status_text {
        with_stack_cef_string("OK", |value| unsafe {
            set_status_text(response, value);
        });
    }
    if let Some(set_mime_type) = (*response).set_mime_type {
        with_stack_cef_string("text/html", |value| unsafe {
            set_mime_type(response, value);
        });
    }
    if let Some(set_charset) = (*response).set_charset {
        with_stack_cef_string("utf-8", |value| unsafe {
            set_charset(response, value);
        });
    }
    if let Some(set_header_by_name) = (*response).set_header_by_name {
        with_stack_cef_string("Cache-Control", |name| {
            with_stack_cef_string(
                "no-store, no-cache, must-revalidate, max-age=0",
                |value| unsafe {
                    set_header_by_name(response, name, value, 1);
                },
            );
        });
        with_stack_cef_string("Pragma", |name| {
            with_stack_cef_string("no-cache", |value| unsafe {
                set_header_by_name(response, name, value, 1);
            });
        });
        with_stack_cef_string("Expires", |name| {
            with_stack_cef_string("0", |value| unsafe {
                set_header_by_name(response, name, value, 1);
            });
        });
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_skip(
    self_: *mut cef_resource_handler_t,
    bytes_to_skip: i64,
    bytes_skipped: *mut i64,
    _callback: *mut switchboard_cef_sys::raw::cef_resource_skip_callback_t,
) -> c_int {
    if self_.is_null() {
        return 0;
    }
    let this = self_ as *mut SwitchboardUiResourceHandler;
    if bytes_to_skip <= 0 {
        if !bytes_skipped.is_null() {
            *bytes_skipped = 0;
        }
        return 1;
    }
    let total = ui_shell_body().len();
    let remaining = total.saturating_sub((*this).offset);
    let to_skip = (bytes_to_skip as usize).min(remaining);
    (*this).offset += to_skip;
    if !bytes_skipped.is_null() {
        *bytes_skipped = to_skip as i64;
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_read(
    self_: *mut cef_resource_handler_t,
    data_out: *mut c_void,
    bytes_to_read: c_int,
    bytes_read: *mut c_int,
    _callback: *mut switchboard_cef_sys::raw::cef_resource_read_callback_t,
) -> c_int {
    if self_.is_null() || data_out.is_null() || bytes_to_read <= 0 {
        if !bytes_read.is_null() {
            *bytes_read = 0;
        }
        return 0;
    }
    let this = self_ as *mut SwitchboardUiResourceHandler;
    let body = ui_shell_body();
    let remaining = body.len().saturating_sub((*this).offset);
    if remaining == 0 {
        if !bytes_read.is_null() {
            *bytes_read = 0;
        }
        return 0;
    }

    let chunk_len = (bytes_to_read as usize).min(remaining);
    std::ptr::copy_nonoverlapping(
        body.as_ptr().add((*this).offset),
        data_out as *mut u8,
        chunk_len,
    );
    (*this).offset += chunk_len;
    if !bytes_read.is_null() {
        *bytes_read = chunk_len as c_int;
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_read_response(
    self_: *mut cef_resource_handler_t,
    data_out: *mut c_void,
    bytes_to_read: c_int,
    bytes_read: *mut c_int,
    _callback: *mut cef_callback_t,
) -> c_int {
    switchboard_resource_handler_read(
        self_,
        data_out,
        bytes_to_read,
        bytes_read,
        std::ptr::null_mut(),
    )
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_cancel(_self_: *mut cef_resource_handler_t) {}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_scheme_factory_create(
    _self_: *mut cef_scheme_handler_factory_t,
    _browser: *mut cef_browser_t,
    _frame: *mut cef_frame_t,
    _scheme_name: *const cef_string_t,
    _request: *mut cef_request_t,
) -> *mut cef_resource_handler_t {
    let handler = Box::new(SwitchboardUiResourceHandler {
        handler: cef_resource_handler_t {
            base: ref_counted_base::<cef_resource_handler_t>(),
            open: Some(switchboard_resource_handler_open),
            process_request: Some(switchboard_resource_handler_process_request),
            get_response_headers: Some(switchboard_resource_handler_get_response_headers),
            skip: Some(switchboard_resource_handler_skip),
            read: Some(switchboard_resource_handler_read),
            read_response: Some(switchboard_resource_handler_read_response),
            cancel: Some(switchboard_resource_handler_cancel),
        },
        offset: 0,
    });
    let ptr = Box::into_raw(handler);
    &mut (*ptr).handler as *mut cef_resource_handler_t
}

#[cfg(target_os = "macos")]
fn allocate_ui_scheme_factory() -> *mut cef_scheme_handler_factory_t {
    let factory = Box::new(SwitchboardUiSchemeFactory {
        factory: cef_scheme_handler_factory_t {
            base: ref_counted_base::<cef_scheme_handler_factory_t>(),
            create: Some(switchboard_scheme_factory_create),
        },
    });
    let ptr = Box::into_raw(factory);
    unsafe { &mut (*ptr).factory as *mut cef_scheme_handler_factory_t }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_address_change(
    _self_: *mut cef_display_handler_t,
    browser: *mut cef_browser_t,
    frame: *mut cef_frame_t,
    url: *const cef_string_t,
) {
    if !frame.is_null() {
        if let Some(is_main) = (*frame).is_main {
            if is_main(frame) == 0 {
                return;
            }
        }
    }
    let next = cef_string_to_owned(url);
    if next.is_empty() {
        return;
    }
    if !(next.starts_with("https://") || next.starts_with("http://")) {
        return;
    }
    if !browser.is_null() {
        set_active_content_browser(browser);
    }
    set_active_content_uri(next.clone());
    if let Some(tab_id) = active_content_tab() {
        emit_content_event(ContentEvent::UrlChanged { tab_id, url: next.clone() });
    }
    if env_flag(ENV_CEF_VERBOSE_ERRORS) {
        eprintln!("switchboard-app: observed content URL change -> {next}");
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_title_change(
    _self_: *mut cef_display_handler_t,
    browser: *mut cef_browser_t,
    title: *const cef_string_t,
) {
    if !browser.is_null() {
        set_active_content_browser(browser);
    }
    let next = cef_string_to_owned(title);
    if let Some(tab_id) = active_content_tab() {
        emit_content_event(ContentEvent::TitleChanged {
            tab_id,
            title: next,
        });
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_loading_progress_change(
    _self_: *mut cef_display_handler_t,
    browser: *mut cef_browser_t,
    progress: f64,
) {
    let mut is_loading = progress < 0.999_999;
    if !browser.is_null() {
        set_active_content_browser(browser);
        if let Some(is_loading_fn) = (*browser).is_loading {
            is_loading = is_loading_fn(browser) != 0;
        }
    }
    if let Some(tab_id) = active_content_tab() {
        emit_content_event(ContentEvent::LoadingChanged { tab_id, is_loading });
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_client_get_display_handler(
    self_: *mut cef_client_t,
) -> *mut cef_display_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardContentClient;
    (*client).display_handler
}

#[cfg(target_os = "macos")]
fn allocate_content_display_handler() -> *mut cef_display_handler_t {
    let handler = Box::new(SwitchboardContentDisplayHandler {
        handler: cef_display_handler_t {
            base: ref_counted_base::<cef_display_handler_t>(),
            on_address_change: Some(switchboard_content_on_address_change),
            on_title_change: Some(switchboard_content_on_title_change),
            on_favicon_urlchange: None,
            on_fullscreen_mode_change: None,
            on_tooltip: None,
            on_status_message: None,
            on_console_message: None,
            on_auto_resize: None,
            on_loading_progress_change: Some(switchboard_content_on_loading_progress_change),
            on_cursor_change: None,
            on_media_access_change: None,
            on_contents_bounds_change: None,
            get_root_window_screen_rect: None,
        },
    });
    let ptr = Box::into_raw(handler);
    unsafe { &mut (*ptr).handler as *mut cef_display_handler_t }
}

#[cfg(target_os = "macos")]
fn allocate_content_cef_client() -> *mut cef_client_t {
    let display_handler = allocate_content_display_handler();
    let client = Box::new(SwitchboardContentClient {
        client: cef_client_t {
            base: ref_counted_base::<cef_client_t>(),
            get_audio_handler: None,
            get_command_handler: None,
            get_context_menu_handler: None,
            get_dialog_handler: None,
            get_display_handler: Some(switchboard_content_client_get_display_handler),
            get_download_handler: None,
            get_drag_handler: None,
            get_find_handler: None,
            get_focus_handler: None,
            get_frame_handler: None,
            get_permission_handler: None,
            get_jsdialog_handler: None,
            get_keyboard_handler: None,
            get_life_span_handler: None,
            get_load_handler: None,
            get_print_handler: None,
            get_render_handler: None,
            get_request_handler: None,
            on_process_message_received: None,
        },
        display_handler,
    });
    let client_ptr = Box::into_raw(client);
    if env_flag(ENV_CEF_VERBOSE_ERRORS) {
        unsafe {
            eprintln!(
                "switchboard-app: allocated content CEF client ptr={client_ptr:p} base.size={}",
                (*client_ptr).client.base.size
            );
        }
    }
    unsafe { &mut (*client_ptr).client as *mut cef_client_t }
}

#[cfg(target_os = "macos")]
unsafe fn free_content_cef_client(client: *mut cef_client_t) {
    if client.is_null() {
        return;
    }
    let content_client = client as *mut SwitchboardContentClient;
    let display_handler = (*content_client).display_handler;
    if !display_handler.is_null() {
        drop(Box::from_raw(
            display_handler as *mut SwitchboardContentDisplayHandler,
        ));
    }
    drop(Box::from_raw(content_client));
}

#[cfg(target_os = "macos")]
impl CefRuntime {
    fn from_environment() -> Result<Option<Self>, HostError> {
        let Some(config) = CefConfig::from_env()? else {
            return Ok(None);
        };
        let config_summary = config.summary();
        let verbose_errors = env_flag(ENV_CEF_VERBOSE_ERRORS);

        unsafe {
            let library = CefLibrary::open(&config.library_path).map_err(|err| {
                let raw_error = err.to_string();
                let reason = short_cef_loader_reason(&raw_error);
                HostError::Native(format!(
                    "CEF bootstrap failed at: load framework\nreason: {reason}\nconfiguration:\n{config_summary}{}{}",
                    if is_gatekeeper_policy_error(&raw_error) {
                        "\nrecommended fix:\n  xattr -dr com.apple.quarantine <cef_dir>"
                    } else {
                        ""
                    },
                    if verbose_errors {
                        format!("\nraw loader detail:\n  {raw_error}")
                    } else {
                        "\nset SWITCHBOARD_CEF_VERBOSE_ERRORS=1 to include raw loader details".to_owned()
                    }
                ))
            })?;
            install_cef_quit_message_loop_hook(library.api.cef_quit_message_loop);
            let app = allocate_cef_app();
            let requested_api_version =
                env_i32(ENV_CEF_API_VERSION).unwrap_or(DEFAULT_CEF_API_VERSION);
            let api_hash = (library.api.cef_api_hash)(requested_api_version, 0);
            if api_hash.is_null() {
                let active_version = (library.api.cef_api_version)();
                return Err(HostError::Native(format!(
                    "CEF bootstrap failed at: configure api version\nreason: cef_api_hash returned null for version {requested_api_version}\n  active_cef_api_version: {active_version}\nconfiguration:\n{config_summary}\nset {ENV_CEF_API_VERSION} to a supported explicit version (for this CEF build, typically {DEFAULT_CEF_API_VERSION})"
                )));
            }
            if verbose_errors {
                let hash_string = CStr::from_ptr(api_hash).to_string_lossy();
                let active_version = (library.api.cef_api_version)();
                eprintln!(
                    "switchboard-app: configured CEF API version={active_version} requested={requested_api_version} platform_hash={hash_string}"
                );
            }

            let mut cef_args: Vec<String> = std::env::args().collect();
            let use_mock_keychain =
                env_flag_or_default(ENV_CEF_USE_MOCK_KEYCHAIN, cfg!(debug_assertions));
            if use_mock_keychain {
                upsert_cef_switch(&mut cef_args, "--use-mock-keychain");
            }
            if let Ok(password_store) = std::env::var(ENV_CEF_PASSWORD_STORE) {
                let trimmed = password_store.trim();
                if !trimmed.is_empty() {
                    upsert_cef_switch_with_value(&mut cef_args, "--password-store", trimmed);
                }
            }
            if verbose_errors {
                eprintln!(
                    "switchboard-app: CEF launch switches mock_keychain={} password_store={}",
                    use_mock_keychain,
                    std::env::var(ENV_CEF_PASSWORD_STORE).unwrap_or_else(|_| "unset".to_owned())
                );
            }

            let mut argv_storage = Vec::new();
            for arg in cef_args {
                let c_arg = CString::new(arg)
                    .map_err(|_| HostError::Native("argv contained interior NUL".to_owned()))?;
                argv_storage.push(c_arg);
            }
            let mut argv_ptrs: Vec<*mut c_char> = argv_storage
                .iter()
                .map(|arg| arg.as_ptr() as *mut c_char)
                .collect();
            let main_args = cef_main_args_t {
                argc: i32::try_from(argv_ptrs.len())
                    .map_err(|_| HostError::Native("too many argv values for CEF".to_owned()))?,
                argv: argv_ptrs.as_mut_ptr(),
            };

            let secondary_exit_code =
                (library.api.cef_execute_process)(&main_args, app, std::ptr::null_mut());
            if verbose_errors {
                eprintln!(
                    "switchboard-app: cef_execute_process returned {secondary_exit_code} (pid={})",
                    std::process::id()
                );
            }
            if secondary_exit_code >= 0 {
                std::process::exit(secondary_exit_code);
            }
            eprintln!("switchboard-app: CEF bootstrap configuration\n{config_summary}");

            let mut settings: cef_settings_t = zeroed();
            settings.size = size_of::<cef_settings_t>();
            settings.no_sandbox = 1;
            settings.background_color = DEFAULT_BACKGROUND_COLOR;

            std::fs::create_dir_all(&config.root_cache_path).map_err(|error| {
                HostError::Native(format!(
                    "failed creating CEF root_cache_path {}: {error}",
                    config.root_cache_path.display()
                ))
            })?;
            std::fs::create_dir_all(&config.cache_path).map_err(|error| {
                HostError::Native(format!(
                    "failed creating CEF cache_path {}: {error}",
                    config.cache_path.display()
                ))
            })?;
            std::fs::create_dir_all(&config.temp_dir).map_err(|error| {
                HostError::Native(format!(
                    "failed creating CEF temp_dir {}: {error}",
                    config.temp_dir.display()
                ))
            })?;
            std::env::set_var("TMPDIR", &config.temp_dir);
            std::env::set_var("TEMP", &config.temp_dir);
            std::env::set_var("TMP", &config.temp_dir);
            stage_cef_runtime_libraries(&config)?;

            let mut settings_strings = Vec::new();
            set_cef_path_string(
                &library,
                &mut settings.framework_dir_path,
                &config.framework_dir_path,
                &mut settings_strings,
            )?;
            set_cef_path_string(
                &library,
                &mut settings.root_cache_path,
                &config.root_cache_path,
                &mut settings_strings,
            )?;
            set_cef_path_string(
                &library,
                &mut settings.cache_path,
                &config.cache_path,
                &mut settings_strings,
            )?;
            if let Some(path) = config.resources_dir_path.as_ref() {
                set_cef_path_string(
                    &library,
                    &mut settings.resources_dir_path,
                    path,
                    &mut settings_strings,
                )?;
            }
            if let Some(path) = config.browser_subprocess_path.as_ref() {
                set_cef_path_string(
                    &library,
                    &mut settings.browser_subprocess_path,
                    path,
                    &mut settings_strings,
                )?;
            }
            if let Some(path) = config.main_bundle_path.as_ref() {
                set_cef_path_string(
                    &library,
                    &mut settings.main_bundle_path,
                    path,
                    &mut settings_strings,
                )?;
            }

            let initialized =
                (library.api.cef_initialize)(&main_args, &settings, app, std::ptr::null_mut());
            drop(settings_strings);
            if initialized == 0 {
                return Err(HostError::Native(format!(
                    "CEF bootstrap failed at: initialize\nreason: cef_initialize returned false\nconfiguration:\n{config_summary}\nnotes:\n  - if logs mention ProcessSingleton/SingletonSocket, ensure tmp/cache paths are writable\n  - override with SWITCHBOARD_CEF_ROOT_CACHE_PATH and SWITCHBOARD_CEF_TMPDIR if needed"
                )));
            }

            let app_scheme = CefString::new(&library, UI_SCHEME)?;
            let ui_scheme_factory = allocate_ui_scheme_factory();
            let registered = (library.api.cef_register_scheme_handler_factory)(
                app_scheme.as_ptr(),
                std::ptr::null(),
                ui_scheme_factory,
            );
            if registered == 0 {
                return Err(HostError::Native(
                    "CEF bootstrap failed at: register app:// scheme handler".to_owned(),
                ));
            }
            let ui_client = allocate_ui_cef_client();

            Ok(Some(Self {
                library,
                config,
                _app: app,
                _ui_scheme_factory: ui_scheme_factory,
                ui_client,
            }))
        }
    }

    fn create_browser_in_view(
        &self,
        parent_view: ObjcId,
        url: &str,
        client: *mut cef_client_t,
        width: f64,
        height: f64,
    ) -> Result<(), HostError> {
        unsafe {
            if env_flag(ENV_CEF_VERBOSE_ERRORS) {
                eprintln!(
                    "switchboard-app: creating CEF browser parent_view={parent_view:p} client={client:p}"
                );
            }
            let mut window_info: cef_window_info_t = zeroed();
            window_info.size = size_of::<cef_window_info_t>();
            window_info.bounds = cef_rect_t {
                x: 0,
                y: 0,
                width: width as c_int,
                height: height as c_int,
            };
            window_info.parent_view = parent_view;
            window_info.runtime_style = CEF_RUNTIME_STYLE_ALLOY;

            let mut browser_settings: cef_browser_settings_t = zeroed();
            browser_settings.size = size_of::<cef_browser_settings_t>();
            browser_settings.background_color = DEFAULT_BACKGROUND_COLOR;

            let url_value = CefString::new(&self.library, url)?;
            let create_browser: cef_browser_host_create_browser_fn =
                self.library.api.cef_browser_host_create_browser;
            let result = create_browser(
                &window_info,
                client,
                url_value.as_ptr(),
                &browser_settings,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if env_flag(ENV_CEF_VERBOSE_ERRORS) {
                eprintln!(
                    "switchboard-app: cef_browser_host_create_browser returned {result} for url={url}"
                );
            }

            if result == 0 {
                let config_summary = self.config.summary();
                return Err(HostError::Native(format!(
                    "CEF browser creation failed\nreason: cef_browser_host_create_browser returned 0\n  url               : {url}\n  parent_view       : {parent_view:p}\nconfiguration:\n{config_summary}"
                )));
            }
            Ok(())
        }
    }

    fn ui_client(&self) -> *mut cef_client_t {
        self.ui_client
    }

    fn run_message_loop(&self) {
        unsafe {
            (self.library.api.cef_run_message_loop)();
        }
    }

    fn shutdown(&self) {
        unsafe {
            (self.library.api.cef_shutdown)();
        }
        clear_cef_quit_message_loop_hook();
    }
}

#[cfg(target_os = "macos")]
fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

#[cfg(target_os = "macos")]
fn default_subprocess_path() -> Result<PathBuf, HostError> {
    std::env::current_exe().map_err(|error| {
        HostError::Native(format!(
            "failed to resolve current executable for browser_subprocess_path: {error}"
        ))
    })
}

#[cfg(target_os = "macos")]
fn default_root_cache_path() -> Result<PathBuf, HostError> {
    let current_dir = std::env::current_dir().map_err(|error| {
        HostError::Native(format!(
            "failed to resolve current directory for CEF cache path: {error}"
        ))
    })?;
    Ok(current_dir.join("target").join("cef_user_data"))
}

#[cfg(target_os = "macos")]
fn default_temp_dir() -> PathBuf {
    std::env::temp_dir().join("switchboard_cef_tmp")
}

#[cfg(target_os = "macos")]
fn describe_path(path: &Path) -> String {
    format!(
        "{} [exists: {}]",
        path.display(),
        if path.exists() { "yes" } else { "no" }
    )
}

#[cfg(target_os = "macos")]
fn describe_optional_path(path: &Option<PathBuf>) -> String {
    match path {
        Some(path) => describe_path(path),
        None => "unset".to_owned(),
    }
}

#[cfg(target_os = "macos")]
fn stage_cef_runtime_libraries(config: &CefConfig) -> Result<(), HostError> {
    let source_dir = config.framework_dir_path.join("Libraries");
    if !source_dir.is_dir() {
        return Ok(());
    }

    let subprocess = config
        .browser_subprocess_path
        .as_ref()
        .ok_or_else(|| HostError::Native("browser_subprocess_path was not resolved".to_owned()))?;
    let target_dir = subprocess.parent().ok_or_else(|| {
        HostError::Native(format!(
            "cannot resolve executable directory from browser_subprocess_path {}",
            subprocess.display()
        ))
    })?;

    for entry in std::fs::read_dir(&source_dir).map_err(|error| {
        HostError::Native(format!(
            "failed reading CEF Libraries dir {}: {error}",
            source_dir.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            HostError::Native(format!(
                "failed iterating CEF Libraries dir {}: {error}",
                source_dir.display()
            ))
        })?;
        let source = entry.path();
        if !source.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        let target = target_dir.join(file_name);
        if target.exists() {
            continue;
        }

        #[cfg(target_os = "macos")]
        {
            if std::os::unix::fs::symlink(&source, &target).is_ok() {
                continue;
            }
        }

        std::fs::copy(&source, &target).map_err(|error| {
            HostError::Native(format!(
                "failed staging CEF runtime lib {} -> {}: {error}",
                source.display(),
                target.display()
            ))
        })?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn env_flag_or_default(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

#[cfg(target_os = "macos")]
fn env_i32(key: &str) -> Option<i32> {
    std::env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            trimmed.parse::<i32>().ok()
        }
    })
}

#[cfg(target_os = "macos")]
fn short_cef_loader_reason(raw_error: &str) -> String {
    if is_gatekeeper_policy_error(raw_error) {
        return "macOS Gatekeeper/code-signing policy blocked the CEF framework".to_owned();
    }

    raw_error
        .split(": tried:")
        .next()
        .unwrap_or(raw_error)
        .trim()
        .to_owned()
}

#[cfg(target_os = "macos")]
fn is_gatekeeper_policy_error(raw_error: &str) -> bool {
    raw_error.contains("library load disallowed by system policy")
        || raw_error.contains("not valid for use in process")
}

#[cfg(target_os = "macos")]
fn detect_cef_helper_binary(release_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        "cefclient.app/Contents/Frameworks/cefclient Helper.app/Contents/MacOS/cefclient Helper",
        "cefsimple.app/Contents/Frameworks/cefsimple Helper.app/Contents/MacOS/cefsimple Helper",
        "ceftests.app/Contents/Frameworks/ceftests Helper.app/Contents/MacOS/ceftests Helper",
    ];
    for candidate in candidates {
        let path = release_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn set_cef_path_string(
    library: &CefLibrary,
    target: &mut cef_string_t,
    path: &Path,
    keep_alive: &mut Vec<CefString>,
) -> Result<(), HostError> {
    let value = CefString::new(library, &path.to_string_lossy())?;
    *target = value.value();
    keep_alive.push(value);
    Ok(())
}

#[cfg(target_os = "macos")]
fn upsert_cef_switch(args: &mut Vec<String>, switch: &str) {
    if args.iter().any(|arg| arg == switch) {
        return;
    }
    args.push(switch.to_owned());
}

#[cfg(target_os = "macos")]
fn upsert_cef_switch_with_value(args: &mut Vec<String>, switch: &str, value: &str) {
    let prefix = format!("{switch}=");
    if let Some(existing) = args.iter_mut().find(|arg| arg.starts_with(&prefix)) {
        *existing = format!("{prefix}{value}");
        return;
    }
    args.push(format!("{prefix}{value}"));
}

#[cfg(target_os = "macos")]
pub struct NativeMacHost {
    app: ObjcId,
    cef: Option<CefRuntime>,
    next_window_id: u64,
    next_ui_view_id: u64,
    next_content_view_id: u64,
    windows: HashMap<WindowId, ObjcId>,
    ui_views: HashMap<UiViewId, ObjcId>,
    content_views: HashMap<ContentViewId, ContentBackend>,
    content_view_windows: HashMap<ContentViewId, WindowId>,
    cef_clients: HashMap<ContentViewId, *mut cef_client_t>,
}

#[cfg(target_os = "macos")]
impl NativeMacHost {
    pub fn new() -> Result<Self, HostError> {
        let is_cef_subprocess = std::env::args().any(|arg| arg.starts_with("--type="));
        // Initialize CEF first so subprocesses can return from cef_execute_process
        // and exit before any Cocoa app activation/Dock integration happens.
        let cef = CefRuntime::from_environment()?;
        if is_cef_subprocess {
            return Err(HostError::Native(
                "CEF subprocess reached UI host initialization unexpectedly".to_owned(),
            ));
        }

        unsafe {
            if NSApplicationLoad() == NO {
                return Err(HostError::Native("failed to load NSApplication".to_owned()));
            }
            install_nsapplication_event_shim()?;
            install_nsapplication_suspend_shim()?;

            let app_class = objc_class("NSApplication")?;
            let app = msg_send_id(app_class, selector("sharedApplication")?);
            if app == NIL {
                return Err(HostError::Native(
                    "NSApplication sharedApplication returned nil".to_owned(),
                ));
            }
            msg_send_void_id(app, selector("setDelegate:")?, app);

            msg_send_void_i64(
                app,
                selector("setActivationPolicy:")?,
                APP_ACTIVATION_POLICY_REGULAR,
            );
            msg_send_void(app, selector("finishLaunching")?);

            Ok(Self {
                app,
                cef,
                next_window_id: 0,
                next_ui_view_id: 0,
                next_content_view_id: 0,
                windows: HashMap::new(),
                ui_views: HashMap::new(),
                content_views: HashMap::new(),
                content_view_windows: HashMap::new(),
                cef_clients: HashMap::new(),
            })
        }
    }

    fn window_for(&self, window_id: WindowId) -> Result<ObjcId, HostError> {
        self.windows
            .get(&window_id)
            .copied()
            .ok_or_else(|| HostError::Native(format!("window not found: {}", window_id.0)))
    }
}

#[cfg(target_os = "macos")]
impl CefHost for NativeMacHost {
    type Error = HostError;

    fn create_window(&mut self, title: &str) -> Result<WindowId, Self::Error> {
        unsafe {
            let style = STYLE_TITLED
                | STYLE_CLOSABLE
                | STYLE_MINIATURIZABLE
                | STYLE_RESIZABLE
                | STYLE_FULL_SIZE_CONTENT_VIEW;
            let frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: WINDOW_WIDTH,
                    height: WINDOW_HEIGHT,
                },
            };

            let window_class = objc_class("NSWindow")?;
            let window_alloc = msg_send_id(window_class, selector("alloc")?);
            let window = msg_send_id_rect_u64_u64_bool(
                window_alloc,
                selector("initWithContentRect:styleMask:backing:defer:")?,
                frame,
                style,
                BACKING_STORE_BUFFERED,
                NO,
            );
            if window == NIL {
                return Err(HostError::Native("failed to create NSWindow".to_owned()));
            }

            msg_send_void_bool(window, selector("setTitlebarAppearsTransparent:")?, YES);
            msg_send_void_i64(window, selector("setTitleVisibility:")?, WINDOW_TITLE_HIDDEN);
            msg_send_void_bool(window, selector("setMovableByWindowBackground:")?, YES);
            msg_send_void_id(window, selector("setDelegate:")?, self.app);
            msg_send_void(window, selector("center")?);
            let title_value = nsstring(title)?;
            msg_send_void_id(window, selector("setTitle:")?, title_value);
            msg_send_void_id(window, selector("makeKeyAndOrderFront:")?, NIL);
            msg_send_void_bool(self.app, selector("activateIgnoringOtherApps:")?, YES);

            self.next_window_id += 1;
            let window_id = WindowId(self.next_window_id);
            self.windows.insert(window_id, window);
            Ok(window_id)
        }
    }

    fn create_ui_view(&mut self, window_id: WindowId, url: &str) -> Result<UiViewId, Self::Error> {
        if !url.starts_with("app://ui") {
            return Err(HostError::InvalidUiUrl(url.to_owned()));
        }
        let cef = self.cef.as_ref().ok_or_else(|| {
            HostError::Native(
                "UI shell requires CEF. Set SWITCHBOARD_CEF_DIST or SWITCHBOARD_CEF_LIBRARY."
                    .to_owned(),
            )
        })?;

        unsafe {
            let window = self.window_for(window_id)?;
            let root_view = msg_send_id(window, selector("contentView")?);
            if root_view == NIL {
                return Err(HostError::Native("window content view is nil".to_owned()));
            }
            msg_send_void_bool(root_view, selector("setAutoresizesSubviews:")?, YES);

            let view_class = objc_class("NSView")?;
            let view_alloc = msg_send_id(view_class, selector("alloc")?);
            let frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: WINDOW_WIDTH,
                    height: WINDOW_HEIGHT,
                },
            };
            let ui_view = msg_send_id_rect(view_alloc, selector("initWithFrame:")?, frame);
            if ui_view == NIL {
                return Err(HostError::Native("failed to create UI view".to_owned()));
            }
            msg_send_void_u64(
                ui_view,
                selector("setAutoresizingMask:")?,
                UI_VIEW_AUTORE_SIZE_MASK,
            );

            msg_send_void_id(root_view, selector("addSubview:")?, ui_view);
            cef.create_browser_in_view(ui_view, url, cef.ui_client(), WINDOW_WIDTH, WINDOW_HEIGHT)?;

            self.next_ui_view_id += 1;
            let view_id = UiViewId(self.next_ui_view_id);
            self.ui_views.insert(view_id, ui_view);
            Ok(view_id)
        }
    }

    fn create_content_view(
        &mut self,
        window_id: WindowId,
        tab_id: TabId,
        url: &str,
    ) -> Result<ContentViewId, Self::Error> {
        if url.starts_with("app://") {
            return Err(HostError::InvalidContentUrl(url.to_owned()));
        }

        unsafe {
            let window = self.window_for(window_id)?;
            let root_view = msg_send_id(window, selector("contentView")?);
            if root_view == NIL {
                return Err(HostError::Native("window content view is nil".to_owned()));
            }
            msg_send_void_bool(root_view, selector("setAutoresizesSubviews:")?, YES);
            let frame = NSRect {
                origin: NSPoint {
                    x: UI_LEFT_WIDTH,
                    y: 0.0,
                },
                size: NSSize {
                    width: CONTENT_WIDTH,
                    height: CONTENT_HEIGHT,
                },
            };
            let content_view = create_content_container(frame)?;
            msg_send_void_u64(
                content_view,
                selector("setAutoresizingMask:")?,
                CONTENT_VIEW_AUTORE_SIZE_MASK,
            );
            msg_send_void_id(root_view, selector("addSubview:")?, content_view);

            let mut cef_client: Option<*mut cef_client_t> = None;
            let backend = if let Some(cef) = self.cef.as_ref() {
                set_active_content_browser(std::ptr::null_mut());
                let client = allocate_content_cef_client();
                if let Err(error) = cef.create_browser_in_view(
                    content_view,
                    url,
                    client,
                    CONTENT_WIDTH,
                    CONTENT_HEIGHT,
                ) {
                    free_content_cef_client(client);
                    return Err(error);
                }
                set_active_content_tab(Some(tab_id));
                set_active_content_uri(url.to_owned());
                cef_client = Some(client);
                ContentBackend::Cef(content_view)
            } else {
                attach_wk_web_view(content_view, url)?;
                set_active_content_tab(Some(tab_id));
                set_active_content_uri(url.to_owned());
                ContentBackend::WebKit(content_view)
            };

            let title_value = nsstring(&format!("Switchboard - {url}"))?;
            msg_send_void_id(window, selector("setTitle:")?, title_value);

            self.next_content_view_id += 1;
            let view_id = ContentViewId(self.next_content_view_id);
            self.content_views.insert(view_id, backend);
            self.content_view_windows.insert(view_id, window_id);
            if let Some(client) = cef_client {
                self.cef_clients.insert(view_id, client);
            }
            Ok(view_id)
        }
    }

    fn navigate_content_view(
        &mut self,
        view_id: ContentViewId,
        tab_id: TabId,
        url: &str,
    ) -> Result<(), Self::Error> {
        if url.starts_with("app://") {
            return Err(HostError::InvalidContentUrl(url.to_owned()));
        }

        let window_id = self
            .content_view_windows
            .get(&view_id)
            .copied()
            .ok_or_else(|| HostError::Native(format!("content view not found: {}", view_id.0)))?;

        unsafe {
            let window = self.window_for(window_id)?;
            let content_backend = self.content_views.get(&view_id).copied().ok_or_else(|| {
                HostError::Native(format!("content view not found: {}", view_id.0))
            })?;
            match content_backend {
                ContentBackend::WebKit(view) => {
                    attach_wk_web_view(view, url)?;
                    set_active_content_tab(Some(tab_id));
                    set_active_content_uri(url.to_owned());
                }
                ContentBackend::Cef(view) => {
                    if !navigate_active_cef_browser(url) {
                        let cef = self.cef.as_ref().ok_or_else(|| {
                            HostError::Native("CEF runtime unavailable".to_owned())
                        })?;
                        let client = self.cef_clients.get(&view_id).copied().ok_or_else(|| {
                            HostError::Native(format!(
                                "CEF client missing for content view: {}",
                                view_id.0
                            ))
                        })?;
                        set_active_content_browser(std::ptr::null_mut());
                        remove_all_subviews(view)?;
                        let (width, height) = current_view_size(view)?;
                        cef.create_browser_in_view(view, url, client, width, height)?;
                    }
                    set_active_content_tab(Some(tab_id));
                    set_active_content_uri(url.to_owned());
                }
            }
            let title_value = nsstring(&format!("Switchboard - {url}"))?;
            msg_send_void_id(window, selector("setTitle:")?, title_value);
        }
        Ok(())
    }

    fn set_content_view_visible(
        &mut self,
        view_id: ContentViewId,
        visible: bool,
    ) -> Result<(), Self::Error> {
        let content_backend = self
            .content_views
            .get(&view_id)
            .copied()
            .ok_or_else(|| HostError::Native(format!("content view not found: {}", view_id.0)))?;
        let container = match content_backend {
            ContentBackend::WebKit(view) | ContentBackend::Cef(view) => view,
        };

        unsafe {
            msg_send_void_bool(container, selector("setHidden:")?, if visible { NO } else { YES });
        }
        Ok(())
    }

    fn clear_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        let content_backend = self
            .content_views
            .get(&view_id)
            .copied()
            .ok_or_else(|| HostError::Native(format!("content view not found: {}", view_id.0)))?;
        unsafe {
            match content_backend {
                ContentBackend::WebKit(view) => {
                    attach_wk_web_view(view, "about:blank")?;
                }
                ContentBackend::Cef(view) => {
                    if !navigate_active_cef_browser("about:blank") {
                        let cef = self.cef.as_ref().ok_or_else(|| {
                            HostError::Native("CEF runtime unavailable".to_owned())
                        })?;
                        let client = self.cef_clients.get(&view_id).copied().ok_or_else(|| {
                            HostError::Native(format!(
                                "CEF client missing for content view: {}",
                                view_id.0
                            ))
                        })?;
                        set_active_content_browser(std::ptr::null_mut());
                        remove_all_subviews(view)?;
                        let (width, height) = current_view_size(view)?;
                        cef.create_browser_in_view(view, "about:blank", client, width, height)?;
                    }
                }
            }
        }
        set_active_content_tab(None);
        set_active_content_uri(String::new());
        Ok(())
    }

    fn run_event_loop(&mut self) -> Result<(), Self::Error> {
        unsafe {
            msg_send_void_bool(self.app, selector("activateIgnoringOtherApps:")?, YES);
            if self.cef.is_some() {
                // Keep CEF runtime available while message loop is running so
                // UI-originated intents can navigate content views.
                let cef = self
                    .cef
                    .as_ref()
                    .ok_or_else(|| HostError::Native("CEF runtime unavailable".to_owned()))?;
                cef.run_message_loop();
                for client in self.cef_clients.values().copied() {
                    free_content_cef_client(client);
                }
                cef.shutdown();
                self.cef = None;
                set_active_content_tab(None);
                set_active_content_browser(std::ptr::null_mut());
            } else {
                msg_send_void(self.app, selector("run")?);
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
unsafe fn objc_class(name: &str) -> Result<ObjcId, HostError> {
    let name = CString::new(name)
        .map_err(|_| HostError::Native("objc class name had interior NUL".to_owned()))?;
    let class = objc_getClass(name.as_ptr());
    if class == NIL {
        return Err(HostError::Native("objc class lookup failed".to_owned()));
    }
    Ok(class)
}

#[cfg(target_os = "macos")]
unsafe fn selector(name: &str) -> Result<ObjcSel, HostError> {
    let name = CString::new(name)
        .map_err(|_| HostError::Native("objc selector had interior NUL".to_owned()))?;
    let sel = sel_registerName(name.as_ptr());
    if sel == NIL {
        return Err(HostError::Native("objc selector lookup failed".to_owned()));
    }
    Ok(sel)
}

#[cfg(target_os = "macos")]
unsafe fn nsstring(value: &str) -> Result<ObjcId, HostError> {
    let string_class = objc_class("NSString")?;
    let alloc = msg_send_id(string_class, selector("alloc")?);
    let utf8 = CString::new(value)
        .map_err(|_| HostError::Native("NSString value had interior NUL".to_owned()))?;
    let string_value = msg_send_id_cstr(alloc, selector("initWithUTF8String:")?, utf8.as_ptr());
    if string_value == NIL {
        return Err(HostError::Native("failed to create NSString".to_owned()));
    }
    Ok(string_value)
}

#[cfg(target_os = "macos")]
unsafe fn create_content_container(frame: NSRect) -> Result<ObjcId, HostError> {
    let view_class = objc_class("NSView")?;
    let view_alloc = msg_send_id(view_class, selector("alloc")?);
    let content_view = msg_send_id_rect(view_alloc, selector("initWithFrame:")?, frame);
    if content_view == NIL {
        return Err(HostError::Native(
            "failed to create content container view".to_owned(),
        ));
    }
    Ok(content_view)
}

#[cfg(target_os = "macos")]
unsafe fn attach_wk_web_view(container: ObjcId, url: &str) -> Result<(), HostError> {
    remove_all_subviews(container)?;
    let (width, height) = current_view_size(container)?;

    let config_class = objc_class("WKWebViewConfiguration")?;
    let config_alloc = msg_send_id(config_class, selector("alloc")?);
    let web_config = msg_send_id(config_alloc, selector("init")?);
    if web_config == NIL {
        return Err(HostError::Native(
            "failed to create WKWebViewConfiguration".to_owned(),
        ));
    }
    let data_store_class = objc_class("WKWebsiteDataStore")?;
    let data_store = msg_send_id(data_store_class, selector("nonPersistentDataStore")?);
    if data_store != NIL {
        msg_send_void_id(web_config, selector("setWebsiteDataStore:")?, data_store);
    }

    let web_view_class = objc_class("WKWebView")?;
    let web_view_alloc = msg_send_id(web_view_class, selector("alloc")?);
    let frame = NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width,
            height,
        },
    };
    let web_view = msg_send_id_rect_id(
        web_view_alloc,
        selector("initWithFrame:configuration:")?,
        frame,
        web_config,
    );
    if web_view == NIL {
        return Err(HostError::Native("failed to create WKWebView".to_owned()));
    }

    msg_send_void_id(container, selector("addSubview:")?, web_view);
    load_url_in_web_view(web_view, url)
}

#[cfg(target_os = "macos")]
unsafe fn remove_all_subviews(view: ObjcId) -> Result<(), HostError> {
    let subviews = msg_send_id(view, selector("subviews")?);
    if subviews == NIL {
        return Ok(());
    }

    loop {
        let count = msg_send_usize(subviews, selector("count")?);
        if count == 0 {
            break;
        }
        let child = msg_send_id_usize(subviews, selector("objectAtIndex:")?, 0);
        if child == NIL {
            break;
        }
        msg_send_void(child, selector("removeFromSuperview")?);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn current_view_size(view: ObjcId) -> Result<(f64, f64), HostError> {
    let bounds = msg_send_rect(view, selector("bounds")?);
    let width = bounds.size.width.max(1.0);
    let height = bounds.size.height.max(1.0);
    Ok((width, height))
}

#[cfg(target_os = "macos")]
unsafe fn navigate_active_cef_browser(url: &str) -> bool {
    let browser = active_content_browser();
    if browser.is_null() {
        return false;
    }
    if let Some(is_valid) = (*browser).is_valid {
        if is_valid(browser) == 0 {
            return false;
        }
    }
    let Some(get_main_frame) = (*browser).get_main_frame else {
        return false;
    };
    let frame = get_main_frame(browser);
    if frame.is_null() {
        return false;
    }
    let Some(load_url) = (*frame).load_url else {
        return false;
    };
    with_stack_cef_string(url, |cef_url| unsafe {
        load_url(frame, cef_url);
    });
    true
}

#[cfg(target_os = "macos")]
unsafe fn load_url_in_web_view(web_view: ObjcId, url: &str) -> Result<(), HostError> {
    let url_string = nsstring(url)?;
    let nsurl_class = objc_class("NSURL")?;
    let nsurl = msg_send_id_id(nsurl_class, selector("URLWithString:")?, url_string);
    if nsurl == NIL {
        return Err(HostError::Native(format!("failed to parse URL: {url}")));
    }

    let request_class = objc_class("NSURLRequest")?;
    let request = msg_send_id_id(request_class, selector("requestWithURL:")?, nsurl);
    if request == NIL {
        return Err(HostError::Native(
            "failed to create NSURLRequest".to_owned(),
        ));
    }

    let _ = msg_send_id_id(web_view, selector("loadRequest:")?, request);
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id(receiver: ObjcId, selector: ObjcSel) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_cstr(receiver: ObjcId, selector: ObjcSel, arg: *const c_char) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, *const c_char) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_rect(receiver: ObjcId, selector: ObjcSel, rect: NSRect) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, NSRect) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, rect)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_rect_id(
    receiver: ObjcId,
    selector: ObjcSel,
    rect: NSRect,
    arg: ObjcId,
) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, NSRect, ObjcId) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, rect, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_rect_u64_u64_bool(
    receiver: ObjcId,
    selector: ObjcSel,
    rect: NSRect,
    style: u64,
    backing: u64,
    should_defer: i8,
) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, NSRect, u64, u64, i8) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, rect, style, backing, should_defer)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_id(receiver: ObjcId, selector: ObjcSel, arg: ObjcId) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, ObjcId) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_usize(receiver: ObjcId, selector: ObjcSel, arg: usize) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, usize) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_rect(receiver: ObjcId, selector: ObjcSel) -> NSRect {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) -> NSRect =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_usize(receiver: ObjcId, selector: ObjcSel) -> usize {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) -> usize =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void(receiver: ObjcId, selector: ObjcSel) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_id(receiver: ObjcId, selector: ObjcSel, arg: ObjcId) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, ObjcId) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_bool(receiver: ObjcId, selector: ObjcSel, arg: i8) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, i8) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_i64(receiver: ObjcId, selector: ObjcSel, arg: i64) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, i64) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_u64(receiver: ObjcId, selector: ObjcSel, arg: u64) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, u64) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Default)]
pub struct NativeMacHost;

#[cfg(not(target_os = "macos"))]
impl NativeMacHost {
    pub fn new() -> Result<Self, HostError> {
        Err(HostError::UnsupportedPlatform)
    }
}

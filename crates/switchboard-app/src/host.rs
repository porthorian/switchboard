use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use switchboard_core::TabId;

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

    fn run_event_loop(self) -> Result<(), Self::Error>
    where
        Self: Sized;
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

    fn run_event_loop(self) -> Result<(), Self::Error> {
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
use switchboard_cef_sys::loader::CefLibrary;
#[cfg(target_os = "macos")]
use switchboard_cef_sys::raw::{
    cef_base_ref_counted_t, cef_browser_host_create_browser_fn, cef_browser_settings_t,
    cef_client_t, cef_main_args_t, cef_rect_t, cef_settings_t, cef_string_t, cef_string_utf16_t,
    cef_window_info_t, CEF_RUNTIME_STYLE_ALLOY,
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
const UI_WIDTH: f64 = 320.0;
#[cfg(target_os = "macos")]
const CONTENT_WIDTH: f64 = WINDOW_WIDTH - UI_WIDTH;
#[cfg(target_os = "macos")]
const CONTENT_HEIGHT: f64 = WINDOW_HEIGHT;
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
const BACKING_STORE_BUFFERED: u64 = 2;
#[cfg(target_os = "macos")]
const APP_ACTIVATION_POLICY_REGULAR: i64 = 0;

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
#[derive(Debug, Clone, Copy)]
enum ContentBackend {
    WebKit(ObjcId),
    Cef(ObjcId),
}

#[cfg(target_os = "macos")]
struct CefRuntime {
    library: CefLibrary,
    config: CefConfig,
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
unsafe extern "C" fn cef_client_add_ref_noop(_self_: *mut cef_base_ref_counted_t) {}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_client_release_noop(_self_: *mut cef_base_ref_counted_t) -> c_int {
    0
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_client_has_one_ref_true(_self_: *mut cef_base_ref_counted_t) -> c_int {
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_client_has_at_least_one_ref_true(
    _self_: *mut cef_base_ref_counted_t,
) -> c_int {
    1
}

#[cfg(target_os = "macos")]
fn allocate_minimal_cef_client() -> *mut cef_client_t {
    let client = Box::new(cef_client_t {
        base: cef_base_ref_counted_t {
            size: size_of::<cef_client_t>(),
            add_ref: Some(cef_client_add_ref_noop),
            release: Some(cef_client_release_noop),
            has_one_ref: Some(cef_client_has_one_ref_true),
            has_at_least_one_ref: Some(cef_client_has_at_least_one_ref_true),
        },
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
        get_jsdialog_handler: None,
        get_keyboard_handler: None,
        get_life_span_handler: None,
        get_load_handler: None,
        get_print_handler: None,
        get_render_handler: None,
        get_request_handler: None,
        on_process_message_received: None,
    });
    let client_ptr = Box::into_raw(client);
    if env_flag(ENV_CEF_VERBOSE_ERRORS) {
        unsafe {
            eprintln!(
                "switchboard-app: allocated minimal CEF client ptr={client_ptr:p} base.size={}",
                (*client_ptr).base.size
            );
        }
    }
    client_ptr
}

#[cfg(target_os = "macos")]
unsafe fn free_minimal_cef_client(client: *mut cef_client_t) {
    if client.is_null() {
        return;
    }
    drop(Box::from_raw(client));
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

            let mut argv_storage = Vec::new();
            for arg in std::env::args() {
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

            let secondary_exit_code = (library.api.cef_execute_process)(
                &main_args,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
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

            let initialized = (library.api.cef_initialize)(
                &main_args,
                &settings,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            drop(settings_strings);
            if initialized == 0 {
                return Err(HostError::Native(format!(
                    "CEF bootstrap failed at: initialize\nreason: cef_initialize returned false\nconfiguration:\n{config_summary}\nnotes:\n  - if logs mention ProcessSingleton/SingletonSocket, ensure tmp/cache paths are writable\n  - override with SWITCHBOARD_CEF_ROOT_CACHE_PATH and SWITCHBOARD_CEF_TMPDIR if needed"
                )));
            }

            Ok(Some(Self { library, config }))
        }
    }

    fn create_browser_in_view(
        &self,
        parent_view: ObjcId,
        url: &str,
        client: *mut cef_client_t,
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
                width: CONTENT_WIDTH as c_int,
                height: CONTENT_HEIGHT as c_int,
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

    fn run_message_loop(&self) {
        unsafe {
            (self.library.api.cef_run_message_loop)();
        }
    }

    fn shutdown(&self) {
        unsafe {
            (self.library.api.cef_shutdown)();
        }
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

            let app_class = objc_class("NSApplication")?;
            let app = msg_send_id(app_class, selector("sharedApplication")?);
            if app == NIL {
                return Err(HostError::Native(
                    "NSApplication sharedApplication returned nil".to_owned(),
                ));
            }

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
            let style = STYLE_TITLED | STYLE_CLOSABLE | STYLE_MINIATURIZABLE | STYLE_RESIZABLE;
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

        unsafe {
            let window = self.window_for(window_id)?;
            let root_view = msg_send_id(window, selector("contentView")?);
            if root_view == NIL {
                return Err(HostError::Native("window content view is nil".to_owned()));
            }

            let view_class = objc_class("NSView")?;
            let view_alloc = msg_send_id(view_class, selector("alloc")?);
            let frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: UI_WIDTH,
                    height: WINDOW_HEIGHT,
                },
            };
            let ui_view = msg_send_id_rect(view_alloc, selector("initWithFrame:")?, frame);
            if ui_view == NIL {
                return Err(HostError::Native("failed to create UI view".to_owned()));
            }

            msg_send_void_id(root_view, selector("addSubview:")?, ui_view);

            self.next_ui_view_id += 1;
            let view_id = UiViewId(self.next_ui_view_id);
            self.ui_views.insert(view_id, ui_view);
            Ok(view_id)
        }
    }

    fn create_content_view(
        &mut self,
        window_id: WindowId,
        _tab_id: TabId,
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
            let frame = NSRect {
                origin: NSPoint {
                    x: UI_WIDTH,
                    y: 0.0,
                },
                size: NSSize {
                    width: CONTENT_WIDTH,
                    height: CONTENT_HEIGHT,
                },
            };
            let content_view = create_content_container(frame)?;
            msg_send_void_id(root_view, selector("addSubview:")?, content_view);

            let mut cef_client: Option<*mut cef_client_t> = None;
            let backend = if let Some(cef) = self.cef.as_ref() {
                let client = allocate_minimal_cef_client();
                if let Err(error) = cef.create_browser_in_view(content_view, url, client) {
                    free_minimal_cef_client(client);
                    return Err(error);
                }
                cef_client = Some(client);
                ContentBackend::Cef(content_view)
            } else {
                attach_wk_web_view(content_view, url)?;
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
        _tab_id: TabId,
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
                }
                ContentBackend::Cef(view) => {
                    let cef = self
                        .cef
                        .as_ref()
                        .ok_or_else(|| HostError::Native("CEF runtime unavailable".to_owned()))?;
                    let client = self.cef_clients.get(&view_id).copied().ok_or_else(|| {
                        HostError::Native(format!(
                            "CEF client missing for content view: {}",
                            view_id.0
                        ))
                    })?;
                    cef.create_browser_in_view(view, url, client)?;
                }
            }
            let title_value = nsstring(&format!("Switchboard - {url}"))?;
            msg_send_void_id(window, selector("setTitle:")?, title_value);
        }
        Ok(())
    }

    fn run_event_loop(mut self) -> Result<(), Self::Error> {
        unsafe {
            msg_send_void_bool(self.app, selector("activateIgnoringOtherApps:")?, YES);
            if let Some(cef) = self.cef.take() {
                cef.run_message_loop();
                for client in self.cef_clients.values().copied() {
                    free_minimal_cef_client(client);
                }
                cef.shutdown();
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
            width: CONTENT_WIDTH,
            height: CONTENT_HEIGHT,
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

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Default)]
pub struct NativeMacHost;

#[cfg(not(target_os = "macos"))]
impl NativeMacHost {
    pub fn new() -> Result<Self, HostError> {
        Err(HostError::UnsupportedPlatform)
    }
}

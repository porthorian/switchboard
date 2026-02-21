use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_uint};

pub type cef_char16_t = u16;
pub type cef_color_t = u32;
pub type cef_log_severity_t = c_uint;
pub type cef_log_items_t = c_uint;
pub type cef_state_t = c_uint;
pub type cef_runtime_style_t = c_uint;
pub type cef_process_id_t = c_uint;
pub type cef_window_handle_t = *mut c_void;

pub const CEF_RUNTIME_STYLE_DEFAULT: cef_runtime_style_t = 0;
pub const CEF_RUNTIME_STYLE_CHROME: cef_runtime_style_t = 1;
pub const CEF_RUNTIME_STYLE_ALLOY: cef_runtime_style_t = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_string_utf16_t {
    pub str_: *mut cef_char16_t,
    pub length: usize,
    pub dtor: Option<unsafe extern "C" fn(str_: *mut cef_char16_t)>,
}

pub type cef_string_t = cef_string_utf16_t;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_rect_t {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_base_ref_counted_t {
    pub size: usize,
    pub add_ref: Option<unsafe extern "C" fn(self_: *mut cef_base_ref_counted_t)>,
    pub release: Option<unsafe extern "C" fn(self_: *mut cef_base_ref_counted_t) -> c_int>,
    pub has_one_ref: Option<unsafe extern "C" fn(self_: *mut cef_base_ref_counted_t) -> c_int>,
    pub has_at_least_one_ref:
        Option<unsafe extern "C" fn(self_: *mut cef_base_ref_counted_t) -> c_int>,
}

#[repr(C)]
pub struct cef_resource_bundle_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_browser_process_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_render_process_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_audio_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_command_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_context_menu_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_dialog_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_display_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_download_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_drag_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_find_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_focus_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_frame_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_permission_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_jsdialog_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_keyboard_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_life_span_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_load_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_print_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_render_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_request_handler_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_browser_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_frame_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_process_message_t {
    pub _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_window_info_t {
    pub size: usize,
    pub window_name: cef_string_t,
    pub bounds: cef_rect_t,
    pub hidden: c_int,
    pub parent_view: cef_window_handle_t,
    pub windowless_rendering_enabled: c_int,
    pub shared_texture_enabled: c_int,
    pub external_begin_frame_enabled: c_int,
    pub view: cef_window_handle_t,
    pub runtime_style: cef_runtime_style_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_client_t {
    pub base: cef_base_ref_counted_t,
    pub get_audio_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_audio_handler_t>,
    pub get_command_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_command_handler_t>,
    pub get_context_menu_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_context_menu_handler_t>,
    pub get_dialog_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_dialog_handler_t>,
    pub get_display_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_display_handler_t>,
    pub get_download_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_download_handler_t>,
    pub get_drag_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_drag_handler_t>,
    pub get_find_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_find_handler_t>,
    pub get_focus_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_focus_handler_t>,
    pub get_frame_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_frame_handler_t>,
    pub get_permission_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_permission_handler_t>,
    pub get_jsdialog_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_jsdialog_handler_t>,
    pub get_keyboard_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_keyboard_handler_t>,
    pub get_life_span_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_life_span_handler_t>,
    pub get_load_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_load_handler_t>,
    pub get_print_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_print_handler_t>,
    pub get_render_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_render_handler_t>,
    pub get_request_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_client_t) -> *mut cef_request_handler_t>,
    pub on_process_message_received: Option<
        unsafe extern "C" fn(
            self_: *mut cef_client_t,
            browser: *mut cef_browser_t,
            frame: *mut cef_frame_t,
            source_process: cef_process_id_t,
            message: *mut cef_process_message_t,
        ) -> c_int,
    >,
}

#[repr(C)]
pub struct cef_dictionary_value_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_request_context_t {
    pub _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_settings_t {
    pub size: usize,
    pub no_sandbox: c_int,
    pub browser_subprocess_path: cef_string_t,
    pub framework_dir_path: cef_string_t,
    pub main_bundle_path: cef_string_t,
    pub multi_threaded_message_loop: c_int,
    pub external_message_pump: c_int,
    pub windowless_rendering_enabled: c_int,
    pub command_line_args_disabled: c_int,
    pub cache_path: cef_string_t,
    pub root_cache_path: cef_string_t,
    pub persist_session_cookies: c_int,
    pub user_agent: cef_string_t,
    pub user_agent_product: cef_string_t,
    pub locale: cef_string_t,
    pub log_file: cef_string_t,
    pub log_severity: cef_log_severity_t,
    pub log_items: cef_log_items_t,
    pub javascript_flags: cef_string_t,
    pub resources_dir_path: cef_string_t,
    pub locales_dir_path: cef_string_t,
    pub remote_debugging_port: c_int,
    pub uncaught_exception_stack_size: c_int,
    pub background_color: cef_color_t,
    pub accept_language_list: cef_string_t,
    pub cookieable_schemes_list: cef_string_t,
    pub cookieable_schemes_exclude_defaults: c_int,
    pub chrome_policy_id: cef_string_t,
    pub chrome_app_icon_id: c_int,
    pub disable_signal_handlers: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_browser_settings_t {
    pub size: usize,
    pub windowless_frame_rate: c_int,
    pub standard_font_family: cef_string_t,
    pub fixed_font_family: cef_string_t,
    pub serif_font_family: cef_string_t,
    pub sans_serif_font_family: cef_string_t,
    pub cursive_font_family: cef_string_t,
    pub fantasy_font_family: cef_string_t,
    pub default_font_size: c_int,
    pub default_fixed_font_size: c_int,
    pub minimum_font_size: c_int,
    pub minimum_logical_font_size: c_int,
    pub default_encoding: cef_string_t,
    pub remote_fonts: cef_state_t,
    pub javascript: cef_state_t,
    pub javascript_close_windows: cef_state_t,
    pub javascript_access_clipboard: cef_state_t,
    pub javascript_dom_paste: cef_state_t,
    pub image_loading: cef_state_t,
    pub image_shrink_standalone_to_fit: cef_state_t,
    pub text_area_resize: cef_state_t,
    pub tab_to_links: cef_state_t,
    pub local_storage: cef_state_t,
    pub databases_deprecated: cef_state_t,
    pub webgl: cef_state_t,
    pub background_color: cef_color_t,
    pub chrome_status_bubble: cef_state_t,
    pub chrome_zoom_bubble: cef_state_t,
}

#[repr(C)]
pub struct cef_task_t {
    pub base: cef_base_ref_counted_t,
    pub execute: Option<unsafe extern "C" fn(self_: *mut cef_task_t)>,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum cef_thread_id_t {
    TID_UI = 0,
    TID_FILE_BACKGROUND = 1,
    TID_FILE_USER_VISIBLE = 2,
    TID_FILE_USER_BLOCKING = 3,
    TID_PROCESS_LAUNCHER = 4,
    TID_IO = 5,
    TID_RENDERER = 6,
}

#[repr(C)]
pub struct cef_app_t {
    pub base: cef_base_ref_counted_t,
    pub on_before_command_line_processing: Option<
        unsafe extern "C" fn(
            self_: *mut cef_app_t,
            process_type: *const cef_string_t,
            command_line: *mut c_void,
        ),
    >,
    pub on_register_custom_schemes:
        Option<unsafe extern "C" fn(self_: *mut cef_app_t, registrar: *mut c_void)>,
    pub get_resource_bundle_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_app_t) -> *mut cef_resource_bundle_handler_t>,
    pub get_browser_process_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_app_t) -> *mut cef_browser_process_handler_t>,
    pub get_render_process_handler:
        Option<unsafe extern "C" fn(self_: *mut cef_app_t) -> *mut cef_render_process_handler_t>,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_main_args_t {
    pub instance: *mut c_void,
}

#[cfg(not(target_os = "windows"))]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct cef_main_args_t {
    pub argc: c_int,
    pub argv: *mut *mut c_char,
}

pub type cef_execute_process_fn = unsafe extern "C" fn(
    args: *const cef_main_args_t,
    application: *mut cef_app_t,
    windows_sandbox_info: *mut c_void,
) -> c_int;

pub type cef_api_hash_fn = unsafe extern "C" fn(version: c_int, entry: c_int) -> *const c_char;
pub type cef_api_version_fn = unsafe extern "C" fn() -> c_int;

pub type cef_initialize_fn = unsafe extern "C" fn(
    args: *const cef_main_args_t,
    settings: *const cef_settings_t,
    application: *mut cef_app_t,
    windows_sandbox_info: *mut c_void,
) -> c_int;

pub type cef_shutdown_fn = unsafe extern "C" fn();
pub type cef_run_message_loop_fn = unsafe extern "C" fn();
pub type cef_do_message_loop_work_fn = unsafe extern "C" fn();
pub type cef_quit_message_loop_fn = unsafe extern "C" fn();
pub type cef_currently_on_fn = unsafe extern "C" fn(thread_id: cef_thread_id_t) -> c_int;
pub type cef_post_task_fn =
    unsafe extern "C" fn(thread_id: cef_thread_id_t, task: *mut cef_task_t) -> c_int;
pub type cef_string_utf16_set_fn = unsafe extern "C" fn(
    src: *const cef_char16_t,
    src_len: usize,
    output: *mut cef_string_utf16_t,
    copy: c_int,
) -> c_int;
pub type cef_string_utf16_clear_fn = unsafe extern "C" fn(str_: *mut cef_string_utf16_t);

pub type cef_browser_host_create_browser_fn = unsafe extern "C" fn(
    window_info: *const cef_window_info_t,
    client: *mut cef_client_t,
    url: *const cef_string_t,
    settings: *const cef_browser_settings_t,
    extra_info: *mut cef_dictionary_value_t,
    request_context: *mut cef_request_context_t,
) -> c_int;

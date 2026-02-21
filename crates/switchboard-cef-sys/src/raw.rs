use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_uint};

pub type cef_char16_t = u16;
pub type cef_color_t = u32;
pub type cef_log_severity_t = c_uint;
pub type cef_log_items_t = c_uint;
pub type cef_state_t = c_uint;
pub type cef_runtime_style_t = c_uint;
pub type cef_process_id_t = c_uint;
pub type cef_errorcode_t = c_int;
pub type cef_jsdialog_type_t = c_uint;
pub type cef_scheme_options_t = c_uint;
pub type cef_string_userfree_t = *mut cef_string_t;
pub type cef_string_list_t = *mut c_void;
pub type cef_string_multimap_t = *mut c_void;
pub type cef_window_handle_t = *mut c_void;
pub type cef_cursor_handle_t = *mut c_void;
pub type cef_cursor_type_t = c_uint;

pub const CEF_RUNTIME_STYLE_DEFAULT: cef_runtime_style_t = 0;
pub const CEF_RUNTIME_STYLE_CHROME: cef_runtime_style_t = 1;
pub const CEF_RUNTIME_STYLE_ALLOY: cef_runtime_style_t = 2;
pub const JSDIALOGTYPE_ALERT: cef_jsdialog_type_t = 0;
pub const JSDIALOGTYPE_CONFIRM: cef_jsdialog_type_t = 1;
pub const JSDIALOGTYPE_PROMPT: cef_jsdialog_type_t = 2;
pub const CEF_SCHEME_OPTION_NONE: cef_scheme_options_t = 0;
pub const CEF_SCHEME_OPTION_STANDARD: cef_scheme_options_t = 1;
pub const CEF_SCHEME_OPTION_LOCAL: cef_scheme_options_t = 2;
pub const CEF_SCHEME_OPTION_DISPLAY_ISOLATED: cef_scheme_options_t = 4;
pub const CEF_SCHEME_OPTION_SECURE: cef_scheme_options_t = 8;
pub const CEF_SCHEME_OPTION_CORS_ENABLED: cef_scheme_options_t = 16;
pub const CEF_SCHEME_OPTION_CSP_BYPASSING: cef_scheme_options_t = 32;
pub const CEF_SCHEME_OPTION_FETCH_ENABLED: cef_scheme_options_t = 64;

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
pub struct cef_size_t {
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
#[derive(Clone, Copy)]
pub struct cef_base_scoped_t {
    pub size: usize,
    pub del: Option<unsafe extern "C" fn(self_: *mut cef_base_scoped_t)>,
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
pub struct cef_cursor_info_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_display_handler_t {
    pub base: cef_base_ref_counted_t,
    pub on_address_change: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            frame: *mut cef_frame_t,
            url: *const cef_string_t,
        ),
    >,
    pub on_title_change: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            title: *const cef_string_t,
        ),
    >,
    pub on_favicon_urlchange: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            icon_urls: cef_string_list_t,
        ),
    >,
    pub on_fullscreen_mode_change: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            fullscreen: c_int,
        ),
    >,
    pub on_tooltip: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            text: *mut cef_string_t,
        ) -> c_int,
    >,
    pub on_status_message: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            value: *const cef_string_t,
        ),
    >,
    pub on_console_message: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            level: cef_log_severity_t,
            message: *const cef_string_t,
            source: *const cef_string_t,
            line: c_int,
        ) -> c_int,
    >,
    pub on_auto_resize: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            new_size: *const cef_size_t,
        ) -> c_int,
    >,
    pub on_loading_progress_change: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            progress: f64,
        ),
    >,
    pub on_cursor_change: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            cursor: cef_cursor_handle_t,
            type_: cef_cursor_type_t,
            custom_cursor_info: *const cef_cursor_info_t,
        ) -> c_int,
    >,
    pub on_media_access_change: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            has_video_access: c_int,
            has_audio_access: c_int,
        ),
    >,
    pub on_contents_bounds_change: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            new_bounds: *const cef_rect_t,
        ) -> c_int,
    >,
    pub get_root_window_screen_rect: Option<
        unsafe extern "C" fn(
            self_: *mut cef_display_handler_t,
            browser: *mut cef_browser_t,
            rect: *mut cef_rect_t,
        ) -> c_int,
    >,
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
pub struct cef_jsdialog_callback_t {
    pub base: cef_base_ref_counted_t,
    pub cont: Option<
        unsafe extern "C" fn(
            self_: *mut cef_jsdialog_callback_t,
            success: c_int,
            user_input: *const cef_string_t,
        ),
    >,
}

#[repr(C)]
pub struct cef_jsdialog_handler_t {
    pub base: cef_base_ref_counted_t,
    pub on_jsdialog: Option<
        unsafe extern "C" fn(
            self_: *mut cef_jsdialog_handler_t,
            browser: *mut cef_browser_t,
            origin_url: *const cef_string_t,
            dialog_type: cef_jsdialog_type_t,
            message_text: *const cef_string_t,
            default_prompt_text: *const cef_string_t,
            callback: *mut cef_jsdialog_callback_t,
            suppress_message: *mut c_int,
        ) -> c_int,
    >,
    pub on_before_unload_dialog: Option<
        unsafe extern "C" fn(
            self_: *mut cef_jsdialog_handler_t,
            browser: *mut cef_browser_t,
            message_text: *const cef_string_t,
            is_reload: c_int,
            callback: *mut cef_jsdialog_callback_t,
        ) -> c_int,
    >,
    pub on_reset_dialog_state: Option<
        unsafe extern "C" fn(self_: *mut cef_jsdialog_handler_t, browser: *mut cef_browser_t),
    >,
    pub on_dialog_closed: Option<
        unsafe extern "C" fn(self_: *mut cef_jsdialog_handler_t, browser: *mut cef_browser_t),
    >,
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
pub struct cef_browser_host_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_browser_t {
    pub base: cef_base_ref_counted_t,
    pub is_valid: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> c_int>,
    pub get_host:
        Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> *mut cef_browser_host_t>,
    pub can_go_back: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> c_int>,
    pub go_back: Option<unsafe extern "C" fn(self_: *mut cef_browser_t)>,
    pub can_go_forward: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> c_int>,
    pub go_forward: Option<unsafe extern "C" fn(self_: *mut cef_browser_t)>,
    pub is_loading: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> c_int>,
    pub reload: Option<unsafe extern "C" fn(self_: *mut cef_browser_t)>,
    pub reload_ignore_cache: Option<unsafe extern "C" fn(self_: *mut cef_browser_t)>,
    pub stop_load: Option<unsafe extern "C" fn(self_: *mut cef_browser_t)>,
    pub get_identifier: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> c_int>,
    pub is_same:
        Option<unsafe extern "C" fn(self_: *mut cef_browser_t, that: *mut cef_browser_t) -> c_int>,
    pub is_popup: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> c_int>,
    pub has_document: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> c_int>,
    pub get_main_frame: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> *mut cef_frame_t>,
    pub get_focused_frame:
        Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> *mut cef_frame_t>,
    pub get_frame_by_identifier: Option<
        unsafe extern "C" fn(
            self_: *mut cef_browser_t,
            identifier: *const cef_string_t,
        ) -> *mut cef_frame_t,
    >,
    pub get_frame_by_name: Option<
        unsafe extern "C" fn(
            self_: *mut cef_browser_t,
            name: *const cef_string_t,
        ) -> *mut cef_frame_t,
    >,
    pub get_frame_count: Option<unsafe extern "C" fn(self_: *mut cef_browser_t) -> usize>,
    pub get_frame_identifiers:
        Option<unsafe extern "C" fn(self_: *mut cef_browser_t, identifiers: cef_string_list_t)>,
    pub get_frame_names:
        Option<unsafe extern "C" fn(self_: *mut cef_browser_t, names: cef_string_list_t)>,
}

#[repr(C)]
pub struct cef_frame_t {
    pub base: cef_base_ref_counted_t,
    pub is_valid: Option<unsafe extern "C" fn(self_: *mut cef_frame_t) -> c_int>,
    pub undo: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub redo: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub cut: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub copy: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub paste: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub paste_and_match_style: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub del: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub select_all: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub view_source: Option<unsafe extern "C" fn(self_: *mut cef_frame_t)>,
    pub get_source: Option<
        unsafe extern "C" fn(self_: *mut cef_frame_t, visitor: *mut c_void),
    >,
    pub get_text: Option<unsafe extern "C" fn(self_: *mut cef_frame_t, visitor: *mut c_void)>,
    pub load_request:
        Option<unsafe extern "C" fn(self_: *mut cef_frame_t, request: *mut cef_request_t)>,
    pub load_url:
        Option<unsafe extern "C" fn(self_: *mut cef_frame_t, url: *const cef_string_t)>,
    pub execute_java_script: Option<
        unsafe extern "C" fn(
            self_: *mut cef_frame_t,
            code: *const cef_string_t,
            script_url: *const cef_string_t,
            start_line: c_int,
        ),
    >,
    pub is_main: Option<unsafe extern "C" fn(self_: *mut cef_frame_t) -> c_int>,
    pub is_focused: Option<unsafe extern "C" fn(self_: *mut cef_frame_t) -> c_int>,
}

#[repr(C)]
pub struct cef_process_message_t {
    pub _private: [u8; 0],
}

#[repr(C)]
pub struct cef_request_t {
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
pub struct cef_callback_t {
    pub base: cef_base_ref_counted_t,
    pub cont: Option<unsafe extern "C" fn(self_: *mut cef_callback_t)>,
    pub cancel: Option<unsafe extern "C" fn(self_: *mut cef_callback_t)>,
}

#[repr(C)]
pub struct cef_response_t {
    pub base: cef_base_ref_counted_t,
    pub is_read_only: Option<unsafe extern "C" fn(self_: *mut cef_response_t) -> c_int>,
    pub get_error: Option<unsafe extern "C" fn(self_: *mut cef_response_t) -> cef_errorcode_t>,
    pub set_error: Option<unsafe extern "C" fn(self_: *mut cef_response_t, error: cef_errorcode_t)>,
    pub get_status: Option<unsafe extern "C" fn(self_: *mut cef_response_t) -> c_int>,
    pub set_status: Option<unsafe extern "C" fn(self_: *mut cef_response_t, status: c_int)>,
    pub get_status_text:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t) -> cef_string_userfree_t>,
    pub set_status_text:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t, status_text: *const cef_string_t)>,
    pub get_mime_type:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t) -> cef_string_userfree_t>,
    pub set_mime_type:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t, mime_type: *const cef_string_t)>,
    pub get_charset:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t) -> cef_string_userfree_t>,
    pub set_charset:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t, charset: *const cef_string_t)>,
    pub get_header_by_name: Option<
        unsafe extern "C" fn(
            self_: *mut cef_response_t,
            name: *const cef_string_t,
        ) -> cef_string_userfree_t,
    >,
    pub set_header_by_name: Option<
        unsafe extern "C" fn(
            self_: *mut cef_response_t,
            name: *const cef_string_t,
            value: *const cef_string_t,
            overwrite: c_int,
        ),
    >,
    pub get_header_map:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t, header_map: cef_string_multimap_t)>,
    pub set_header_map:
        Option<unsafe extern "C" fn(self_: *mut cef_response_t, header_map: cef_string_multimap_t)>,
    pub get_url: Option<unsafe extern "C" fn(self_: *mut cef_response_t) -> cef_string_userfree_t>,
    pub set_url: Option<unsafe extern "C" fn(self_: *mut cef_response_t, url: *const cef_string_t)>,
}

#[repr(C)]
pub struct cef_resource_skip_callback_t {
    pub base: cef_base_ref_counted_t,
    pub cont:
        Option<unsafe extern "C" fn(self_: *mut cef_resource_skip_callback_t, bytes_skipped: i64)>,
}

#[repr(C)]
pub struct cef_resource_read_callback_t {
    pub base: cef_base_ref_counted_t,
    pub cont:
        Option<unsafe extern "C" fn(self_: *mut cef_resource_read_callback_t, bytes_read: c_int)>,
}

#[repr(C)]
pub struct cef_resource_handler_t {
    pub base: cef_base_ref_counted_t,
    pub open: Option<
        unsafe extern "C" fn(
            self_: *mut cef_resource_handler_t,
            request: *mut cef_request_t,
            handle_request: *mut c_int,
            callback: *mut cef_callback_t,
        ) -> c_int,
    >,
    pub process_request: Option<
        unsafe extern "C" fn(
            self_: *mut cef_resource_handler_t,
            request: *mut cef_request_t,
            callback: *mut cef_callback_t,
        ) -> c_int,
    >,
    pub get_response_headers: Option<
        unsafe extern "C" fn(
            self_: *mut cef_resource_handler_t,
            response: *mut cef_response_t,
            response_length: *mut i64,
            redirect_url: *mut cef_string_t,
        ),
    >,
    pub skip: Option<
        unsafe extern "C" fn(
            self_: *mut cef_resource_handler_t,
            bytes_to_skip: i64,
            bytes_skipped: *mut i64,
            callback: *mut cef_resource_skip_callback_t,
        ) -> c_int,
    >,
    pub read: Option<
        unsafe extern "C" fn(
            self_: *mut cef_resource_handler_t,
            data_out: *mut c_void,
            bytes_to_read: c_int,
            bytes_read: *mut c_int,
            callback: *mut cef_resource_read_callback_t,
        ) -> c_int,
    >,
    pub read_response: Option<
        unsafe extern "C" fn(
            self_: *mut cef_resource_handler_t,
            data_out: *mut c_void,
            bytes_to_read: c_int,
            bytes_read: *mut c_int,
            callback: *mut cef_callback_t,
        ) -> c_int,
    >,
    pub cancel: Option<unsafe extern "C" fn(self_: *mut cef_resource_handler_t)>,
}

#[repr(C)]
pub struct cef_scheme_registrar_t {
    pub base: cef_base_scoped_t,
    pub add_custom_scheme: Option<
        unsafe extern "C" fn(
            self_: *mut cef_scheme_registrar_t,
            scheme_name: *const cef_string_t,
            options: c_int,
        ) -> c_int,
    >,
}

#[repr(C)]
pub struct cef_scheme_handler_factory_t {
    pub base: cef_base_ref_counted_t,
    pub create: Option<
        unsafe extern "C" fn(
            self_: *mut cef_scheme_handler_factory_t,
            browser: *mut cef_browser_t,
            frame: *mut cef_frame_t,
            scheme_name: *const cef_string_t,
            request: *mut cef_request_t,
        ) -> *mut cef_resource_handler_t,
    >,
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
        Option<unsafe extern "C" fn(self_: *mut cef_app_t, registrar: *mut cef_scheme_registrar_t)>,
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

pub type cef_register_scheme_handler_factory_fn = unsafe extern "C" fn(
    scheme_name: *const cef_string_t,
    domain_name: *const cef_string_t,
    factory: *mut cef_scheme_handler_factory_t,
) -> c_int;

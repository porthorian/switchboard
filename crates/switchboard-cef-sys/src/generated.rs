#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

#[path = "generated_load_handler_145.rs"]
mod load_handler;
#[path = "generated_request_context_145.rs"]
mod request_context;

pub use load_handler::cef_load_handler_t;
pub use request_context::cef_request_context_settings_t;

pub const CEF_VERSION: &str = "145.0.26+g6ed7554+chromium-145.0.7632.110";
pub const CEF_API_VERSION: i32 = 14500;

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::{cef_load_handler_t, cef_request_context_settings_t};
    use std::mem::{offset_of, size_of};

    #[test]
    fn pinned_load_handler_layout_matches_cef_145() {
        assert_eq!(size_of::<cef_load_handler_t>(), 72);
        assert_eq!(offset_of!(cef_load_handler_t, on_loading_state_change), 40);
        assert_eq!(offset_of!(cef_load_handler_t, on_load_start), 48);
        assert_eq!(offset_of!(cef_load_handler_t, on_load_end), 56);
        assert_eq!(offset_of!(cef_load_handler_t, on_load_error), 64);
    }

    #[test]
    fn pinned_request_context_layout_matches_cef_145() {
        assert_eq!(size_of::<cef_request_context_settings_t>(), 96);
        assert_eq!(offset_of!(cef_request_context_settings_t, cache_path), 8);
        assert_eq!(
            offset_of!(cef_request_context_settings_t, persist_session_cookies),
            32
        );
        assert_eq!(
            offset_of!(cef_request_context_settings_t, accept_language_list),
            40
        );
        assert_eq!(
            offset_of!(cef_request_context_settings_t, cookieable_schemes_list),
            64
        );
        assert_eq!(
            offset_of!(
                cef_request_context_settings_t,
                cookieable_schemes_exclude_defaults
            ),
            88
        );
    }
}

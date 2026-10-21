//! CEF 145 / API 14500 image structs, pinned to the bundled C API headers.
//! The request prefix is used only to read URLs/methods; CEF owns the allocation.
use crate::raw::{cef_base_ref_counted_t, cef_request_t, cef_string_t, cef_string_userfree_t};
use std::ffi::{c_int, c_void};

#[repr(C)]
pub struct cef_download_image_callback_t {
    pub base: cef_base_ref_counted_t,
    pub on_download_image_finished: Option<unsafe extern "C" fn(*mut Self, *const cef_string_t, c_int, *mut cef_image_t)>,
}
#[repr(C)]
pub struct cef_binary_value_t {
    pub base: cef_base_ref_counted_t,
    pub is_valid: Option<unsafe extern "C" fn(*mut Self) -> c_int>,
    pub is_owned: Option<unsafe extern "C" fn(*mut Self) -> c_int>,
    pub is_same: Option<unsafe extern "C" fn(*mut Self, *mut Self) -> c_int>,
    pub is_equal: Option<unsafe extern "C" fn(*mut Self, *mut Self) -> c_int>,
    pub copy: Option<unsafe extern "C" fn(*mut Self) -> *mut Self>,
    pub get_raw_data: Option<unsafe extern "C" fn(*mut Self) -> *const c_void>,
    pub get_size: Option<unsafe extern "C" fn(*mut Self) -> usize>,
    pub get_data: Option<unsafe extern "C" fn(*mut Self, *mut c_void, usize, usize) -> usize>,
}
#[repr(C)]
pub struct cef_image_t {
    pub base: cef_base_ref_counted_t,
    pub is_empty: Option<unsafe extern "C" fn(*mut Self) -> c_int>,
    pub is_same: Option<unsafe extern "C" fn(*mut Self, *mut Self) -> c_int>,
    pub add_bitmap: Option<unsafe extern "C" fn(*mut Self, f32, c_int, c_int, u32, u32, *const c_void, usize) -> c_int>,
    pub add_png: Option<unsafe extern "C" fn(*mut Self, f32, *const c_void, usize) -> c_int>,
    pub add_jpeg: Option<unsafe extern "C" fn(*mut Self, f32, *const c_void, usize) -> c_int>,
    pub get_width: Option<unsafe extern "C" fn(*mut Self) -> usize>,
    pub get_height: Option<unsafe extern "C" fn(*mut Self) -> usize>,
    pub has_representation: Option<unsafe extern "C" fn(*mut Self, f32) -> c_int>,
    pub remove_representation: Option<unsafe extern "C" fn(*mut Self, f32) -> c_int>,
    pub get_representation_info: Option<unsafe extern "C" fn(*mut Self, f32, *mut f32, *mut c_int, *mut c_int) -> c_int>,
    pub get_as_bitmap: Option<unsafe extern "C" fn(*mut Self, f32, u32, u32, *mut c_int, *mut c_int) -> *mut cef_binary_value_t>,
    pub get_as_png: Option<unsafe extern "C" fn(*mut Self, f32, c_int, *mut c_int, *mut c_int) -> *mut cef_binary_value_t>,
    pub get_as_jpeg: Option<unsafe extern "C" fn(*mut Self, f32, c_int, *mut c_int, *mut c_int) -> *mut cef_binary_value_t>,
}
#[repr(C)]
pub struct CefRequestPrefix {
    pub base: cef_base_ref_counted_t,
    pub is_read_only: Option<unsafe extern "C" fn(*mut cef_request_t) -> c_int>,
    pub get_url: Option<unsafe extern "C" fn(*mut cef_request_t) -> cef_string_userfree_t>,
    pub set_url: Option<unsafe extern "C" fn(*mut cef_request_t, *const cef_string_t)>,
    pub get_method: Option<unsafe extern "C" fn(*mut cef_request_t) -> cef_string_userfree_t>,
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};
    #[test]
    fn pinned_image_layouts() {
        assert_eq!(size_of::<cef_download_image_callback_t>(), 48);
        assert_eq!(size_of::<cef_image_t>(), 144);
        assert_eq!(offset_of!(cef_image_t, get_as_png), 128);
        assert_eq!(size_of::<cef_binary_value_t>(), 104);
        assert_eq!(offset_of!(cef_binary_value_t, get_size), 88);
        assert_eq!(offset_of!(CefRequestPrefix, get_url), 48);
        assert_eq!(offset_of!(CefRequestPrefix, get_method), 64);
        assert_eq!(offset_of!(crate::raw::cef_browser_host_t, download_image), 184);
    }
}

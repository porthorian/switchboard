#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

#[cfg(switchboard_cef_generated)]
include!(concat!(env!("OUT_DIR"), "/cef_bindings_generated.rs"));

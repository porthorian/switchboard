use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(switchboard_cef_generated)");
    println!("cargo:rerun-if-env-changed=SWITCHBOARD_CEF_GENERATE_BINDINGS");
    println!("cargo:rerun-if-env-changed=SWITCHBOARD_CEF_HEADER");
    println!("cargo:rerun-if-env-changed=SWITCHBOARD_CEF_INCLUDE_DIR");

    let generate = env::var("SWITCHBOARD_CEF_GENERATE_BINDINGS")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !generate {
        return;
    }

    let header = env::var("SWITCHBOARD_CEF_HEADER")
        .expect("SWITCHBOARD_CEF_HEADER is required when generating CEF bindings");
    let include_dir = env::var("SWITCHBOARD_CEF_INCLUDE_DIR").unwrap_or_default();
    let header_path = PathBuf::from(&header);

    let out_file = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"))
        .join("cef_bindings_generated.rs");

    let mut cmd = Command::new("bindgen");
    cmd.arg(&header)
        .arg("--output")
        .arg(&out_file)
        .arg("--allowlist-type")
        .arg("cef_.*")
        .arg("--allowlist-function")
        .arg("cef_.*")
        .arg("--allowlist-var")
        .arg("CEF_.*")
        .arg("--no-layout-tests")
        .arg("--use-core")
        .arg("--rust-target")
        .arg("1.70")
        .arg("--");

    let mut include_roots: Vec<PathBuf> = Vec::new();
    if !include_dir.is_empty() {
        include_roots.push(PathBuf::from(include_dir));
    }
    if let Some(parent) = header_path.parent() {
        include_roots.push(parent.to_path_buf());
        if let Some(parent2) = parent.parent() {
            include_roots.push(parent2.to_path_buf());
            if let Some(parent3) = parent2.parent() {
                include_roots.push(parent3.to_path_buf());
            }
        }
    }

    include_roots.sort();
    include_roots.dedup();
    for include_root in include_roots {
        cmd.arg(format!("-I{}", include_root.display()));
    }

    let status = cmd
        .status()
        .expect("failed to invoke bindgen executable; install `bindgen` CLI first");
    if !status.success() {
        panic!("bindgen failed generating CEF bindings");
    }

    println!("cargo:rustc-cfg=switchboard_cef_generated");
    println!("cargo:rerun-if-changed={header}");
}

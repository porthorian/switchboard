mod bridge;
mod host;
mod persistence;
mod runtime;

use host::DefaultHost;
#[cfg(target_os = "macos")]
use host::NativeMacHost;
use runtime::AppRuntime;

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let host = build_host()?;
    let runtime = AppRuntime::bootstrap(host, "0.1.0-dev")
        .map_err(|error| format!("switchboard-app: bootstrap failed\n  {error}"))?;
    println!(
        "switchboard alpha revision={} ui_view_id={} profiles={} workspaces={} tabs={}",
        runtime.revision(),
        runtime.ui_view_id().0,
        runtime.engine().state().profiles.len(),
        runtime.engine().state().workspaces.len(),
        runtime.engine().state().tabs.len()
    );

    runtime
        .run()
        .map_err(|error| format!("switchboard-app: event loop failed\n  {error}"))?;
    Ok(())
}

fn build_host() -> Result<DefaultHost, String> {
    #[cfg(target_os = "macos")]
    {
        NativeMacHost::new()
            .map_err(|error| format!("switchboard-app: host initialization failed\n  {error}"))
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(DefaultHost::default())
    }
}

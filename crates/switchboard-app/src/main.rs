mod bridge;
mod host;
mod persistence;
mod runtime;

use bridge::UiCommand;
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
    let mut runtime = AppRuntime::bootstrap(host, "0.1.0-dev")
        .map_err(|error| format!("switchboard-app: bootstrap failed\n  {error}"))?;
    if !runtime.has_tabs() {
        let workspace_id = runtime.default_workspace_id();
        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: None,
                make_active: true,
            })
            .map_err(|error| format!("switchboard-app: failed to create initial tab\n  {error}"))?;

        let tab_id = runtime
            .active_tab_id(workspace_id)
            .ok_or_else(|| "switchboard-app: created tab was not active".to_owned())?;

        let patch = runtime
            .handle_ui_command(UiCommand::Navigate {
                tab_id: tab_id.0,
                url: "https://youtube.com".to_owned(),
            })
            .map_err(|error| format!("switchboard-app: initial navigation failed\n  {error}"))?;

        println!(
            "milestone7 seeded revision={} ui_view_id={} profiles={} workspaces={} tabs={} patch_ops={}",
            runtime.revision(),
            runtime.ui_view_id().0,
            runtime.engine().state().profiles.len(),
            runtime.engine().state().workspaces.len(),
            runtime.engine().state().tabs.len(),
            patch.ops.len()
        );
    } else {
        println!(
            "milestone7 restored revision={} ui_view_id={} profiles={} workspaces={} tabs={}",
            runtime.revision(),
            runtime.ui_view_id().0,
            runtime.engine().state().profiles.len(),
            runtime.engine().state().workspaces.len(),
            runtime.engine().state().tabs.len()
        );
    }

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

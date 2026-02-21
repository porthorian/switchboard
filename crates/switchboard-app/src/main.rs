use switchboard_core::{BrowserState, Engine, Intent, NoopPersistence};

fn main() {
    let mut state = BrowserState::default();
    let profile_id = state.add_profile("Default");
    let workspace_id = state
        .add_workspace(profile_id, "Workspace 1")
        .expect("default profile must exist");

    let mut engine = Engine::with_state(NoopPersistence, state, 0);

    let _ = engine
        .dispatch(Intent::UiReady {
            ui_version: "0.1.0-dev".to_owned(),
        })
        .expect("ui ready intent should succeed");

    let patch = engine
        .dispatch(Intent::NewTab {
            workspace_id,
            url: Some("https://example.com".to_owned()),
            make_active: true,
        })
        .expect("initial tab creation should succeed");

    println!(
        "bootstrapped revision={} profiles={} workspaces={} tabs={} patch_ops={}",
        engine.revision(),
        engine.state().profiles.len(),
        engine.state().workspaces.len(),
        engine.state().tabs.len(),
        patch.ops.len()
    );
}

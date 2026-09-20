use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use switchboard_core::{
    Intent, Patch, Profile, ProfileId, SettingValue, Tab, TabId, Workspace, WorkspaceId,
};

pub const BRIDGE_PROTOCOL_VERSION: u16 = 1;
pub const MAX_BRIDGE_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_SEARCH_RESULTS: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientEnvelope {
    pub protocol_version: u16,
    pub request_id: String,
    pub command: UiCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerEnvelope {
    pub protocol_version: u16,
    pub request_id: Option<String>,
    pub revision: u64,
    pub message: ServerMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Snapshot {
        snapshot: UiSnapshot,
    },
    Patch {
        patch: Patch,
    },
    RestoreRequested {
        tab_id: TabId,
        generation: u64,
    },
    SearchResults {
        query: String,
        results: Vec<SearchResult>,
    },
    ActiveUri {
        url: String,
    },
    Error {
        code: String,
        message: String,
    },
    Ack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSnapshot {
    pub revision: u64,
    pub active_profile_id: Option<ProfileId>,
    pub profiles: Vec<Profile>,
    pub workspaces: Vec<Workspace>,
    pub tabs: Vec<Tab>,
    pub settings: BTreeMap<String, SettingValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResultKind {
    Command,
    OpenTab,
    History,
    ArchivedTab,
    Web,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub kind: SearchResultKind,
    pub title: String,
    pub subtitle: String,
    pub url: Option<String>,
    pub profile_id: Option<ProfileId>,
    pub workspace_id: Option<WorkspaceId>,
    pub tab_id: Option<TabId>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiCommand {
    UiReady {
        ui_version: String,
        last_revision: Option<u64>,
    },
    RequestResync {
        last_revision: Option<u64>,
    },
    FrameCommitted {
        tab_id: u64,
        generation: u64,
    },
    Search {
        query: String,
        limit: Option<usize>,
    },
    SetUiOverlay {
        visible: bool,
    },
    SetBrowserMode {
        active: bool,
    },
    SetFocusMode {
        active: bool,
    },
    SyncKeybindings {
        close_tab: String,
        command_palette: String,
        focus_navigation: String,
        toggle_devtools: String,
    },
    NewTab {
        workspace_id: u64,
        url: Option<String>,
        make_active: bool,
    },
    NewSubtab {
        workspace_id: u64,
        parent_tab_id: u64,
        url: String,
        make_active: bool,
    },
    OpenLinkFromTab {
        source_tab_id: u64,
        generation: u64,
        url: String,
        secondary: bool,
    },
    Navigate {
        tab_id: u64,
        url: String,
    },
    NavigateActive {
        url: String,
    },
    GoBack,
    GoForward,
    Reload,
    NewWorkspace {
        name: String,
    },
    NewProfile {
        name: String,
    },
    DeleteProfile {
        profile_id: u64,
    },
    RenameProfile {
        profile_id: u64,
        name: String,
    },
    RenameWorkspace {
        workspace_id: u64,
        name: String,
    },
    DeleteWorkspace {
        workspace_id: u64,
    },
    SwitchWorkspace {
        workspace_id: u64,
    },
    SwitchProfile {
        profile_id: u64,
    },
    ActivateTab {
        tab_id: u64,
    },
    CompleteTab {
        tab_id: u64,
        completed_at_ms: i64,
    },
    CloseTab {
        tab_id: u64,
    },
    RestoreTab {
        tab_id: u64,
    },
    SnoozeTab {
        tab_id: u64,
        wake_at_ms: i64,
    },
    WakeSnoozed {
        now_ms: i64,
    },
    PermanentlyDeleteTab {
        tab_id: u64,
    },
    ClearArchive {
        profile_id: u64,
    },
    RenameTab {
        tab_id: u64,
        title: String,
    },
    SetTabLocked {
        tab_id: u64,
        locked: bool,
    },
    PinTab {
        tab_id: u64,
        pinned: bool,
    },
    MoveTabTree {
        tab_id: u64,
        workspace_id: u64,
        parent_tab_id: Option<u64>,
        sibling_index: usize,
    },
    SetSecondaryTab {
        workspace_id: u64,
        tab_id: u64,
    },
    ClearSplit {
        workspace_id: u64,
    },
    SwapSplit {
        workspace_id: u64,
    },
    SetSplitRatio {
        workspace_id: u64,
        ratio: f64,
    },
    ClearProfileHistory {
        profile_id: u64,
    },
    ClearAllHistory,
    ToggleDevTools,
    SettingSet {
        key: String,
        value: SettingValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeDecodeError {
    TooLarge,
    InvalidJson(String),
    UnsupportedVersion(u16),
    MissingRequestId,
}

pub fn decode_client_message(payload: &str) -> Result<ClientEnvelope, BridgeDecodeError> {
    if payload.len() > MAX_BRIDGE_MESSAGE_BYTES {
        return Err(BridgeDecodeError::TooLarge);
    }
    let envelope: ClientEnvelope = serde_json::from_str(payload)
        .map_err(|error| BridgeDecodeError::InvalidJson(error.to_string()))?;
    if envelope.protocol_version != BRIDGE_PROTOCOL_VERSION {
        return Err(BridgeDecodeError::UnsupportedVersion(
            envelope.protocol_version,
        ));
    }
    if envelope.request_id.trim().is_empty() || envelope.request_id.len() > 128 {
        return Err(BridgeDecodeError::MissingRequestId);
    }
    Ok(envelope)
}

pub fn encode_server_message(envelope: &ServerEnvelope) -> Result<String, serde_json::Error> {
    serde_json::to_string(envelope)
}

impl UiCommand {
    pub fn into_intent(self) -> Intent {
        match self {
            Self::UiReady { ui_version, .. } => Intent::UiReady { ui_version },
            Self::FrameCommitted { tab_id, generation } => Intent::TabFrameCommitted {
                tab_id: TabId(tab_id),
                generation,
            },
            Self::NewTab {
                workspace_id,
                url,
                make_active,
            } => Intent::NewTab {
                workspace_id: WorkspaceId(workspace_id),
                url,
                make_active,
            },
            Self::NewSubtab {
                workspace_id,
                parent_tab_id,
                url,
                make_active,
            } => Intent::NewSubtab {
                workspace_id: WorkspaceId(workspace_id),
                parent_tab_id: TabId(parent_tab_id),
                url,
                make_active,
            },
            Self::OpenLinkFromTab {
                source_tab_id,
                generation: _,
                url,
                secondary,
            } => Intent::OpenLinkFromTab {
                source_tab_id: TabId(source_tab_id),
                url,
                secondary,
            },
            Self::Navigate { tab_id, url } => Intent::Navigate {
                tab_id: TabId(tab_id),
                url,
            },
            Self::DeleteProfile { profile_id } => Intent::DeleteProfile {
                profile_id: ProfileId(profile_id),
            },
            Self::RenameProfile { profile_id, name } => Intent::RenameProfile {
                profile_id: ProfileId(profile_id),
                name,
            },
            Self::RenameWorkspace { workspace_id, name } => Intent::RenameWorkspace {
                workspace_id: WorkspaceId(workspace_id),
                name,
            },
            Self::DeleteWorkspace { workspace_id } => Intent::DeleteWorkspace {
                workspace_id: WorkspaceId(workspace_id),
            },
            Self::SwitchWorkspace { workspace_id } => Intent::SwitchWorkspace {
                workspace_id: WorkspaceId(workspace_id),
            },
            Self::SwitchProfile { profile_id } => Intent::SwitchProfile {
                profile_id: ProfileId(profile_id),
            },
            Self::ActivateTab { tab_id } => Intent::ActivateTab {
                tab_id: TabId(tab_id),
            },
            Self::CloseTab { tab_id } => Intent::CloseTab {
                tab_id: TabId(tab_id),
            },
            Self::CompleteTab {
                tab_id,
                completed_at_ms,
            } => Intent::CompleteTab {
                tab_id: TabId(tab_id),
                completed_at_ms,
            },
            Self::RestoreTab { tab_id } => Intent::RestoreTab {
                tab_id: TabId(tab_id),
            },
            Self::SnoozeTab { tab_id, wake_at_ms } => Intent::SnoozeTab {
                tab_id: TabId(tab_id),
                wake_at_ms,
            },
            Self::WakeSnoozed { now_ms } => Intent::WakeSnoozed { now_ms },
            Self::PermanentlyDeleteTab { tab_id } => Intent::PermanentlyDeleteTab {
                tab_id: TabId(tab_id),
            },
            Self::ClearArchive { profile_id } => Intent::ClearArchive {
                profile_id: ProfileId(profile_id),
            },
            Self::RenameTab { tab_id, title } => Intent::RenameTab {
                tab_id: TabId(tab_id),
                title,
            },
            Self::SetTabLocked { tab_id, locked } => Intent::SetTabLocked {
                tab_id: TabId(tab_id),
                locked,
            },
            Self::PinTab { tab_id, pinned } => Intent::PinTab {
                tab_id: TabId(tab_id),
                pinned,
            },
            Self::MoveTabTree {
                tab_id,
                workspace_id,
                parent_tab_id,
                sibling_index,
            } => Intent::MoveTabTree {
                tab_id: TabId(tab_id),
                workspace_id: WorkspaceId(workspace_id),
                parent_tab_id: parent_tab_id.map(TabId),
                sibling_index,
            },
            Self::SetSecondaryTab {
                workspace_id,
                tab_id,
            } => Intent::SetSecondaryTab {
                workspace_id: WorkspaceId(workspace_id),
                tab_id: TabId(tab_id),
            },
            Self::ClearSplit { workspace_id } => Intent::ClearSplit {
                workspace_id: WorkspaceId(workspace_id),
            },
            Self::SwapSplit { workspace_id } => Intent::SwapSplit {
                workspace_id: WorkspaceId(workspace_id),
            },
            Self::SetSplitRatio {
                workspace_id,
                ratio,
            } => Intent::SetSplitRatio {
                workspace_id: WorkspaceId(workspace_id),
                ratio,
            },
            Self::ClearProfileHistory { profile_id } => Intent::ClearHistory {
                scope: switchboard_core::ClearHistoryScope::Profile(ProfileId(profile_id)),
            },
            Self::ClearAllHistory => Intent::ClearHistory {
                scope: switchboard_core::ClearHistoryScope::All,
            },
            Self::SettingSet { key, value } => Intent::SettingSet { key, value },
            Self::NavigateActive { .. }
            | Self::GoBack
            | Self::GoForward
            | Self::Reload
            | Self::NewWorkspace { .. }
            | Self::NewProfile { .. }
            | Self::Search { .. }
            | Self::SetUiOverlay { .. }
            | Self::SetBrowserMode { .. }
            | Self::SetFocusMode { .. }
            | Self::SyncKeybindings { .. }
            | Self::RequestResync { .. }
            | Self::ToggleDevTools => {
                unreachable!("runtime-only UI command must be handled before intent conversion")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_message_roundtrips() {
        let value = ClientEnvelope {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            request_id: "request-1".into(),
            command: UiCommand::ActivateTab { tab_id: 7 },
        };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(decode_client_message(&json).unwrap(), value);
    }

    #[test]
    fn unknown_command_and_version_are_rejected() {
        let unknown = r#"{"protocol_version":1,"request_id":"x","command":{"type":"execute_arbitrary_code"}}"#;
        assert!(matches!(
            decode_client_message(unknown),
            Err(BridgeDecodeError::InvalidJson(_))
        ));
        let wrong = r#"{"protocol_version":9,"request_id":"x","command":{"type":"go_back"}}"#;
        assert_eq!(
            decode_client_message(wrong),
            Err(BridgeDecodeError::UnsupportedVersion(9))
        );
    }

    #[test]
    fn oversized_message_is_rejected_before_parsing() {
        let payload = "x".repeat(MAX_BRIDGE_MESSAGE_BYTES + 1);
        assert_eq!(
            decode_client_message(&payload),
            Err(BridgeDecodeError::TooLarge)
        );
    }

    #[test]
    fn missing_request_id_and_malformed_payload_are_rejected() {
        let missing = r#"{"protocol_version":1,"request_id":"","command":{"type":"go_back"}}"#;
        assert_eq!(
            decode_client_message(missing),
            Err(BridgeDecodeError::MissingRequestId)
        );
        assert!(matches!(
            decode_client_message("{not-json"),
            Err(BridgeDecodeError::InvalidJson(_))
        ));
    }

    #[test]
    fn patch_envelope_uses_stable_tagged_operation_shape() {
        let envelope = ServerEnvelope {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            request_id: None,
            revision: 2,
            message: ServerMessage::Patch {
                patch: Patch {
                    from_revision: 1,
                    to_revision: 2,
                    ops: vec![switchboard_core::PatchOp::SetActiveTab {
                        workspace_id: WorkspaceId(4),
                        tab_id: Some(TabId(7)),
                    }],
                },
            },
        };
        let json = encode_server_message(&envelope).unwrap();
        assert!(json.contains(r#""type":"set_active_tab""#));
        assert!(json.contains(r#""workspace_id":4"#));
        assert!(json.contains(r#""tab_id":7"#));

        let profile_json =
            serde_json::to_string(&switchboard_core::PatchOp::UpsertProfile(Profile {
                id: ProfileId(9),
                name: "Work".into(),
                workspace_order: Vec::new(),
                active_workspace_id: None,
            }))
            .unwrap();
        assert_eq!(
            profile_json,
            r#"{"type":"upsert_profile","id":9,"name":"Work","workspace_order":[],"active_workspace_id":null}"#
        );
    }
}

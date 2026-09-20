use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use switchboard_core::{
    BrowserState, HistoryEntry, Persistence, Profile, ProfileId, RestorePosition, SettingValue,
    Tab, TabId, TabRuntimeState, TabStatus, Workspace, WorkspaceId,
};

const SCHEMA_VERSION: i64 = 2;
const META_SCHEMA_VERSION: &str = "schema_version";
const META_REVISION: &str = "revision";
const META_ACTIVE_PROFILE_ID: &str = "active_profile_id";
const ENV_STATE_DB_PATH: &str = "SWITCHBOARD_STATE_DB_PATH";

pub struct AppPersistence {
    connection: Connection,
}

#[derive(Debug)]
pub enum AppPersistenceError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    InvalidData(String),
}

impl Display for AppPersistenceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Io(error) => write!(f, "persistence I/O failed: {error}"),
            Self::Sqlite(error) => write!(f, "SQLite operation failed: {error}"),
            Self::InvalidData(message) => write!(f, "invalid persisted state: {message}"),
        }
    }
}

impl Error for AppPersistenceError {}

impl From<std::io::Error> for AppPersistenceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for AppPersistenceError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl AppPersistence {
    pub fn open_default() -> Result<Self, AppPersistenceError> {
        Self::open_path(default_state_db_path()?)
    }

    pub fn open_path(path: impl AsRef<Path>) -> Result<Self, AppPersistenceError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        backup_incompatible_database(path)?;
        let connection = Connection::open(path)?;
        configure_connection(&connection, true)?;
        migrate_v2(&connection)?;
        Ok(Self { connection })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, AppPersistenceError> {
        let connection = Connection::open_in_memory()?;
        configure_connection(&connection, false)?;
        migrate_v2(&connection)?;
        Ok(Self { connection })
    }

    pub fn load(&self) -> Result<Option<(BrowserState, u64)>, AppPersistenceError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))?;
        if count == 0 {
            return Ok(None);
        }

        let mut state = BrowserState::default();
        {
            let mut statement = self.connection.prepare(
                "SELECT id, name, active_workspace_id FROM profiles ORDER BY position, id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(Profile {
                    id: ProfileId(as_u64(row.get::<_, i64>(0)?, "profiles.id")?),
                    name: row.get(1)?,
                    workspace_order: Vec::new(),
                    active_workspace_id: row
                        .get::<_, Option<i64>>(2)?
                        .map(|id| as_u64(id, "profiles.active_workspace_id"))
                        .transpose()?
                        .map(WorkspaceId),
                })
            })?;
            for profile in rows {
                let profile = profile?;
                state.profiles.insert(profile.id, profile);
            }
        }

        {
            let mut statement = self.connection.prepare(
                "SELECT id, profile_id, name, primary_tab_id, secondary_tab_id, split_enabled, split_ratio
                 FROM workspaces ORDER BY profile_id, position, id",
            )?;
            let rows = statement.query_map([], |row| {
                let id = WorkspaceId(as_u64(row.get::<_, i64>(0)?, "workspaces.id")?);
                let profile_id = ProfileId(as_u64(row.get::<_, i64>(1)?, "workspaces.profile_id")?);
                let primary_tab_id = row
                    .get::<_, Option<i64>>(3)?
                    .map(|value| as_u64(value, "workspaces.primary_tab_id"))
                    .transpose()?
                    .map(TabId);
                Ok(Workspace {
                    id,
                    profile_id,
                    name: row.get(2)?,
                    tab_order: Vec::new(),
                    active_tab_id: primary_tab_id,
                    primary_tab_id,
                    secondary_tab_id: row
                        .get::<_, Option<i64>>(4)?
                        .map(|value| as_u64(value, "workspaces.secondary_tab_id"))
                        .transpose()?
                        .map(TabId),
                    split_enabled: row.get::<_, i64>(5)? != 0,
                    split_ratio: row.get::<_, f64>(6)?.clamp(0.25, 0.75),
                })
            })?;
            for workspace in rows {
                let workspace = workspace?;
                let profile = state
                    .profiles
                    .get_mut(&workspace.profile_id)
                    .ok_or_else(|| {
                        AppPersistenceError::InvalidData(format!(
                            "workspace {} references missing profile {}",
                            workspace.id.0, workspace.profile_id.0
                        ))
                    })?;
                profile.workspace_order.push(workspace.id);
                state.workspaces.insert(workspace.id, workspace);
            }
        }

        {
            let mut statement = self.connection.prepare(
                "SELECT id, profile_id, workspace_id, parent_tab_id, url, observed_title,
                        custom_title, pinned, locked, muted, status, status_time_ms,
                        restore_parent_tab_id, restore_sibling_index, position
                 FROM tabs ORDER BY workspace_id, CASE WHEN position IS NULL THEN 1 ELSE 0 END, position, id",
            )?;
            let rows = statement.query_map([], load_tab)?;
            for row in rows {
                let (tab, position) = row?;
                if !state.profiles.contains_key(&tab.profile_id) {
                    return Err(AppPersistenceError::InvalidData(format!(
                        "tab {} references missing profile {}",
                        tab.id.0, tab.profile_id.0
                    )));
                }
                let workspace = state.workspaces.get_mut(&tab.workspace_id).ok_or_else(|| {
                    AppPersistenceError::InvalidData(format!(
                        "tab {} references missing workspace {}",
                        tab.id.0, tab.workspace_id.0
                    ))
                })?;
                if tab.status.is_open() && position.is_some() {
                    workspace.tab_order.push(tab.id);
                }
                state.tabs.insert(tab.id, tab);
            }
        }

        {
            let mut statement = self.connection.prepare(
                "SELECT key, value_type, bool_value, int_value, text_value FROM settings ORDER BY key",
            )?;
            let rows = statement.query_map([], |row| {
                let key: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let value = match kind.as_str() {
                    "bool" => SettingValue::Bool(row.get::<_, i64>(2)? != 0),
                    "int" => SettingValue::Int(row.get(3)?),
                    "text" => SettingValue::Text(row.get(4)?),
                    _ => {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(InvalidValue(format!("unknown setting type {kind}"))),
                        ))
                    }
                };
                Ok((key, value))
            })?;
            for row in rows {
                let (key, value) = row?;
                state.settings.insert(key, value);
            }
        }

        {
            let mut statement = self.connection.prepare(
                "SELECT profile_id, url, title, visit_count, last_visit_ms
                 FROM history ORDER BY profile_id, sequence",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(HistoryEntry {
                    profile_id: ProfileId(as_u64(row.get::<_, i64>(0)?, "history.profile_id")?),
                    url: row.get(1)?,
                    title: row.get(2)?,
                    visit_count: as_u64(row.get::<_, i64>(3)?, "history.visit_count")?,
                    last_visit_ms: row.get(4)?,
                })
            })?;
            for entry in rows {
                let entry = entry?;
                state
                    .history
                    .entry(entry.profile_id)
                    .or_default()
                    .push(entry);
            }
        }

        state.active_profile_id = meta_value(&self.connection, META_ACTIVE_PROFILE_ID)?
            .filter(|value| !value.is_empty())
            .map(|value| {
                value.parse::<u64>().map(ProfileId).map_err(|_| {
                    AppPersistenceError::InvalidData("invalid active profile id".into())
                })
            })
            .transpose()?;
        if state
            .active_profile_id
            .is_some_and(|id| !state.profiles.contains_key(&id))
        {
            state.active_profile_id = state.profiles.keys().next().copied();
        }
        state.recompute_next_ids();
        state.reset_runtime_state();
        let revision = meta_value(&self.connection, META_REVISION)?
            .unwrap_or_else(|| "0".into())
            .parse::<u64>()
            .map_err(|_| AppPersistenceError::InvalidData("invalid persisted revision".into()))?;
        Ok(Some((state, revision)))
    }
}

impl Persistence for AppPersistence {
    type Error = AppPersistenceError;

    fn commit(&mut self, state: &BrowserState, revision: u64) -> Result<(), Self::Error> {
        let transaction = self.connection.transaction()?;
        save_state(&transaction, state, revision)?;
        transaction.commit()?;
        Ok(())
    }
}

fn configure_connection(
    connection: &Connection,
    file_backed: bool,
) -> Result<(), AppPersistenceError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    if file_backed {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
    }
    Ok(())
}

fn migrate_v2(connection: &Connection) -> Result<(), AppPersistenceError> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS profiles (
             id INTEGER PRIMARY KEY,
             name TEXT NOT NULL,
             active_workspace_id INTEGER,
             position INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS workspaces (
             id INTEGER PRIMARY KEY,
             profile_id INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             name TEXT NOT NULL,
             primary_tab_id INTEGER,
             secondary_tab_id INTEGER,
             split_enabled INTEGER NOT NULL,
             split_ratio REAL NOT NULL,
             position INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS tabs (
             id INTEGER PRIMARY KEY,
             profile_id INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             workspace_id INTEGER NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
             parent_tab_id INTEGER REFERENCES tabs(id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED,
             url TEXT NOT NULL,
             observed_title TEXT NOT NULL,
             custom_title TEXT,
             pinned INTEGER NOT NULL,
             locked INTEGER NOT NULL,
             muted INTEGER NOT NULL,
             status TEXT NOT NULL,
             status_time_ms INTEGER,
             restore_parent_tab_id INTEGER,
             restore_sibling_index INTEGER,
             position INTEGER
         );
         CREATE INDEX IF NOT EXISTS tabs_workspace_position ON tabs(workspace_id, position);
         CREATE INDEX IF NOT EXISTS tabs_profile_status ON tabs(profile_id, status, status_time_ms);
         CREATE TABLE IF NOT EXISTS settings (
             key TEXT PRIMARY KEY,
             value_type TEXT NOT NULL,
             bool_value INTEGER,
             int_value INTEGER,
             text_value TEXT
         );
         CREATE TABLE IF NOT EXISTS history (
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,
             profile_id INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             url TEXT NOT NULL,
             title TEXT NOT NULL,
             visit_count INTEGER NOT NULL,
             last_visit_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS history_profile_recency ON history(profile_id, last_visit_ms DESC);
         INSERT OR REPLACE INTO meta(key, value) VALUES('schema_version', '2');
         INSERT OR IGNORE INTO meta(key, value) VALUES('revision', '0');
         COMMIT;",
    )?;
    Ok(())
}

fn save_state(
    transaction: &Transaction<'_>,
    state: &BrowserState,
    revision: u64,
) -> Result<(), AppPersistenceError> {
    transaction.execute_batch(
        "PRAGMA defer_foreign_keys = ON;
         DELETE FROM history;
         DELETE FROM tabs;
         DELETE FROM workspaces;
         DELETE FROM profiles;
         DELETE FROM settings;",
    )?;

    for (position, profile_id) in state.profiles.keys().enumerate() {
        let profile = &state.profiles[profile_id];
        transaction.execute(
            "INSERT INTO profiles(id, name, active_workspace_id, position) VALUES(?1, ?2, ?3, ?4)",
            params![
                to_i64(profile.id.0)?,
                profile.name,
                profile
                    .active_workspace_id
                    .map(|id| to_i64(id.0))
                    .transpose()?,
                to_i64(position as u64)?
            ],
        )?;
    }
    for profile in state.profiles.values() {
        for (position, workspace_id) in profile.workspace_order.iter().enumerate() {
            let workspace = state.workspaces.get(workspace_id).ok_or_else(|| {
                AppPersistenceError::InvalidData(format!(
                    "profile order references missing workspace {}",
                    workspace_id.0
                ))
            })?;
            transaction.execute(
                "INSERT INTO workspaces(id, profile_id, name, primary_tab_id, secondary_tab_id, split_enabled, split_ratio, position)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    to_i64(workspace.id.0)?,
                    to_i64(workspace.profile_id.0)?,
                    workspace.name,
                    workspace.primary_tab_id.map(|id| to_i64(id.0)).transpose()?,
                    workspace.secondary_tab_id.map(|id| to_i64(id.0)).transpose()?,
                    i64::from(workspace.split_enabled),
                    workspace.split_ratio.clamp(0.25, 0.75),
                    to_i64(position as u64)?,
                ],
            )?;
        }
    }

    for tab in state.tabs.values() {
        let position = state
            .workspaces
            .get(&tab.workspace_id)
            .and_then(|workspace| workspace.tab_order.iter().position(|id| *id == tab.id))
            .map(|value| to_i64(value as u64))
            .transpose()?;
        let (status, status_time, restore_parent, restore_index) = match tab.status {
            TabStatus::Open => ("open", None, None, None),
            TabStatus::Done {
                completed_at_ms,
                restore,
            } => (
                "done",
                Some(completed_at_ms),
                restore.parent_tab_id.map(|id| to_i64(id.0)).transpose()?,
                Some(to_i64(restore.sibling_index as u64)?),
            ),
            TabStatus::Snoozed {
                wake_at_ms,
                restore,
            } => (
                "snoozed",
                Some(wake_at_ms),
                restore.parent_tab_id.map(|id| to_i64(id.0)).transpose()?,
                Some(to_i64(restore.sibling_index as u64)?),
            ),
        };
        transaction.execute(
            "INSERT INTO tabs(id, profile_id, workspace_id, parent_tab_id, url, observed_title,
                              custom_title, pinned, locked, muted, status, status_time_ms,
                              restore_parent_tab_id, restore_sibling_index, position)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                to_i64(tab.id.0)?,
                to_i64(tab.profile_id.0)?,
                to_i64(tab.workspace_id.0)?,
                tab.parent_tab_id.map(|id| to_i64(id.0)).transpose()?,
                tab.url,
                tab.observed_title,
                tab.custom_title,
                i64::from(tab.pinned),
                i64::from(tab.locked),
                i64::from(tab.muted),
                status,
                status_time,
                restore_parent,
                restore_index,
                position,
            ],
        )?;
    }

    for (key, value) in &state.settings {
        let (kind, bool_value, int_value, text_value): (
            &str,
            Option<i64>,
            Option<i64>,
            Option<&str>,
        ) = match value {
            SettingValue::Bool(value) => ("bool", Some(i64::from(*value)), None, None),
            SettingValue::Int(value) => ("int", None, Some(*value), None),
            SettingValue::Text(value) => ("text", None, None, Some(value)),
        };
        transaction.execute(
            "INSERT INTO settings(key, value_type, bool_value, int_value, text_value) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![key, kind, bool_value, int_value, text_value],
        )?;
    }

    for profile in state.profiles.values() {
        if let Some(entries) = state.history.get(&profile.id) {
            for entry in entries {
                transaction.execute(
                    "INSERT INTO history(profile_id, url, title, visit_count, last_visit_ms) VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![to_i64(profile.id.0)?, entry.url, entry.title, to_i64(entry.visit_count)?, entry.last_visit_ms],
                )?;
            }
        }
    }

    transaction.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES(?1, ?2)",
        params![META_SCHEMA_VERSION, SCHEMA_VERSION.to_string()],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES(?1, ?2)",
        params![META_REVISION, revision.to_string()],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES(?1, ?2)",
        params![
            META_ACTIVE_PROFILE_ID,
            state
                .active_profile_id
                .map(|id| id.0.to_string())
                .unwrap_or_default()
        ],
    )?;
    Ok(())
}

fn load_tab(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Tab, Option<i64>)> {
    let id = TabId(as_u64(row.get::<_, i64>(0)?, "tabs.id")?);
    let restore = RestorePosition {
        parent_tab_id: row
            .get::<_, Option<i64>>(12)?
            .map(|value| as_u64(value, "tabs.restore_parent_tab_id"))
            .transpose()?
            .map(TabId),
        sibling_index: row
            .get::<_, Option<i64>>(13)?
            .map(|value| as_u64(value, "tabs.restore_sibling_index"))
            .transpose()?
            .unwrap_or(0) as usize,
    };
    let status_name: String = row.get(10)?;
    let status_time = row.get::<_, Option<i64>>(11)?.unwrap_or(0);
    let status = match status_name.as_str() {
        "open" => TabStatus::Open,
        "done" => TabStatus::Done {
            completed_at_ms: status_time,
            restore,
        },
        "snoozed" => TabStatus::Snoozed {
            wake_at_ms: status_time,
            restore,
        },
        _ => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(InvalidValue(format!("unknown tab status {status_name}"))),
            ))
        }
    };
    let observed_title: String = row.get(5)?;
    let custom_title: Option<String> = row.get(6)?;
    let title = custom_title
        .as_deref()
        .unwrap_or(&observed_title)
        .to_owned();
    Ok((
        Tab {
            id,
            profile_id: ProfileId(as_u64(row.get::<_, i64>(1)?, "tabs.profile_id")?),
            workspace_id: WorkspaceId(as_u64(row.get::<_, i64>(2)?, "tabs.workspace_id")?),
            parent_tab_id: row
                .get::<_, Option<i64>>(3)?
                .map(|value| as_u64(value, "tabs.parent_tab_id"))
                .transpose()?
                .map(TabId),
            url: row.get(4)?,
            title,
            observed_title,
            custom_title,
            pinned: row.get::<_, i64>(7)? != 0,
            locked: row.get::<_, i64>(8)? != 0,
            muted: row.get::<_, i64>(9)? != 0,
            status,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            runtime_state: TabRuntimeState::Discarded,
        },
        row.get(14)?,
    ))
}

fn meta_value(connection: &Connection, key: &str) -> Result<Option<String>, AppPersistenceError> {
    Ok(connection
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

fn backup_incompatible_database(path: &Path) -> Result<Option<PathBuf>, AppPersistenceError> {
    if !path.exists() || fs::metadata(path)?.len() == 0 {
        return Ok(None);
    }
    let connection = Connection::open(path)?;
    let has_meta: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta')",
        [],
        |row| row.get(0),
    )?;
    let version = if has_meta {
        connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .and_then(|value| value.parse::<i64>().ok())
    } else {
        None
    };
    drop(connection);
    if version == Some(SCHEMA_VERSION) {
        return Ok(None);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("sqlite3");
    let backup = path.with_extension(format!(
        "v{}-backup-{stamp}.{extension}",
        version.unwrap_or(0)
    ));
    fs::rename(path, &backup)?;
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    let shm = PathBuf::from(format!("{}-shm", path.display()));
    if wal.exists() {
        fs::rename(&wal, PathBuf::from(format!("{}-wal", backup.display())))?;
    }
    if shm.exists() {
        fs::rename(&shm, PathBuf::from(format!("{}-shm", backup.display())))?;
    }
    Ok(Some(backup))
}

fn default_state_db_path() -> Result<PathBuf, AppPersistenceError> {
    resolve_state_db_path(
        std::env::var_os(ENV_STATE_DB_PATH).map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

fn resolve_state_db_path(
    configured: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, AppPersistenceError> {
    if let Some(configured) = configured.filter(|path| !path.as_os_str().is_empty()) {
        if !configured.is_absolute() {
            return Err(AppPersistenceError::InvalidData(format!(
                "{ENV_STATE_DB_PATH} must be an absolute path"
            )));
        }
        return Ok(configured);
    }
    let base = home.ok_or_else(|| AppPersistenceError::InvalidData("HOME is not set".into()))?;
    Ok(base
        .join("Library")
        .join("Application Support")
        .join("Switchboard")
        .join("state.sqlite3"))
}

fn to_i64(value: u64) -> Result<i64, AppPersistenceError> {
    i64::try_from(value).map_err(|_| {
        AppPersistenceError::InvalidData(format!("integer {value} exceeds SQLite range"))
    })
}

fn as_u64(value: i64, field: &'static str) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            Box::new(InvalidValue(format!("{field} is negative"))),
        )
    })
}

#[derive(Debug)]
struct InvalidValue(String);

impl Display for InvalidValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}

impl Error for InvalidValue {}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::{Intent, NoopPersistence};

    fn temporary_database_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "switchboard-{name}-{}-{}.sqlite3",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn seeded_state() -> BrowserState {
        let mut engine = switchboard_core::Engine::new(NoopPersistence);
        engine
            .dispatch(Intent::NewProfile {
                name: "Default".into(),
            })
            .unwrap();
        let workspace = engine.state().active_workspace_id().unwrap();
        engine
            .dispatch(Intent::NewTab {
                workspace_id: workspace,
                url: Some("https://example.com".into()),
                make_active: true,
            })
            .unwrap();
        engine.state().clone()
    }

    #[test]
    fn v2_roundtrip_restores_revision_and_resets_runtime() {
        let mut persistence = AppPersistence::open_in_memory().unwrap();
        let state = seeded_state();
        persistence.commit(&state, 42).unwrap();
        let (loaded, revision) = persistence.load().unwrap().unwrap();
        assert_eq!(revision, 42);
        assert_eq!(loaded.profiles, state.profiles);
        assert_eq!(loaded.workspaces, state.workspaces);
        assert!(loaded
            .tabs
            .values()
            .all(|tab| tab.runtime_state == TabRuntimeState::Discarded));
    }

    #[test]
    fn history_and_archive_survive_roundtrip() {
        let mut persistence = AppPersistence::open_in_memory().unwrap();
        let mut state = seeded_state();
        let profile = state.active_profile_id.unwrap();
        let tab = *state.tabs.keys().next().unwrap();
        switchboard_core::reducer::apply_intent(
            &mut state,
            Intent::RecordHistory {
                profile_id: profile,
                url: "https://example.com".into(),
                title: "Example".into(),
                visited_at_ms: 9,
            },
        )
        .unwrap();
        switchboard_core::reducer::apply_intent(
            &mut state,
            Intent::CompleteTab {
                tab_id: tab,
                completed_at_ms: 10,
            },
        )
        .unwrap();
        persistence.commit(&state, 7).unwrap();
        let (loaded, _) = persistence.load().unwrap().unwrap();
        assert!(matches!(
            loaded.tabs[&tab].status,
            TabStatus::Done {
                completed_at_ms: 10,
                ..
            }
        ));
        assert_eq!(loaded.history[&profile][0].visit_count, 1);
    }

    #[test]
    fn incompatible_v1_database_is_backed_up_before_v2_creation() {
        let path = temporary_database_path("v1-backup");
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO meta(key, value) VALUES ('schema_version', '1');
                 CREATE TABLE legacy_marker(value TEXT NOT NULL);
                 INSERT INTO legacy_marker(value) VALUES ('preserve-me');",
            )
            .unwrap();
        drop(legacy);

        let persistence = AppPersistence::open_path(&path).unwrap();
        assert!(persistence.load().unwrap().is_none());
        let parent = path.parent().unwrap();
        let prefix = format!("{}.v1-backup-", path.file_stem().unwrap().to_string_lossy());
        let backup = fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|candidate| {
                candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
            })
            .expect("v1 backup should exist");
        let preserved = Connection::open(&backup)
            .unwrap()
            .query_row("SELECT value FROM legacy_marker", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap();
        assert_eq!(preserved, "preserve-me");
        fs::remove_file(path).ok();
        fs::remove_file(backup).ok();
    }

    #[test]
    fn failed_sqlite_commit_rolls_back_state_and_revision() {
        let mut persistence = AppPersistence::open_in_memory().unwrap();
        let state = seeded_state();
        persistence.commit(&state, 3).unwrap();
        let mut invalid = state.clone();
        invalid.profiles.clear();
        assert!(persistence.commit(&invalid, 4).is_err());
        let (loaded, revision) = persistence.load().unwrap().unwrap();
        assert_eq!(revision, 3);
        assert_eq!(loaded.profiles, state.profiles);
        assert_eq!(loaded.workspaces, state.workspaces);
    }

    #[test]
    fn history_has_no_implicit_limit_and_clear_scopes_persist() {
        let mut persistence = AppPersistence::open_in_memory().unwrap();
        let mut state = seeded_state();
        let first_profile = state.active_profile_id.unwrap();
        for index in 0..600 {
            switchboard_core::reducer::apply_intent(
                &mut state,
                Intent::RecordHistory {
                    profile_id: first_profile,
                    url: format!("https://history-{index}.example"),
                    title: format!("History {index}"),
                    visited_at_ms: index,
                },
            )
            .unwrap();
        }
        switchboard_core::reducer::apply_intent(
            &mut state,
            Intent::NewProfile {
                name: "Other".into(),
            },
        )
        .unwrap();
        let second_profile = state.active_profile_id.unwrap();
        switchboard_core::reducer::apply_intent(
            &mut state,
            Intent::RecordHistory {
                profile_id: second_profile,
                url: "https://other.example".into(),
                title: "Other".into(),
                visited_at_ms: 700,
            },
        )
        .unwrap();

        persistence.commit(&state, 1).unwrap();
        let (mut loaded, _) = persistence.load().unwrap().unwrap();
        assert_eq!(loaded.history[&first_profile].len(), 600);
        switchboard_core::reducer::apply_intent(
            &mut loaded,
            Intent::ClearHistory {
                scope: switchboard_core::ClearHistoryScope::Profile(first_profile),
            },
        )
        .unwrap();
        persistence.commit(&loaded, 2).unwrap();
        let (mut loaded, _) = persistence.load().unwrap().unwrap();
        assert!(!loaded.history.contains_key(&first_profile));
        assert_eq!(loaded.history[&second_profile].len(), 1);

        switchboard_core::reducer::apply_intent(
            &mut loaded,
            Intent::ClearHistory {
                scope: switchboard_core::ClearHistoryScope::All,
            },
        )
        .unwrap();
        persistence.commit(&loaded, 3).unwrap();
        let (loaded, _) = persistence.load().unwrap().unwrap();
        assert!(loaded.history.is_empty());
    }

    #[test]
    fn smoke_database_override_must_be_absolute() {
        let configured = PathBuf::from("/private/tmp/switchboard-smoke/state.sqlite3");
        assert_eq!(
            resolve_state_db_path(Some(configured.clone()), None).unwrap(),
            configured
        );
        assert!(resolve_state_db_path(Some(PathBuf::from("relative.sqlite3")), None).is_err());
    }
}

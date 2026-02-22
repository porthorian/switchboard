#![cfg_attr(test, allow(dead_code))]

use std::env;
use std::error::Error;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use switchboard_core::{
    BrowserState, Persistence, Profile, ProfileId, SettingValue, Tab, TabId, TabRuntimeState,
    Workspace, WorkspaceId,
};

const ENV_STATE_DB: &str = "SWITCHBOARD_STATE_DB";
const META_SCHEMA_VERSION: &str = "schema_version";
const META_ACTIVE_PROFILE_ID: &str = "active_profile_id";
const SCHEMA_VERSION: i64 = 1;

const SQLITE_OK: c_int = 0;
const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;

#[repr(C)]
struct sqlite3 {
    _private: [u8; 0],
}

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        pp_db: *mut *mut sqlite3,
        flags: c_int,
        z_vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut sqlite3) -> c_int;
    fn sqlite3_exec(
        db: *mut sqlite3,
        sql: *const c_char,
        callback: Option<
            unsafe extern "C" fn(
                arg: *mut c_void,
                argc: c_int,
                argv: *mut *mut c_char,
                col_names: *mut *mut c_char,
            ) -> c_int,
        >,
        arg: *mut c_void,
        errmsg: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_errmsg(db: *mut sqlite3) -> *const c_char;
    fn sqlite3_free(ptr: *mut c_void);
}

pub struct AppPersistence {
    store: SqliteStore,
}

struct SqliteStore {
    db: *mut sqlite3,
}

#[derive(Debug)]
pub enum AppPersistenceError {
    Io(std::io::Error),
    Sqlite(String),
    InvalidData(String),
}

impl Display for AppPersistenceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Io(err) => write!(f, "filesystem error: {err}"),
            Self::Sqlite(message) => write!(f, "sqlite error: {message}"),
            Self::InvalidData(message) => write!(f, "invalid persisted data: {message}"),
        }
    }
}

impl Error for AppPersistenceError {}

impl From<std::io::Error> for AppPersistenceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl AppPersistence {
    pub fn open_default() -> Result<Self, AppPersistenceError> {
        let path = env::var_os(ENV_STATE_DB)
            .map(PathBuf::from)
            .unwrap_or(default_state_db_path()?);
        Self::open_path(path)
    }

    pub fn open_path(path: impl AsRef<Path>) -> Result<Self, AppPersistenceError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut store = SqliteStore::open(path)?;
        store.migrate()?;
        Ok(Self { store })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, AppPersistenceError> {
        let mut store = SqliteStore::open_memory()?;
        store.migrate()?;
        Ok(Self { store })
    }

    pub fn load_state(&mut self) -> Result<Option<BrowserState>, AppPersistenceError> {
        self.store.load_state()
    }
}

impl Persistence for AppPersistence {
    type Error = AppPersistenceError;

    fn commit(&mut self, state: &BrowserState) -> Result<(), Self::Error> {
        self.store.save_state(state)
    }
}

impl SqliteStore {
    fn open(path: &Path) -> Result<Self, AppPersistenceError> {
        let c_path = path_to_cstring(path)?;
        let mut db = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_open_v2(
                c_path.as_ptr(),
                &mut db,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
                std::ptr::null(),
            )
        };
        if rc != SQLITE_OK || db.is_null() {
            let message = if !db.is_null() {
                unsafe { sqlite_error_message(db) }
            } else {
                "failed to open sqlite database".to_owned()
            };
            if !db.is_null() {
                unsafe {
                    let _ = sqlite3_close(db);
                }
            }
            return Err(AppPersistenceError::Sqlite(message));
        }
        Ok(Self { db })
    }

    #[cfg(test)]
    fn open_memory() -> Result<Self, AppPersistenceError> {
        let c_memory = CString::new(":memory:")
            .map_err(|_| AppPersistenceError::InvalidData("invalid sqlite memory uri".to_owned()))?;
        let mut db = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_open_v2(
                c_memory.as_ptr(),
                &mut db,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
                std::ptr::null(),
            )
        };
        if rc != SQLITE_OK || db.is_null() {
            let message = if !db.is_null() {
                unsafe { sqlite_error_message(db) }
            } else {
                "failed to open sqlite in-memory database".to_owned()
            };
            if !db.is_null() {
                unsafe {
                    let _ = sqlite3_close(db);
                }
            }
            return Err(AppPersistenceError::Sqlite(message));
        }
        Ok(Self { db })
    }

    fn migrate(&mut self) -> Result<(), AppPersistenceError> {
        self.exec_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS profiles (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                active_workspace_id INTEGER
            );
            CREATE TABLE IF NOT EXISTS profile_workspace_order (
                profile_id INTEGER NOT NULL,
                position INTEGER NOT NULL,
                workspace_id INTEGER NOT NULL,
                PRIMARY KEY (profile_id, position)
            );
            CREATE TABLE IF NOT EXISTS workspaces (
                id INTEGER PRIMARY KEY,
                profile_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                active_tab_id INTEGER
            );
            CREATE TABLE IF NOT EXISTS workspace_tab_order (
                workspace_id INTEGER NOT NULL,
                position INTEGER NOT NULL,
                tab_id INTEGER NOT NULL,
                PRIMARY KEY (workspace_id, position)
            );
            CREATE TABLE IF NOT EXISTS tabs (
                id INTEGER PRIMARY KEY,
                profile_id INTEGER NOT NULL,
                workspace_id INTEGER NOT NULL,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                loading INTEGER NOT NULL,
                thumbnail_data_url TEXT,
                pinned INTEGER NOT NULL,
                muted INTEGER NOT NULL,
                runtime_state INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                bool_value INTEGER,
                int_value INTEGER,
                text_value TEXT
            );
            ",
        )?;
        self.exec_batch(&format!(
            "INSERT OR REPLACE INTO meta(key, value) VALUES({}, {});",
            sql_text_literal(META_SCHEMA_VERSION),
            sql_text_literal(&SCHEMA_VERSION.to_string()),
        ))?;
        Ok(())
    }

    fn save_state(&mut self, state: &BrowserState) -> Result<(), AppPersistenceError> {
        let mut sql = String::with_capacity(64 * 1024);
        sql.push_str("BEGIN IMMEDIATE;\n");
        sql.push_str(
            "
            DELETE FROM profile_workspace_order;
            DELETE FROM workspace_tab_order;
            DELETE FROM tabs;
            DELETE FROM workspaces;
            DELETE FROM profiles;
            DELETE FROM settings;
            ",
        );

        for profile in state.profiles.values() {
            sql.push_str(&format!(
                "INSERT INTO profiles(id, name, active_workspace_id) VALUES({}, {}, {});\n",
                profile.id.0,
                sql_text_literal(&profile.name),
                sql_opt_u64(profile.active_workspace_id.map(|id| id.0))
            ));
            for (position, workspace_id) in profile.workspace_order.iter().enumerate() {
                sql.push_str(&format!(
                    "INSERT INTO profile_workspace_order(profile_id, position, workspace_id) VALUES({}, {}, {});\n",
                    profile.id.0,
                    position,
                    workspace_id.0
                ));
            }
        }

        for workspace in state.workspaces.values() {
            sql.push_str(&format!(
                "INSERT INTO workspaces(id, profile_id, name, active_tab_id) VALUES({}, {}, {}, {});\n",
                workspace.id.0,
                workspace.profile_id.0,
                sql_text_literal(&workspace.name),
                sql_opt_u64(workspace.active_tab_id.map(|id| id.0))
            ));
            for (position, tab_id) in workspace.tab_order.iter().enumerate() {
                sql.push_str(&format!(
                    "INSERT INTO workspace_tab_order(workspace_id, position, tab_id) VALUES({}, {}, {});\n",
                    workspace.id.0,
                    position,
                    tab_id.0
                ));
            }
        }

        for tab in state.tabs.values() {
            sql.push_str(&format!(
                "INSERT INTO tabs(
                    id, profile_id, workspace_id, url, title, loading, thumbnail_data_url,
                    pinned, muted, runtime_state
                 ) VALUES({}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
                tab.id.0,
                tab.profile_id.0,
                tab.workspace_id.0,
                sql_text_literal(&tab.url),
                sql_text_literal(&tab.title),
                sql_bool(tab.loading),
                sql_opt_text(tab.thumbnail_data_url.as_deref()),
                sql_bool(tab.pinned),
                sql_bool(tab.muted),
                runtime_state_to_i64(tab.runtime_state)
            ));
        }

        for (key, value) in &state.settings {
            let (kind, bool_value, int_value, text_value) = match value {
                SettingValue::Bool(value) => {
                    ("bool", Some(if *value { 1_i64 } else { 0_i64 }), None, None)
                }
                SettingValue::Int(value) => ("int", None, Some(*value), None),
                SettingValue::Text(value) => ("text", None, None, Some(value.as_str())),
            };
            sql.push_str(&format!(
                "INSERT INTO settings(key, kind, bool_value, int_value, text_value) VALUES({}, {}, {}, {}, {});\n",
                sql_text_literal(key),
                sql_text_literal(kind),
                sql_opt_i64(bool_value),
                sql_opt_i64(int_value),
                sql_opt_text(text_value)
            ));
        }

        let active_profile_value = state
            .active_profile_id
            .map(|id| id.0.to_string())
            .unwrap_or_default();
        sql.push_str(&format!(
            "INSERT OR REPLACE INTO meta(key, value) VALUES({}, {});\n",
            sql_text_literal(META_ACTIVE_PROFILE_ID),
            sql_text_literal(&active_profile_value)
        ));
        sql.push_str(&format!(
            "INSERT OR REPLACE INTO meta(key, value) VALUES({}, {});\n",
            sql_text_literal(META_SCHEMA_VERSION),
            sql_text_literal(&SCHEMA_VERSION.to_string())
        ));
        sql.push_str("COMMIT;");
        self.exec_batch(&sql)
    }

    fn load_state(&mut self) -> Result<Option<BrowserState>, AppPersistenceError> {
        let profile_count_rows = self.query_rows("SELECT COUNT(*) FROM profiles;")?;
        let profile_count = profile_count_rows
            .first()
            .and_then(|row| row.first())
            .and_then(|cell| cell.as_deref())
            .map(|value| parse_i64(value, "profiles.count"))
            .transpose()?
            .unwrap_or(0);
        if profile_count <= 0 {
            return Ok(None);
        }

        let mut state = BrowserState::default();

        for row in self.query_rows("SELECT id, name, active_workspace_id FROM profiles ORDER BY id;")? {
            let id = ProfileId(parse_u64(required_cell(&row, 0, "profiles.id")?, "profiles.id")?);
            let name = required_cell(&row, 1, "profiles.name")?.to_owned();
            let active_workspace_id = optional_cell(&row, 2)
                .map(|value| parse_u64(value, "profiles.active_workspace_id"))
                .transpose()?
                .map(WorkspaceId);
            state.profiles.insert(
                id,
                Profile {
                    id,
                    name,
                    workspace_order: Vec::new(),
                    active_workspace_id,
                },
            );
        }

        for row in self.query_rows(
            "SELECT profile_id, workspace_id FROM profile_workspace_order ORDER BY profile_id, position;",
        )? {
            let profile_id = ProfileId(parse_u64(
                required_cell(&row, 0, "profile_workspace_order.profile_id")?,
                "profile_workspace_order.profile_id",
            )?);
            let workspace_id = WorkspaceId(parse_u64(
                required_cell(&row, 1, "profile_workspace_order.workspace_id")?,
                "profile_workspace_order.workspace_id",
            )?);
            if let Some(profile) = state.profiles.get_mut(&profile_id) {
                profile.workspace_order.push(workspace_id);
            }
        }

        for row in self.query_rows(
            "SELECT id, profile_id, name, active_tab_id FROM workspaces ORDER BY id;",
        )? {
            let id = WorkspaceId(parse_u64(required_cell(&row, 0, "workspaces.id")?, "workspaces.id")?);
            let profile_id = ProfileId(parse_u64(
                required_cell(&row, 1, "workspaces.profile_id")?,
                "workspaces.profile_id",
            )?);
            let name = required_cell(&row, 2, "workspaces.name")?.to_owned();
            let active_tab_id = optional_cell(&row, 3)
                .map(|value| parse_u64(value, "workspaces.active_tab_id"))
                .transpose()?
                .map(TabId);
            state.workspaces.insert(
                id,
                Workspace {
                    id,
                    profile_id,
                    name,
                    tab_order: Vec::new(),
                    active_tab_id,
                },
            );
        }

        for row in self.query_rows(
            "SELECT workspace_id, tab_id FROM workspace_tab_order ORDER BY workspace_id, position;",
        )? {
            let workspace_id = WorkspaceId(parse_u64(
                required_cell(&row, 0, "workspace_tab_order.workspace_id")?,
                "workspace_tab_order.workspace_id",
            )?);
            let tab_id = TabId(parse_u64(
                required_cell(&row, 1, "workspace_tab_order.tab_id")?,
                "workspace_tab_order.tab_id",
            )?);
            if let Some(workspace) = state.workspaces.get_mut(&workspace_id) {
                workspace.tab_order.push(tab_id);
            }
        }

        for row in self.query_rows(
            "SELECT
                id, profile_id, workspace_id, url, title, loading, thumbnail_data_url,
                pinned, muted, runtime_state
             FROM tabs
             ORDER BY id;",
        )? {
            let id = TabId(parse_u64(required_cell(&row, 0, "tabs.id")?, "tabs.id")?);
            let profile_id = ProfileId(parse_u64(
                required_cell(&row, 1, "tabs.profile_id")?,
                "tabs.profile_id",
            )?);
            let workspace_id = WorkspaceId(parse_u64(
                required_cell(&row, 2, "tabs.workspace_id")?,
                "tabs.workspace_id",
            )?);
            let runtime_state = runtime_state_from_i64(parse_i64(
                required_cell(&row, 9, "tabs.runtime_state")?,
                "tabs.runtime_state",
            )?)
            .ok_or_else(|| {
                AppPersistenceError::InvalidData(format!(
                    "unsupported tabs.runtime_state for tab {}",
                    id.0
                ))
            })?;

            state.tabs.insert(
                id,
                Tab {
                    id,
                    profile_id,
                    workspace_id,
                    url: required_cell(&row, 3, "tabs.url")?.to_owned(),
                    title: required_cell(&row, 4, "tabs.title")?.to_owned(),
                    loading: parse_i64(required_cell(&row, 5, "tabs.loading")?, "tabs.loading")? != 0,
                    thumbnail_data_url: optional_cell(&row, 6).map(ToOwned::to_owned),
                    pinned: parse_i64(required_cell(&row, 7, "tabs.pinned")?, "tabs.pinned")? != 0,
                    muted: parse_i64(required_cell(&row, 8, "tabs.muted")?, "tabs.muted")? != 0,
                    runtime_state,
                },
            );
        }

        for row in self.query_rows(
            "SELECT key, kind, bool_value, int_value, text_value FROM settings ORDER BY key;",
        )? {
            let key = required_cell(&row, 0, "settings.key")?.to_owned();
            let kind = required_cell(&row, 1, "settings.kind")?;
            let value = match kind {
                "bool" => SettingValue::Bool(
                    optional_cell(&row, 2)
                        .map(|cell| parse_i64(cell, "settings.bool_value"))
                        .transpose()?
                        .unwrap_or(0)
                        != 0,
                ),
                "int" => SettingValue::Int(
                    optional_cell(&row, 3)
                        .map(|cell| parse_i64(cell, "settings.int_value"))
                        .transpose()?
                        .unwrap_or(0),
                ),
                "text" => SettingValue::Text(optional_cell(&row, 4).unwrap_or("").to_owned()),
                _ => continue,
            };
            state.settings.insert(key, value);
        }

        let meta_rows = self.query_rows(&format!(
            "SELECT value FROM meta WHERE key = {};",
            sql_text_literal(META_ACTIVE_PROFILE_ID)
        ))?;
        if let Some(value) = meta_rows
            .first()
            .and_then(|row| row.first())
            .and_then(|cell| cell.as_deref())
            .map(str::trim)
        {
            if !value.is_empty() {
                state.active_profile_id =
                    Some(ProfileId(parse_u64(value, "meta.active_profile_id")?));
            }
        }

        normalize_loaded_state(&mut state);
        if state.profiles.is_empty() {
            return Ok(None);
        }
        Ok(Some(state))
    }

    fn exec_batch(&mut self, sql: &str) -> Result<(), AppPersistenceError> {
        let c_sql = CString::new(sql).map_err(|_| {
            AppPersistenceError::InvalidData("sql batch contained interior NUL byte".to_owned())
        })?;
        let mut err: *mut c_char = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_exec(
                self.db,
                c_sql.as_ptr(),
                None,
                std::ptr::null_mut(),
                &mut err,
            )
        };
        if rc != SQLITE_OK {
            let message = unsafe { sqlite_exec_error_message(self.db, err) };
            return Err(AppPersistenceError::Sqlite(message));
        }
        if !err.is_null() {
            unsafe { sqlite3_free(err as *mut c_void) };
        }
        Ok(())
    }

    fn query_rows(&mut self, sql: &str) -> Result<Vec<Vec<Option<String>>>, AppPersistenceError> {
        let c_sql = CString::new(sql).map_err(|_| {
            AppPersistenceError::InvalidData("sql query contained interior NUL byte".to_owned())
        })?;
        let mut rows = Vec::<Vec<Option<String>>>::new();
        let mut err: *mut c_char = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_exec(
                self.db,
                c_sql.as_ptr(),
                Some(collect_rows_callback),
                (&mut rows as *mut Vec<Vec<Option<String>>>).cast::<c_void>(),
                &mut err,
            )
        };
        if rc != SQLITE_OK {
            let message = unsafe { sqlite_exec_error_message(self.db, err) };
            return Err(AppPersistenceError::Sqlite(message));
        }
        if !err.is_null() {
            unsafe { sqlite3_free(err as *mut c_void) };
        }
        Ok(rows)
    }
}

impl Drop for SqliteStore {
    fn drop(&mut self) {
        if !self.db.is_null() {
            unsafe {
                let _ = sqlite3_close(self.db);
            }
            self.db = std::ptr::null_mut();
        }
    }
}

unsafe extern "C" fn collect_rows_callback(
    arg: *mut c_void,
    argc: c_int,
    argv: *mut *mut c_char,
    _col_names: *mut *mut c_char,
) -> c_int {
    if arg.is_null() {
        return 0;
    }
    let rows = &mut *(arg as *mut Vec<Vec<Option<String>>>);
    let mut row = Vec::with_capacity(argc as usize);
    for index in 0..argc {
        let value_ptr = *argv.add(index as usize);
        if value_ptr.is_null() {
            row.push(None);
        } else {
            let value = CStr::from_ptr(value_ptr).to_string_lossy().into_owned();
            row.push(Some(value));
        }
    }
    rows.push(row);
    0
}

unsafe fn sqlite_error_message(db: *mut sqlite3) -> String {
    if db.is_null() {
        return "sqlite operation failed".to_owned();
    }
    let ptr = sqlite3_errmsg(db);
    if ptr.is_null() {
        return "sqlite operation failed".to_owned();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

unsafe fn sqlite_exec_error_message(db: *mut sqlite3, err: *mut c_char) -> String {
    if !err.is_null() {
        let message = CStr::from_ptr(err).to_string_lossy().into_owned();
        sqlite3_free(err as *mut c_void);
        return message;
    }
    sqlite_error_message(db)
}

fn normalize_loaded_state(state: &mut BrowserState) {
    state
        .workspaces
        .retain(|_, workspace| state.profiles.contains_key(&workspace.profile_id));
    state.tabs.retain(|_, tab| {
        let Some(workspace) = state.workspaces.get(&tab.workspace_id) else {
            return false;
        };
        state.profiles.contains_key(&tab.profile_id) && workspace.profile_id == tab.profile_id
    });

    let profile_ids: Vec<ProfileId> = state.profiles.keys().copied().collect();
    for profile_id in profile_ids {
        if let Some(profile) = state.profiles.get_mut(&profile_id) {
            profile.workspace_order.retain(|workspace_id| {
                state
                    .workspaces
                    .get(workspace_id)
                    .map(|workspace| workspace.profile_id == profile_id)
                    .unwrap_or(false)
            });
        }
    }

    let workspace_ids: Vec<WorkspaceId> = state.workspaces.keys().copied().collect();
    for workspace_id in workspace_ids {
        let Some(workspace) = state.workspaces.get(&workspace_id).cloned() else {
            continue;
        };
        if let Some(profile) = state.profiles.get_mut(&workspace.profile_id) {
            if !profile.workspace_order.contains(&workspace_id) {
                profile.workspace_order.push(workspace_id);
            }
        }
    }

    let workspace_ids: Vec<WorkspaceId> = state.workspaces.keys().copied().collect();
    for workspace_id in workspace_ids {
        if let Some(workspace) = state.workspaces.get_mut(&workspace_id) {
            workspace.tab_order.retain(|tab_id| {
                state
                    .tabs
                    .get(tab_id)
                    .map(|tab| tab.workspace_id == workspace_id)
                    .unwrap_or(false)
            });
        }
    }

    let tab_ids: Vec<TabId> = state.tabs.keys().copied().collect();
    for tab_id in tab_ids {
        let Some(tab) = state.tabs.get(&tab_id).cloned() else {
            continue;
        };
        if let Some(workspace) = state.workspaces.get_mut(&tab.workspace_id) {
            if !workspace.tab_order.contains(&tab_id) {
                workspace.tab_order.push(tab_id);
            }
        }
    }

    for profile in state.profiles.values_mut() {
        if profile
            .active_workspace_id
            .map(|workspace_id| !profile.workspace_order.contains(&workspace_id))
            .unwrap_or(false)
        {
            profile.active_workspace_id = None;
        }
        if profile.active_workspace_id.is_none() {
            profile.active_workspace_id = profile.workspace_order.first().copied();
        }
    }

    for workspace in state.workspaces.values_mut() {
        if workspace
            .active_tab_id
            .map(|tab_id| !workspace.tab_order.contains(&tab_id))
            .unwrap_or(false)
        {
            workspace.active_tab_id = None;
        }
        if workspace.active_tab_id.is_none() {
            workspace.active_tab_id = workspace.tab_order.first().copied();
        }
    }

    if state
        .active_profile_id
        .map(|profile_id| !state.profiles.contains_key(&profile_id))
        .unwrap_or(true)
    {
        state.active_profile_id = state.profiles.keys().next().copied();
    }

    state.recompute_next_ids();
}

fn required_cell<'a>(
    row: &'a [Option<String>],
    index: usize,
    field: &str,
) -> Result<&'a str, AppPersistenceError> {
    row.get(index)
        .and_then(|value| value.as_deref())
        .ok_or_else(|| AppPersistenceError::InvalidData(format!("missing required {field}")))
}

fn optional_cell(row: &[Option<String>], index: usize) -> Option<&str> {
    row.get(index).and_then(|value| value.as_deref())
}

fn parse_u64(value: &str, field: &str) -> Result<u64, AppPersistenceError> {
    let parsed = value.parse::<i128>().map_err(|_| {
        AppPersistenceError::InvalidData(format!("{field} is not a valid integer: {value}"))
    })?;
    if parsed < 0 || parsed > u64::MAX as i128 {
        return Err(AppPersistenceError::InvalidData(format!(
            "{field} out of range for u64: {value}"
        )));
    }
    Ok(parsed as u64)
}

fn parse_i64(value: &str, field: &str) -> Result<i64, AppPersistenceError> {
    value.parse::<i64>().map_err(|_| {
        AppPersistenceError::InvalidData(format!("{field} is not a valid i64: {value}"))
    })
}

fn runtime_state_to_i64(state: TabRuntimeState) -> i64 {
    match state {
        TabRuntimeState::Active => 0,
        TabRuntimeState::Warm => 1,
        TabRuntimeState::Discarded => 2,
        TabRuntimeState::Restoring => 3,
    }
}

fn runtime_state_from_i64(value: i64) -> Option<TabRuntimeState> {
    match value {
        0 => Some(TabRuntimeState::Active),
        1 => Some(TabRuntimeState::Warm),
        2 => Some(TabRuntimeState::Discarded),
        3 => Some(TabRuntimeState::Restoring),
        _ => None,
    }
}

fn sql_bool(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

fn sql_opt_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NULL".to_owned())
}

fn sql_opt_i64(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NULL".to_owned())
}

fn sql_opt_text(value: Option<&str>) -> String {
    value
        .map(sql_text_literal)
        .unwrap_or_else(|| "NULL".to_owned())
}

fn sql_text_literal(value: &str) -> String {
    let sanitized = value.replace('\0', " ");
    let escaped = sanitized.replace('\'', "''");
    format!("'{escaped}'")
}

fn path_to_cstring(path: &Path) -> Result<CString, AppPersistenceError> {
    #[cfg(unix)]
    {
        return CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            AppPersistenceError::InvalidData(format!(
                "sqlite path contains interior NUL bytes: {}",
                path.display()
            ))
        });
    }

    #[cfg(not(unix))]
    {
        CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
            AppPersistenceError::InvalidData(format!(
                "sqlite path contains interior NUL bytes: {}",
                path.display()
            ))
        })
    }
}

fn default_state_db_path() -> Result<PathBuf, AppPersistenceError> {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            return Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Switchboard")
                .join("state.sqlite3"));
        }
    }

    let cwd = env::current_dir()?;
    Ok(cwd.join("target").join("switchboard-state.sqlite3"))
}

#[cfg(test)]
mod tests {
    use switchboard_core::WorkspaceId;

    use super::*;

    fn sample_state() -> BrowserState {
        let mut state = BrowserState::default();
        let profile_id = state.add_profile("Default");
        let workspace_id = state
            .add_workspace(profile_id, "Workspace 1")
            .expect("profile should exist");
        let tab_id = TabId(1);
        state.tabs.insert(
            tab_id,
            Tab {
                id: tab_id,
                profile_id,
                workspace_id,
                url: "https://example.com".to_owned(),
                title: "Example".to_owned(),
                loading: false,
                thumbnail_data_url: Some("data:image/svg+xml;utf8,test".to_owned()),
                pinned: true,
                muted: false,
                runtime_state: TabRuntimeState::Active,
            },
        );
        state
            .workspaces
            .get_mut(&workspace_id)
            .expect("workspace should exist")
            .tab_order
            .push(tab_id);
        state
            .workspaces
            .get_mut(&workspace_id)
            .expect("workspace should exist")
            .active_tab_id = Some(tab_id);
        state.active_profile_id = Some(profile_id);
        state.settings.insert(
            "homepage".to_owned(),
            SettingValue::Text("https://example.com".to_owned()),
        );
        state.settings.insert(
            "restore_last_session".to_owned(),
            SettingValue::Bool(true),
        );
        state.settings.insert(
            "warm_pool_budget".to_owned(),
            SettingValue::Int(8),
        );
        state.recompute_next_ids();
        state
    }

    #[test]
    fn sqlite_persistence_roundtrip() {
        let mut persistence = AppPersistence::open_in_memory().expect("open in-memory sqlite");
        let state = sample_state();

        persistence.commit(&state).expect("commit should succeed");
        let loaded = persistence
            .load_state()
            .expect("load should succeed")
            .expect("state should exist");

        assert_eq!(loaded.active_profile_id, state.active_profile_id);
        assert_eq!(loaded.profiles, state.profiles);
        assert_eq!(loaded.workspaces, state.workspaces);
        assert_eq!(loaded.tabs, state.tabs);
        assert_eq!(loaded.settings, state.settings);
        assert_eq!(
            loaded
                .active_workspace_id()
                .expect("active workspace should exist"),
            WorkspaceId(1)
        );
    }
}

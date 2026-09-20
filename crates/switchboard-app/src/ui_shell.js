const key = "switchboard.active_uri";
const BRIDGE_PROTOCOL_VERSION = 1;

const input = document.getElementById("url");
const backButton = document.getElementById("nav-back");
const forwardButton = document.getElementById("nav-forward");
const reloadButton = document.getElementById("nav-reload");
const profileMenuButton = document.getElementById("profile-menu-button");
const profileMenuLabel = document.getElementById("profile-menu-label");
const profileMenuPopover = document.getElementById("profile-menu-popover");
const profileMenuList = document.getElementById("profile-menu-list");
const profileRename = document.getElementById("profile-rename");
const profileDelete = document.getElementById("profile-delete");
const profileEditor = document.getElementById("profile-editor");
const profileEditorInput = document.getElementById("profile-editor-input");
const profileEditorSubmit = document.getElementById("profile-editor-submit");
const profileEditorCancel = document.getElementById("profile-editor-cancel");
const profileNew = document.getElementById("profile-new");
const workspaceList = document.getElementById("workspace-list");
const workspaceNew = document.getElementById("workspace-new");
const workspaceTitleWrap = document.getElementById("workspace-title-wrap");
const workspaceTitle = document.getElementById("workspace-title");
const workspaceTitleInput = document.getElementById("workspace-title-input");
const workspaceDelete = document.getElementById("workspace-delete");
const tabList = document.getElementById("tab-list");
const tabNew = document.getElementById("tab-new");
const archiveOpen = document.getElementById("archive-open");
const devtoolsToggle = document.getElementById("devtools-toggle");
const splitToggle = document.getElementById("split-toggle");
const splitRatio = document.getElementById("split-ratio");
const restorePlaceholder = document.getElementById("restore-placeholder");
const focusToggle = document.getElementById("focus-toggle");
const modeIndicator = document.getElementById("mode-indicator");
const settingsToggle = document.getElementById("settings-toggle");
const settingsBackdrop = document.getElementById("settings-backdrop");
const commandBackdrop = document.getElementById("command-backdrop");
const commandPanel = document.getElementById("command-panel");
const commandForm = document.getElementById("command-form");
const commandInput = document.getElementById("command-input");
const commandResults = document.getElementById("command-results");
const settingsPanel = document.getElementById("settings-panel");
const settingsClose = document.getElementById("settings-close");
const settingsSearchEngine = document.getElementById("settings-search-engine");
const settingsHomepage = document.getElementById("settings-homepage");
const settingsNewTabBehavior = document.getElementById("settings-new-tab-behavior");
const settingsCustomUrlField = document.getElementById("settings-custom-url-field");
const settingsCustomUrl = document.getElementById("settings-custom-url");
const settingsKeybindingCloseTab = document.getElementById("settings-keybinding-close-tab");
const settingsKeybindingCommand = document.getElementById("settings-keybinding-command");
const settingsKeybindingFocusNav = document.getElementById("settings-keybinding-focus-nav");
const settingsKeybindingDevTools = document.getElementById("settings-keybinding-devtools");

const TAB_ROW_HEIGHT = 56;
const TAB_OVERSCAN = 6;
const TAB_LIST_PADDING_Y = 8;
const SEARCH_ENGINE_SETTING_KEY = "search_engine";
const HOMEPAGE_SETTING_KEY = "homepage";
const NEW_TAB_BEHAVIOR_SETTING_KEY = "new_tab_behavior";
const NEW_TAB_CUSTOM_URL_SETTING_KEY = "new_tab_custom_url";
const KEYBINDING_CLOSE_TAB_SETTING_KEY = "keybinding_close_tab";
const KEYBINDING_COMMAND_PALETTE_SETTING_KEY = "keybinding_command_palette";
const KEYBINDING_FOCUS_NAVIGATION_SETTING_KEY = "keybinding_focus_navigation";
const KEYBINDING_TOGGLE_DEVTOOLS_SETTING_KEY = "keybinding_toggle_devtools";
const DEFAULT_SEARCH_ENGINE = "google";
const DEFAULT_HOMEPAGE = "https://youtube.com";
const DEFAULT_NEW_TAB_BEHAVIOR = "homepage";
const DEFAULT_NEW_TAB_CUSTOM_URL = "https://example.com";
const DEFAULT_KEYBINDING_CLOSE_TAB = "mod+w";
const DEFAULT_KEYBINDING_COMMAND_PALETTE = "space";
const DEFAULT_KEYBINDING_FOCUS_NAVIGATION = "mod+l";
const DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS = "mod+shift+i";
const SEARCH_ENGINE_URLS = Object.freeze({
  google: "https://www.google.com/search?q=%s",
  duckduckgo: "https://duckduckgo.com/?q=%s",
  bing: "https://www.bing.com/search?q=%s",
  brave: "https://search.brave.com/search?q=%s",
  kagi: "https://kagi.com/search?q=%s",
  startpage: "https://www.startpage.com/do/dsearch?query=%s",
});
const KEYBINDING_MODIFIERS = Object.freeze(["mod", "ctrl", "meta", "alt", "shift"]);
const KEYBINDING_SPECIAL_KEYS = new Set([
  "space",
  "enter",
  "escape",
  "tab",
  "backspace",
  "delete",
  "arrowup",
  "arrowdown",
  "arrowleft",
  "arrowright",
  "home",
  "end",
  "pageup",
  "pagedown",
]);

let activeUri = normalizeUrl(localStorage.getItem(key)) || "https://youtube.com";
let shellRevision = -1;
let shellState = null;
let editingWorkspaceId = null;
let editingWorkspaceOriginalName = "";
let pendingWorkspaceRenameId = null;
let virtualTabs = [];
let virtualActiveTabId = null;
let lastRenderedProfileId = null;
let lastRenderedWorkspaceId = null;
let lastRenderedTabId = null;
let virtualRenderPending = false;
let virtualDataEpoch = 0;
let virtualRenderKey = "";
let profileMenuOpen = false;
let profileEditorMode = null;
let profileEditorTargetId = null;
let settingsPanelOpen = false;
let commandPanelOpen = false;
let commandSearchResults = [];
let commandSelectedIndex = 0;
const collapsedTabIds = new Set();
let lazySearchCreatesTab = false;
let lazySearchParentTabId = null;
let keyboardMode = "browser";
let focusMode = false;
let snoozeTimer = null;
let uiOverlayVisible = false;
let websiteKeybindingsActive = false;
let hostKeybindingsSignature = "";
let bridgeRequestCounter = 0;
const bridgeQueue = [];
let bridgeSending = false;

function postBridgeCommand(command) {
  bridgeRequestCounter += 1;
  bridgeQueue.push({
    protocol_version: BRIDGE_PROTOCOL_VERSION,
    request_id: `ui-${Date.now()}-${bridgeRequestCounter}`,
    command,
  });
  flushBridgeQueue();
}

function flushBridgeQueue() {
  if (bridgeSending || bridgeQueue.length === 0) return;
  bridgeSending = true;
  const envelope = bridgeQueue.shift();
  const encoded = encodeURIComponent(JSON.stringify(envelope));
  window.location.hash = `bridge=${encoded}`;
  window.setTimeout(() => {
    window.history.replaceState(null, "", `${window.location.pathname}${window.location.search}`);
    bridgeSending = false;
    flushBridgeQueue();
  }, 0);
}

function parseLegacyCommand(payload) {
  const [name, ...parts] = String(payload || "").trim().split(/\s+/);
  const rest = String(payload || "").trim().slice(name.length).trim();
  const number = (value) => Number.parseInt(value, 10);
  switch (name) {
    case "ui_ready": return { type: "ui_ready", ui_version: rest || "0.1.0-dev", last_revision: shellRevision >= 0 ? shellRevision : null };
    case "navigate": return { type: "navigate_active", url: rest };
    case "devtools_toggle": return { type: "toggle_dev_tools" };
    case "switch_profile": return { type: "switch_profile", profile_id: number(parts[0]) };
    case "delete_profile": return { type: "delete_profile", profile_id: number(parts[0]) };
    case "rename_profile": return { type: "rename_profile", profile_id: number(parts[0]), name: parts.slice(1).join(" ") };
    case "new_profile": return { type: "new_profile", name: rest };
    case "new_workspace": return { type: "new_workspace", name: rest };
    case "rename_workspace": return { type: "rename_workspace", workspace_id: number(parts[0]), name: parts.slice(1).join(" ") };
    case "delete_workspace": return { type: "delete_workspace", workspace_id: number(parts[0]) };
    case "switch_workspace": return { type: "switch_workspace", workspace_id: number(parts[0]) };
    case "new_tab": return { type: "new_tab", workspace_id: number(parts[0]), url: null, make_active: true };
    case "activate_tab": return { type: "activate_tab", tab_id: number(parts[0]) };
    case "close_tab": return { type: "complete_tab", tab_id: number(parts[0]), completed_at_ms: Date.now() };
    case "setting_set_text": return { type: "setting_set", key: parts[0], value: parts.slice(1).join(" ") };
    case "ui_overlay": return { type: "set_ui_overlay", visible: parts[0] === "on" };
    case "website_keybindings": return { type: "set_browser_mode", active: parts[0] === "on" };
    case "keybindings_sync": return {
      type: "sync_keybindings",
      close_tab: parts[0],
      command_palette: parts[1],
      focus_navigation: parts[2],
      toggle_devtools: parts[3],
    };
    case "query_active_uri": return null;
    default: return null;
  }
}

function send(payload) {
  const command = typeof payload === "string" ? parseLegacyCommand(payload) : payload;
  if (command) postBridgeCommand(command);
  return "";
}

function normalizeUrl(value) {
  const raw = (value || "").trim();
  if (!raw) return "";
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(raw)) return raw;
  if (raw.includes("://")) return raw;
  return `https://${raw}`;
}

function normalizeSearchEngine(value) {
  const candidate = (value || "").trim().toLowerCase();
  return SEARCH_ENGINE_URLS[candidate] ? candidate : DEFAULT_SEARCH_ENGINE;
}

function normalizeNewTabBehavior(value) {
  const candidate = (value || "").trim().toLowerCase();
  if (
    candidate === "blank" ||
    candidate === "homepage" ||
    candidate === "custom" ||
    candidate === "workspace_default"
  ) {
    return candidate;
  }
  return DEFAULT_NEW_TAB_BEHAVIOR;
}

function activeProfileIdFromState(sourceState = shellState) {
  if (!sourceState) return null;
  const profileId = sourceState.active_profile_id;
  if (profileId === null || profileId === undefined) return null;
  return String(profileId);
}

function profileScopedSettingKey(prefix, sourceState = shellState) {
  const profileId = activeProfileIdFromState(sourceState);
  if (!profileId) return null;
  return `${prefix}${profileId}`;
}

function shellSettingText(keyName, fallback, sourceState = shellState) {
  if (!sourceState || !sourceState.settings || typeof sourceState.settings !== "object") {
    return fallback;
  }
  const value = sourceState.settings[keyName];
  return typeof value === "string" ? value : fallback;
}

function profileScopedSettingText(prefix, defaultKey, fallback, sourceState = shellState) {
  const profileKey = profileScopedSettingKey(prefix, sourceState);
  if (profileKey) {
    const profileValue = shellSettingText(profileKey, "", sourceState);
    if (profileValue) return profileValue;
  }
  return shellSettingText(defaultKey, fallback, sourceState);
}

function setLocalSettingValue(keyName, value) {
  if (!shellState || !shellState.settings || typeof shellState.settings !== "object") {
    return;
  }
  shellState.settings[keyName] = value;
}

function searchUrlForQuery(query) {
  const engine = normalizeSearchEngine(
    shellSettingText(SEARCH_ENGINE_SETTING_KEY, DEFAULT_SEARCH_ENGINE)
  );
  const template = SEARCH_ENGINE_URLS[engine] || SEARCH_ENGINE_URLS[DEFAULT_SEARCH_ENGINE];
  return template.replace("%s", encodeURIComponent(query));
}

function normalizeNavigationInput(value) {
  const raw = (value || "").trim();
  if (!raw) return "";

  if (raw.startsWith("http://") || raw.startsWith("https://")) {
    return raw;
  }

  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(raw)) {
    return searchUrlForQuery(raw);
  }

  if (/\s/.test(raw)) {
    return searchUrlForQuery(raw);
  }

  const lower = raw.toLowerCase();
  if (
    lower.startsWith("localhost") ||
    lower.startsWith("127.0.0.1") ||
    lower.startsWith("[::1]")
  ) {
    return `http://${raw}`;
  }

  if (raw.includes(".")) {
    return `https://${raw}`;
  }

  return searchUrlForQuery(raw);
}

function normalizeConfiguredUrl(value, fallback) {
  const raw = (value || "").trim();
  if (!raw) return fallback;
  if (raw.toLowerCase() === "about:blank") return "about:blank";
  if (raw.startsWith("http://") || raw.startsWith("https://")) return raw;
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(raw)) return fallback;
  return `https://${raw}`;
}

function normalizeKeyToken(value) {
  const raw = String(value || "").toLowerCase();
  if (raw === " " || raw === "spacebar") return "space";
  const token = raw.trim();
  if (!token) return "";
  if (token === "esc") return "escape";
  if (token === "return") return "enter";
  if (KEYBINDING_SPECIAL_KEYS.has(token)) return token;
  if (token.length === 1 && !/\s/.test(token)) return token;
  return "";
}

function parseKeybinding(value) {
  const raw = (value || "").trim().toLowerCase();
  if (!raw) return null;
  const parts = raw.split("+").map((part) => part.trim()).filter(Boolean);
  if (parts.length === 0) return null;

  const modifiers = new Set();
  let keyToken = "";
  for (const part of parts) {
    if (KEYBINDING_MODIFIERS.includes(part)) {
      modifiers.add(part);
      continue;
    }
    if (keyToken) return null;
    keyToken = normalizeKeyToken(part);
  }
  if (!keyToken) return null;

  const orderedModifiers = KEYBINDING_MODIFIERS.filter((modifier) =>
    modifiers.has(modifier)
  );
  return {
    key: keyToken,
    modifiers: orderedModifiers,
    normalized: [...orderedModifiers, keyToken].join("+"),
  };
}

function normalizeKeybinding(value, fallback) {
  const parsed = parseKeybinding(value);
  if (parsed) return parsed.normalized;
  const parsedFallback = parseKeybinding(fallback);
  return parsedFallback ? parsedFallback.normalized : "";
}

function eventKeyToken(event) {
  return normalizeKeyToken(event.key);
}

function keybindingMatchesEvent(binding, event) {
  const parsed = parseKeybinding(binding);
  if (!parsed) return false;
  const keyToken = eventKeyToken(event);
  if (!keyToken || keyToken !== parsed.key) return false;

  const modifiers = new Set(parsed.modifiers);
  const hasPrimary = event.metaKey || event.ctrlKey;
  if (modifiers.has("mod")) {
    if (!hasPrimary) return false;
  } else if (hasPrimary) {
    return false;
  }

  if (modifiers.has("ctrl")) {
    if (!event.ctrlKey) return false;
  } else if (!modifiers.has("mod") && event.ctrlKey) {
    return false;
  }

  if (modifiers.has("meta")) {
    if (!event.metaKey) return false;
  } else if (!modifiers.has("mod") && event.metaKey) {
    return false;
  }

  if (modifiers.has("alt")) {
    if (!event.altKey) return false;
  } else if (event.altKey) {
    return false;
  }

  if (modifiers.has("shift")) {
    if (!event.shiftKey) return false;
  } else if (event.shiftKey) {
    return false;
  }

  return true;
}

function keybindingSetting(settingKey, fallback) {
  return normalizeKeybinding(shellSettingText(settingKey, fallback), fallback);
}

function syncHostKeybindings(
  closeTabBinding,
  commandPaletteBinding,
  focusNavigationBinding,
  toggleDevToolsBinding
) {
  const signature = [
    closeTabBinding,
    commandPaletteBinding,
    focusNavigationBinding,
    toggleDevToolsBinding,
  ].join("|");
  if (signature === hostKeybindingsSignature) return;
  hostKeybindingsSignature = signature;
  send(
    `keybindings_sync ${closeTabBinding} ${commandPaletteBinding} ${focusNavigationBinding} ${toggleDevToolsBinding}`
  );
}

function renderUri() {
  input.value = activeUri;
  const activeTab = shellState ? deriveActiveContext(shellState).activeTab : null;
  backButton.disabled = !activeTab?.can_go_back;
  forwardButton.disabled = !activeTab?.can_go_forward;
}

function setActiveUri(next, pushHistory) {
  if (!next) return;
  if (next === activeUri) {
    renderUri();
    return;
  }
  activeUri = next;
  localStorage.setItem(key, activeUri);
  renderUri();
}

function navigateTo(next, pushHistory) {
  if (!next) return;
  setActiveUri(next, pushHistory);
  send(`navigate ${activeUri}`);
  queueStateRefresh();
}

function navigateFromInput() {
  const next = normalizeNavigationInput(input.value);
  if (!next) return;
  navigateTo(next, true);
}

function goBack() {
  postBridgeCommand({ type: "go_back" });
}

function goForward() {
  postBridgeCommand({ type: "go_forward" });
}

function reload() {
  postBridgeCommand({ type: "reload" });
}

function syncActiveUriFromHost() {
  if (document.hidden || document.activeElement === input || document.activeElement === commandInput) {
    return;
  }
  const activeTab = shellState ? deriveActiveContext(shellState).activeTab : null;
  const hostUri = normalizeUrl(activeTab?.url || "");
  if (!hostUri || hostUri === activeUri) return;
  setActiveUri(hostUri, true);
}

function parseShellState(raw) {
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch (_error) {
    return null;
  }
}

function upsertById(items, value) {
  const index = items.findIndex((item) => String(item.id) === String(value.id));
  if (index >= 0) items[index] = value;
  else items.push(value);
}

function applyPatchOperation(state, operation) {
  switch (operation.type) {
    case "upsert_profile":
      upsertById(state.profiles, operation);
      break;
    case "upsert_workspace":
      upsertById(state.workspaces, operation);
      break;
    case "upsert_tab":
      upsertById(state.tabs, operation);
      break;
    case "remove_profile":
      state.profiles = state.profiles.filter((item) => String(item.id) !== String(operation.profile_id));
      break;
    case "remove_workspace":
      state.workspaces = state.workspaces.filter((item) => String(item.id) !== String(operation.workspace_id));
      break;
    case "remove_tab":
      state.tabs = state.tabs.filter((item) => String(item.id) !== String(operation.tab_id));
      break;
    case "set_active_profile":
      state.active_profile_id = operation.profile_id;
      break;
    case "set_active_workspace": {
      const profile = state.profiles.find((item) => String(item.id) === String(operation.profile_id));
      if (profile) profile.active_workspace_id = operation.workspace_id;
      break;
    }
    case "set_active_tab": {
      const workspace = state.workspaces.find((item) => String(item.id) === String(operation.workspace_id));
      if (workspace) {
        workspace.active_tab_id = operation.tab_id;
        workspace.primary_tab_id = operation.tab_id;
      }
      break;
    }
    case "setting_changed":
      state.settings[operation.key] = operation.value;
      break;
    default:
      break;
  }
}

function receiveSnapshot(snapshot) {
  shellRevision = snapshot.revision;
  shellState = snapshot;
  renderShellState(snapshot);
  scheduleNextSnoozeWake();
  syncActiveUriFromHost();
}

function receivePatch(patch) {
  if (!shellState || patch.from_revision !== shellRevision) {
    postBridgeCommand({ type: "request_resync", last_revision: shellRevision >= 0 ? shellRevision : null });
    return;
  }
  for (const operation of patch.ops || []) applyPatchOperation(shellState, operation);
  shellRevision = patch.to_revision;
  shellState.revision = shellRevision;
  renderShellState(shellState);
  scheduleNextSnoozeWake();
  syncActiveUriFromHost();
}

window.switchboardReceive = (payload) => {
  let envelope;
  try {
    envelope = typeof payload === "string" ? JSON.parse(payload) : payload;
  } catch (_error) {
    return;
  }
  if (!envelope || envelope.protocol_version !== BRIDGE_PROTOCOL_VERSION) return;
  const message = envelope.message || {};
  if (message.type === "snapshot" && message.snapshot) {
    receiveSnapshot(message.snapshot);
  } else if (message.type === "patch" && message.patch) {
    receivePatch(message.patch);
  } else if (message.type === "restore_requested") {
    const tabId = Number(message.tab_id);
    const generation = Number(message.generation);
    if (Number.isFinite(tabId) && Number.isFinite(generation)) {
      window.requestAnimationFrame(() => {
        postBridgeCommand({ type: "frame_committed", tab_id: tabId, generation });
      });
    }
  } else if (message.type === "active_uri" && message.url) {
    setActiveUri(message.url, false);
  } else if (message.type === "search_results") {
    renderSearchResults(message.results || []);
  }
};

function scheduleNextSnoozeWake() {
  if (snoozeTimer !== null) {
    window.clearTimeout(snoozeTimer);
    snoozeTimer = null;
  }
  const wakeTimes = (shellState?.tabs || [])
    .filter((tab) => tab.status?.kind === "snoozed")
    .map((tab) => Number(tab.status.wake_at_ms))
    .filter(Number.isFinite)
    .sort((left, right) => left - right);
  if (wakeTimes.length === 0) return;
  const delay = Math.max(0, Math.min(wakeTimes[0] - Date.now(), 2_147_000_000));
  snoozeTimer = window.setTimeout(() => {
    snoozeTimer = null;
    postBridgeCommand({ type: "wake_snoozed", now_ms: Date.now() });
  }, delay);
}

function syncSettingsControlsFromState(state) {
  if (!state || !state.settings || typeof state.settings !== "object") return;

  const searchEngine = normalizeSearchEngine(
    shellSettingText(SEARCH_ENGINE_SETTING_KEY, DEFAULT_SEARCH_ENGINE, state)
  );
  if (settingsSearchEngine.value !== searchEngine) {
    settingsSearchEngine.value = searchEngine;
  }

  const homepage = normalizeConfiguredUrl(
    shellSettingText(HOMEPAGE_SETTING_KEY, DEFAULT_HOMEPAGE, state),
    DEFAULT_HOMEPAGE
  );
  if (document.activeElement !== settingsHomepage && settingsHomepage.value !== homepage) {
    settingsHomepage.value = homepage;
  }

  const behavior = normalizeNewTabBehavior(
    shellSettingText(NEW_TAB_BEHAVIOR_SETTING_KEY, DEFAULT_NEW_TAB_BEHAVIOR, state)
  );
  if (settingsNewTabBehavior.value !== behavior) {
    settingsNewTabBehavior.value = behavior;
  }

  const customUrl = normalizeConfiguredUrl(
    shellSettingText(NEW_TAB_CUSTOM_URL_SETTING_KEY, DEFAULT_NEW_TAB_CUSTOM_URL, state),
    DEFAULT_NEW_TAB_CUSTOM_URL
  );
  if (document.activeElement !== settingsCustomUrl && settingsCustomUrl.value !== customUrl) {
    settingsCustomUrl.value = customUrl;
  }

  const keybindingCloseTab = normalizeKeybinding(
    shellSettingText(
      KEYBINDING_CLOSE_TAB_SETTING_KEY,
      DEFAULT_KEYBINDING_CLOSE_TAB,
      state
    ),
    DEFAULT_KEYBINDING_CLOSE_TAB
  );
  if (
    document.activeElement !== settingsKeybindingCloseTab &&
    settingsKeybindingCloseTab.value !== keybindingCloseTab
  ) {
    settingsKeybindingCloseTab.value = keybindingCloseTab;
  }

  const keybindingCommand = normalizeKeybinding(
    shellSettingText(
      KEYBINDING_COMMAND_PALETTE_SETTING_KEY,
      DEFAULT_KEYBINDING_COMMAND_PALETTE,
      state
    ),
    DEFAULT_KEYBINDING_COMMAND_PALETTE
  );
  if (
    document.activeElement !== settingsKeybindingCommand &&
    settingsKeybindingCommand.value !== keybindingCommand
  ) {
    settingsKeybindingCommand.value = keybindingCommand;
  }

  const keybindingFocusNav = normalizeKeybinding(
    shellSettingText(
      KEYBINDING_FOCUS_NAVIGATION_SETTING_KEY,
      DEFAULT_KEYBINDING_FOCUS_NAVIGATION,
      state
    ),
    DEFAULT_KEYBINDING_FOCUS_NAVIGATION
  );
  if (
    document.activeElement !== settingsKeybindingFocusNav &&
    settingsKeybindingFocusNav.value !== keybindingFocusNav
  ) {
    settingsKeybindingFocusNav.value = keybindingFocusNav;
  }

  const keybindingDevTools = normalizeKeybinding(
    shellSettingText(
      KEYBINDING_TOGGLE_DEVTOOLS_SETTING_KEY,
      DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS,
      state
    ),
    DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS
  );
  if (
    document.activeElement !== settingsKeybindingDevTools &&
    settingsKeybindingDevTools.value !== keybindingDevTools
  ) {
    settingsKeybindingDevTools.value = keybindingDevTools;
  }
  syncHostKeybindings(
    keybindingCloseTab,
    keybindingCommand,
    keybindingFocusNav,
    keybindingDevTools
  );

  settingsCustomUrlField.hidden = behavior !== "custom";
}

function commitTextSetting(keyName, value) {
  const sanitized = (value || "").replace(/\r?\n/g, " ").trim();
  if (!sanitized) return;
  const current = shellSettingText(keyName, "");
  if (sanitized === current) return;
  setLocalSettingValue(keyName, sanitized);
  send(`setting_set_text ${keyName} ${sanitized}`);
  queueStateRefresh();
}

function commitHomepageSetting() {
  const normalized = normalizeConfiguredUrl(settingsHomepage.value, DEFAULT_HOMEPAGE);
  settingsHomepage.value = normalized;
  commitTextSetting(HOMEPAGE_SETTING_KEY, normalized);
}

function commitCustomNewTabUrlSetting() {
  const normalized = normalizeConfiguredUrl(
    settingsCustomUrl.value,
    DEFAULT_NEW_TAB_CUSTOM_URL
  );
  settingsCustomUrl.value = normalized;
  commitTextSetting(NEW_TAB_CUSTOM_URL_SETTING_KEY, normalized);
}

function commitKeybindingSetting(inputElement, settingKey, fallback) {
  const normalized = normalizeKeybinding(inputElement.value, fallback);
  inputElement.value = normalized;
  commitTextSetting(settingKey, normalized);
}

function syncUiOverlayVisibility() {
  const nextVisible = settingsPanelOpen || commandPanelOpen;
  if (nextVisible === uiOverlayVisible) return;
  uiOverlayVisible = nextVisible;
  send(nextVisible ? "ui_overlay on" : "ui_overlay off");
  syncWebsiteKeybindingsActive();
}

function setWebsiteKeybindingsActive(active) {
  if (websiteKeybindingsActive === active) return;
  websiteKeybindingsActive = active;
  send(active ? "website_keybindings on" : "website_keybindings off");
}

function syncWebsiteKeybindingsActive() {
  if (uiOverlayVisible || settingsPanelOpen || commandPanelOpen || document.hidden) {
    setWebsiteKeybindingsActive(false);
    return;
  }
  if (!document.hasFocus()) {
    setWebsiteKeybindingsActive(true);
    return;
  }
  const activeElement = document.activeElement;
  if (
    !activeElement ||
    activeElement === document.body ||
    activeElement === document.documentElement
  ) {
    setWebsiteKeybindingsActive(true);
    return;
  }
  setWebsiteKeybindingsActive(false);
}

function openSettingsPanel() {
  if (settingsPanelOpen) return;
  settingsPanelOpen = true;
  settingsBackdrop.hidden = false;
  settingsPanel.hidden = false;
  settingsToggle.setAttribute("aria-expanded", "true");
  syncSettingsControlsFromState(shellState);
  syncUiOverlayVisibility();
}

function closeSettingsPanel() {
  if (!settingsPanelOpen) return;
  settingsPanelOpen = false;
  settingsBackdrop.hidden = true;
  settingsPanel.hidden = true;
  settingsToggle.setAttribute("aria-expanded", "false");
  syncUiOverlayVisibility();
}

function toggleSettingsPanel() {
  if (settingsPanelOpen) {
    closeSettingsPanel();
  } else {
    openSettingsPanel();
  }
}

function requestLazySearch() {
  postBridgeCommand({ type: "search", query: commandInput.value, limit: 30 });
}

function renderSearchResults(results) {
  commandSearchResults = Array.isArray(results) ? results : [];
  commandSelectedIndex = Math.max(0, Math.min(commandSelectedIndex, commandSearchResults.length - 1));
  const fragment = document.createDocumentFragment();
  commandSearchResults.forEach((result, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `command-result${index === commandSelectedIndex ? " selected" : ""}`;
    button.dataset.index = String(index);
    button.setAttribute("role", "option");
    button.setAttribute("aria-selected", index === commandSelectedIndex ? "true" : "false");
    const title = document.createElement("span");
    title.className = "command-result-title";
    title.textContent = result.title || result.url || "Untitled";
    const kind = document.createElement("span");
    kind.className = "command-result-kind";
    kind.textContent = String(result.kind || "").replaceAll("_", " ");
    const subtitle = document.createElement("span");
    subtitle.className = "command-result-subtitle";
    subtitle.textContent = result.subtitle || "";
    button.append(title, kind, subtitle);
    button.addEventListener("click", () => executeSearchResult(result));
    if (result.kind === "archived_tab" && result.tab_id) {
      button.title = "Click to restore · Right-click to permanently delete";
      button.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        if (window.confirm(`Permanently delete “${result.title || "this tab"}”?`)) {
          postBridgeCommand({ type: "permanently_delete_tab", tab_id: Number(result.tab_id) });
          requestLazySearch();
        }
      });
    }
    fragment.appendChild(button);
  });
  commandResults.replaceChildren(fragment);
}

function executeSearchCommand(command) {
  switch (command) {
    case "new_workspace": createWorkspace(); break;
    case "show_archive": openCommandPanel({ initial: "done:", createsTab: false }); break;
    case "toggle_split": toggleSplitView(); break;
    case "toggle_focus": toggleFocusMode(); break;
    case "open_settings": openSettingsPanel(); break;
    case "clear_profile_history": {
      const profileId = activeProfileIdFromState();
      if (profileId) postBridgeCommand({ type: "clear_profile_history", profile_id: Number(profileId) });
      break;
    }
    case "clear_archive": {
      const profileId = activeProfileIdFromState();
      if (profileId && window.confirm("Permanently delete all unlocked completed tabs in this profile?")) {
        postBridgeCommand({ type: "clear_archive", profile_id: Number(profileId) });
      }
      break;
    }
    default: break;
  }
}

function executeSearchResult(result) {
  if (!result) return;
  closeCommandPanel();
  if (result.kind === "command") {
    executeSearchCommand(result.command);
    return;
  }
  if (result.kind === "open_tab" && result.tab_id) {
    postBridgeCommand({ type: "activate_tab", tab_id: Number(result.tab_id) });
    return;
  }
  if (result.kind === "archived_tab" && result.tab_id) {
    postBridgeCommand({ type: "restore_tab", tab_id: Number(result.tab_id) });
    postBridgeCommand({ type: "activate_tab", tab_id: Number(result.tab_id) });
    return;
  }
  if (result.url) {
    const { activeWorkspace } = deriveActiveContext(shellState || {});
    if (lazySearchParentTabId && activeWorkspace) {
      postBridgeCommand({
        type: "new_subtab",
        workspace_id: Number(activeWorkspace.id),
        parent_tab_id: Number(lazySearchParentTabId),
        url: result.url,
        make_active: true,
      });
    } else if (lazySearchCreatesTab && activeWorkspace) {
      postBridgeCommand({
        type: "new_tab",
        workspace_id: Number(activeWorkspace.id),
        url: result.url,
        make_active: true,
      });
    } else {
      postBridgeCommand({ type: "navigate_active", url: result.url });
    }
  }
}

function openCommandPanel(options = {}) {
  if (commandPanelOpen) return;
  if (settingsPanelOpen) closeSettingsPanel();
  closeProfileMenu();
  lazySearchCreatesTab = Boolean(options.createsTab);
  lazySearchParentTabId = options.parentTabId || null;
  commandPanelOpen = true;
  commandBackdrop.hidden = false;
  commandPanel.hidden = false;
  commandInput.value = options.initial || "";
  commandSearchResults = [];
  commandSelectedIndex = 0;
  commandResults.replaceChildren();
  syncUiOverlayVisibility();
  window.requestAnimationFrame(() => {
    commandInput.focus();
    requestLazySearch();
  });
}

function closeCommandPanel() {
  if (!commandPanelOpen) return;
  commandPanelOpen = false;
  commandBackdrop.hidden = true;
  commandPanel.hidden = true;
  syncUiOverlayVisibility();
}

function navigateFromCommandPanel() {
  const selected = commandSearchResults[commandSelectedIndex];
  if (selected) {
    executeSearchResult(selected);
    return;
  }
  const next = normalizeNavigationInput(commandInput.value);
  if (!next) return;
  const { activeWorkspace } = deriveActiveContext(shellState || {});
  closeCommandPanel();
  if (lazySearchParentTabId && activeWorkspace) {
    postBridgeCommand({
      type: "new_subtab",
      workspace_id: Number(activeWorkspace.id),
      parent_tab_id: Number(lazySearchParentTabId),
      url: next,
      make_active: true,
    });
  } else if (lazySearchCreatesTab && activeWorkspace) {
    postBridgeCommand({ type: "new_tab", workspace_id: Number(activeWorkspace.id), url: next, make_active: true });
  } else {
    postBridgeCommand({ type: "navigate_active", url: next });
  }
}

function focusTopNavigationInput() {
  closeCommandPanel();
  input.focus();
  input.select();
  syncWebsiteKeybindingsActive();
}

function toggleDevTools() {
  send("devtools_toggle");
}

function workspaceBadge(name) {
  const trimmed = (name || "").trim();
  if (!trimmed) return "W";
  return trimmed.slice(0, 1).toUpperCase();
}

function profileBadge(name, fallbackId) {
  const trimmed = (name || "").trim();
  if (trimmed) return trimmed.slice(0, 1).toUpperCase();
  if (fallbackId !== undefined && fallbackId !== null) return String(fallbackId).slice(0, 1).toUpperCase();
  return "P";
}

function profileDisplayName(profile) {
  const trimmed = (profile?.name || "").trim();
  if (trimmed) return trimmed;
  if (profile && profile.id !== undefined && profile.id !== null) {
    return `Profile ${profile.id}`;
  }
  return "Profile";
}

function isTextInputTarget(target) {
  if (!target) return false;
  if (target instanceof HTMLInputElement) return true;
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  return false;
}

function shouldIgnoreGlobalShortcutTarget(target) {
  if (!target) return false;
  if (isTextInputTarget(target)) return true;
  if (!(target instanceof HTMLElement)) return false;
  if (target.closest("#settings-panel")) return true;
  if (target.closest(".profile-menu")) return true;
  return false;
}

function closeActiveTabFromShortcut() {
  if (!shellState) return;
  const { activeTab } = deriveActiveContext(shellState);
  if (!activeTab) return;
  send(`close_tab ${activeTab.id}`);
  queueStateRefresh();
}

function profileMenuItems() {
  return Array.from(profileMenuList.querySelectorAll(".profile-menu-item"));
}

function focusProfileMenuItem(index) {
  const items = profileMenuItems();
  if (items.length === 0) return;
  const normalized = Math.max(0, Math.min(index, items.length - 1));
  items[normalized].focus();
}

function focusNextProfileMenuItem(step) {
  const items = profileMenuItems();
  if (items.length === 0) return;
  const active = document.activeElement;
  const currentIndex = items.indexOf(active);
  const startIndex = currentIndex === -1 ? 0 : currentIndex;
  const nextIndex = (startIndex + step + items.length) % items.length;
  items[nextIndex].focus();
}

function tabLabel(tab) {
  if (tab.title && tab.title.trim()) return tab.title.trim();
  if (tab.url) {
    try {
      const parsed = new URL(tab.url);
      if (parsed.hostname) return parsed.hostname;
    } catch (_error) {}
    return tab.url;
  }
  return "New Tab";
}

function deriveActiveContext(state) {
  const profiles = new Map((state.profiles || []).map((profile) => [profile.id, profile]));
  const workspaces = new Map((state.workspaces || []).map((workspace) => [workspace.id, workspace]));
  const tabs = new Map((state.tabs || []).map((tab) => [tab.id, tab]));

  const activeProfile = profiles.get(state.active_profile_id) || null;
  const orderedWorkspaces = activeProfile
    ? (activeProfile.workspace_order || []).map((id) => workspaces.get(id)).filter(Boolean)
    : [];
  const activeWorkspace = activeProfile
    ? workspaces.get(activeProfile.active_workspace_id) || null
    : null;
  const orderedTabs = activeWorkspace
    ? (activeWorkspace.tab_order || []).map((id) => tabs.get(id)).filter(Boolean)
    : [];
  const activeTab = activeWorkspace ? tabs.get(activeWorkspace.active_tab_id) || null : null;

  return {
    orderedProfiles: state.profiles || [],
    activeProfile,
    activeWorkspace,
    activeTab,
    orderedWorkspaces,
    orderedTabs,
  };
}

function renderProfileControls(orderedProfiles, activeProfile) {
  profileMenuList.innerHTML = "";
  const activeProfileId = activeProfile ? activeProfile.id : null;
  let activeProfileName = "";
  let activeProfileBadge = "P";
  orderedProfiles.forEach((profile) => {
    const displayName = profileDisplayName(profile);
    const option = document.createElement("button");
    option.type = "button";
    option.className = "profile-menu-item";
    option.dataset.profileId = String(profile.id);
    option.title = displayName;

    const badge = document.createElement("span");
    badge.className = "profile-menu-item-badge";
    badge.textContent = profileBadge(profile.name, profile.id);
    option.appendChild(badge);

    const name = document.createElement("span");
    name.className = "profile-menu-item-name";
    name.textContent = displayName;
    option.appendChild(name);

    if (profile.id === activeProfileId) {
      option.classList.add("active");
      activeProfileName = displayName;
      activeProfileBadge = profileBadge(profile.name, profile.id);
    }
    profileMenuList.appendChild(option);
  });
  if (orderedProfiles.length === 0) {
    const empty = document.createElement("div");
    empty.className = "profile-menu-empty";
    empty.textContent = "No profiles";
    profileMenuList.appendChild(empty);
  }
  profileMenuButton.disabled = orderedProfiles.length === 0;
  profileMenuButton.title = activeProfileName || "No active profile";
  profileMenuLabel.textContent = activeProfileBadge;
  profileRename.disabled = activeProfileId === null;
  profileDelete.disabled = activeProfileId === null || orderedProfiles.length <= 1;
  profileRename.title = activeProfileName
    ? `Rename "${activeProfileName}"`
    : "Rename profile";
  profileDelete.title = activeProfileName
    ? `Delete "${activeProfileName}"`
    : "Delete profile";
  if (profileMenuOpen && activeProfileId === null) {
    closeProfileMenu();
  }
  if (profileEditorMode === "rename" && activeProfileId === null) {
    closeProfileEditor();
  }
}

function renderWorkspaceRail(orderedWorkspaces, activeWorkspaceId) {
  workspaceList.innerHTML = "";
  orderedWorkspaces.forEach((workspace) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "workspace-item";
    if (workspace.id === activeWorkspaceId) {
      button.classList.add("active");
    }
    button.dataset.workspaceId = String(workspace.id);
    button.title = workspace.name;
    button.textContent = workspaceBadge(workspace.name);
    workspaceList.appendChild(button);
  });
}

function tabDepth(tab) {
  if (!shellState || !tab?.parent_tab_id) return 0;
  const tabs = new Map((shellState.tabs || []).map((candidate) => [String(candidate.id), candidate]));
  const seen = new Set();
  let parentId = tab.parent_tab_id;
  let depth = 0;
  while (parentId && depth < 12 && !seen.has(String(parentId))) {
    seen.add(String(parentId));
    const parent = tabs.get(String(parentId));
    if (!parent) break;
    depth += 1;
    parentId = parent.parent_tab_id;
  }
  return depth;
}

function visibleTreeTabs(orderedTabs) {
  const byId = new Map(orderedTabs.map((tab) => [String(tab.id), tab]));
  return orderedTabs.filter((tab) => {
    const seen = new Set();
    let parentId = tab.parent_tab_id;
    while (parentId && !seen.has(String(parentId))) {
      if (collapsedTabIds.has(String(parentId))) return false;
      seen.add(String(parentId));
      parentId = byId.get(String(parentId))?.parent_tab_id;
    }
    return true;
  });
}

function toggleTabCollapsed(tabId) {
  const key = String(tabId);
  if (collapsedTabIds.has(key)) collapsedTabIds.delete(key);
  else collapsedTabIds.add(key);
  if (shellState) renderShellState(shellState);
}

function createTabButton(tab, isActive) {
  const label = tabLabel(tab);
  const button = document.createElement("button");
  button.type = "button";
  button.className = "tab-item";
  button.dataset.tabId = String(tab.id);
  button.dataset.tabUrl = tab.url || "";
  button.draggable = true;
  button.style.paddingLeft = `${8 + tabDepth(tab) * 16}px`;
  if (isActive) {
    button.classList.add("active");
  }
  if (tab.loading) {
    button.classList.add("loading");
  }

  const hasChildren = (shellState?.tabs || []).some(
    (candidate) => String(candidate.parent_tab_id || "") === String(tab.id)
      && candidate.status?.kind === "open"
  );
  const disclosure = document.createElement("span");
  disclosure.className = `tab-disclosure${hasChildren ? "" : " empty"}`;
  disclosure.textContent = collapsedTabIds.has(String(tab.id)) ? "▸" : "▾";
  disclosure.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (hasChildren) toggleTabCollapsed(tab.id);
  });
  button.appendChild(disclosure);

  const icon = document.createElement("span");
  icon.className = "tab-icon";
  icon.textContent = label.slice(0, 1).toUpperCase();
  button.appendChild(icon);

  const content = document.createElement("span");
  content.className = "tab-copy";

  const title = document.createElement("span");
  title.className = "tab-title";
  title.textContent = `${tab.pinned ? "● " : ""}${tab.locked ? "🔒 " : ""}${label}`;
  content.appendChild(title);

  const url = document.createElement("span");
  url.className = "tab-url";
  url.textContent = tab.url || "about:blank";
  content.appendChild(url);

  button.appendChild(content);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "tab-close";
  close.dataset.tabId = String(tab.id);
  close.setAttribute("aria-label", "Close tab");
  close.title = "Close tab";
  close.textContent = "×";
  close.disabled = Boolean(tab.locked);
  button.appendChild(close);

  return button;
}

function setVirtualTabState(orderedTabs, activeTabId) {
  virtualTabs = orderedTabs;
  virtualActiveTabId = activeTabId;
  virtualDataEpoch += 1;
  virtualRenderKey = "";
}

function computeTabListHeight(totalRows) {
  const pane = tabList.closest(".tab-pane");
  const header = pane ? pane.querySelector(".tab-pane-header") : null;
  if (!pane || !header) {
    return Math.min(totalRows * TAB_ROW_HEIGHT + TAB_LIST_PADDING_Y * 2, 320);
  }
  const footerStyle = getComputedStyle(tabNew);
  const footerMargins =
    (Number.parseFloat(footerStyle.marginTop) || 0) +
    (Number.parseFloat(footerStyle.marginBottom) || 0);
  const footerHeight = tabNew.offsetHeight + footerMargins;
  const available = pane.clientHeight - header.offsetHeight - footerHeight;
  const minHeight = TAB_ROW_HEIGHT + TAB_LIST_PADDING_Y * 2;
  const maxHeight = Math.max(minHeight, available);
  const desired = totalRows * TAB_ROW_HEIGHT + TAB_LIST_PADDING_Y * 2;
  return Math.min(maxHeight, desired);
}

function scheduleVirtualTabListRender() {
  if (virtualRenderPending) return;
  virtualRenderPending = true;
  window.requestAnimationFrame(() => {
    virtualRenderPending = false;
    renderVirtualTabList();
  });
}

function computeVirtualRange(totalRows, scrollTop, viewportHeight) {
  const start = Math.max(0, Math.floor(scrollTop / TAB_ROW_HEIGHT) - TAB_OVERSCAN);
  const end = Math.min(
    totalRows,
    Math.ceil((scrollTop + viewportHeight) / TAB_ROW_HEIGHT) + TAB_OVERSCAN
  );
  return { start, end };
}

function renderVirtualTabList() {
  const totalRows = virtualTabs.length;
  if (totalRows === 0) {
    const emptyKey = `empty:${virtualDataEpoch}`;
    if (virtualRenderKey === emptyKey) return;
    virtualRenderKey = emptyKey;
    tabList.style.height = "auto";
    tabList.scrollTop = 0;
    const empty = document.createElement("div");
    empty.className = "tab-empty";
    empty.textContent = "No tabs yet.";
    tabList.replaceChildren(empty);
    return;
  }

  const listHeight = Math.round(computeTabListHeight(totalRows));
  tabList.style.height = `${listHeight}px`;

  const viewportHeight = Math.max(1, tabList.clientHeight - TAB_LIST_PADDING_Y * 2);
  const maxScrollTop = Math.max(0, totalRows * TAB_ROW_HEIGHT - viewportHeight);
  if (tabList.scrollTop > maxScrollTop) {
    tabList.scrollTop = maxScrollTop;
  }

  const scrollTop = tabList.scrollTop;
  const { start, end } = computeVirtualRange(totalRows, scrollTop, viewportHeight);
  const nextRenderKey = [
    virtualDataEpoch,
    totalRows,
    listHeight,
    start,
    end,
    virtualActiveTabId ?? "none",
  ].join(":");
  if (virtualRenderKey === nextRenderKey) return;
  virtualRenderKey = nextRenderKey;

  const fragment = document.createDocumentFragment();

  if (start > 0) {
    const topSpacer = document.createElement("div");
    topSpacer.className = "tab-spacer";
    topSpacer.style.height = `${start * TAB_ROW_HEIGHT}px`;
    fragment.appendChild(topSpacer);
  }

  for (let index = start; index < end; index += 1) {
    const tab = virtualTabs[index];
    const button = createTabButton(tab, tab.id === virtualActiveTabId);
    fragment.appendChild(button);
  }

  if (end < totalRows) {
    const bottomSpacer = document.createElement("div");
    bottomSpacer.className = "tab-spacer";
    bottomSpacer.style.height = `${(totalRows - end) * TAB_ROW_HEIGHT}px`;
    fragment.appendChild(bottomSpacer);
  }

  tabList.replaceChildren(fragment);
}

function renderShellState(state) {
  const {
    orderedProfiles,
    activeProfile,
    activeWorkspace,
    activeTab,
    orderedWorkspaces,
    orderedTabs,
  } = deriveActiveContext(state);
  const activeProfileId = activeProfile ? activeProfile.id : null;
  const activeWorkspaceId = activeWorkspace ? activeWorkspace.id : null;
  const activeTabId = activeWorkspace ? activeWorkspace.active_tab_id : null;
  const renameSyncResolved =
    pendingWorkspaceRenameId !== null &&
    activeWorkspaceId === pendingWorkspaceRenameId;

  if (editingWorkspaceId !== null && editingWorkspaceId !== activeWorkspaceId) {
    cancelWorkspaceRename();
  }
  if (!workspaceTitleWrap.classList.contains("editing") || renameSyncResolved) {
    workspaceTitle.textContent = activeWorkspace ? activeWorkspace.name : "No Workspace";
  }
  if (renameSyncResolved) {
    pendingWorkspaceRenameId = null;
  }

  renderProfileControls(orderedProfiles, activeProfile);
  renderWorkspaceRail(orderedWorkspaces, activeWorkspaceId);
  if (
    lastRenderedProfileId !== activeProfileId ||
    lastRenderedWorkspaceId !== activeWorkspaceId ||
    lastRenderedTabId !== activeTabId
  ) {
    tabList.scrollTop = 0;
    setKeyboardMode("browser");
    lastRenderedProfileId = activeProfileId;
    lastRenderedWorkspaceId = activeWorkspaceId;
    lastRenderedTabId = activeTabId;
  }
  setVirtualTabState(visibleTreeTabs(orderedTabs), activeTabId);
  renderVirtualTabList();

  tabNew.disabled = !activeWorkspaceId;
  workspaceTitleWrap.classList.toggle("disabled", !activeWorkspaceId);
  workspaceTitleWrap.setAttribute("tabindex", activeWorkspaceId ? "0" : "-1");
  workspaceDelete.disabled = !activeWorkspaceId || orderedWorkspaces.length <= 1;
  splitToggle.setAttribute("aria-pressed", activeWorkspace?.split_enabled ? "true" : "false");
  splitToggle.disabled = !activeTab;
  splitRatio.hidden = !activeWorkspace?.split_enabled;
  splitRatio.value = String(Math.round(Number(activeWorkspace?.split_ratio || 0.5) * 100));
  const secondaryTab = activeWorkspace?.secondary_tab_id
    ? (state.tabs || []).find((tab) => String(tab.id) === String(activeWorkspace.secondary_tab_id))
    : null;
  restorePlaceholder.hidden =
    activeTab?.runtime_state !== "restoring" && secondaryTab?.runtime_state !== "restoring";

  if (activeTab && activeTab.url && document.activeElement !== input) {
    setActiveUri(normalizeUrl(activeTab.url), false);
  }
  syncSettingsControlsFromState(state);
}

function syncShellStateFromHost(force) {
  if (!force && shellState) return;
  postBridgeCommand({
    type: "request_resync",
    last_revision: shellRevision >= 0 ? shellRevision : null,
  });
}

function queueStateRefresh() {
  // Rust pushes the resulting revisioned patch after the committed SQLite write.
}

function nextWorkspaceName() {
  if (!shellState || !Array.isArray(shellState.workspaces)) {
    return "Workspace";
  }
  return `Workspace ${shellState.workspaces.length + 1}`;
}

function nextProfileName() {
  if (!shellState || !Array.isArray(shellState.profiles)) {
    return "Profile";
  }
  return `Profile ${shellState.profiles.length + 1}`;
}

function createProfile() {
  openProfileEditor("create");
}

function switchProfile(profileId) {
  if (!profileId) return;
  if (!shellState || !Array.isArray(shellState.profiles)) return;
  const exists = shellState.profiles.some((profile) => String(profile.id) === String(profileId));
  if (!exists) return;
  if (
    shellState &&
    shellState.active_profile_id !== null &&
    String(shellState.active_profile_id) === String(profileId)
  ) {
    return;
  }
  send(`switch_profile ${profileId}`);
  queueStateRefresh();
}

function renameActiveProfile() {
  if (!shellState) return;
  const { activeProfile } = deriveActiveContext(shellState);
  if (!activeProfile) return;
  openProfileEditor("rename");
}

function deleteActiveProfile() {
  if (!shellState || !Array.isArray(shellState.profiles)) return;
  const { activeProfile } = deriveActiveContext(shellState);
  if (!activeProfile) return;
  if (shellState.profiles.length <= 1) {
    window.alert("At least one profile must remain.");
    return;
  }
  const confirmed = window.confirm(
    `Delete profile "${profileDisplayName(activeProfile)}" and all associated workspaces/tabs?`
  );
  if (!confirmed) return;
  send(`delete_profile ${activeProfile.id}`);
  closeProfileEditor();
  closeProfileMenu();
  queueStateRefresh();
}

function openProfileEditor(mode) {
  if (!shellState) return;
  const { activeProfile } = deriveActiveContext(shellState);
  const isRename = mode === "rename";
  const targetProfile = isRename ? activeProfile : null;
  if (isRename && !targetProfile) return;
  openProfileMenu();
  profileEditorMode = mode;
  profileEditorTargetId = targetProfile ? targetProfile.id : null;
  profileEditorSubmit.textContent = isRename ? "Save" : "Create";
  profileEditorInput.value = isRename
    ? profileDisplayName(targetProfile)
    : nextProfileName();
  profileEditor.hidden = false;
  window.requestAnimationFrame(() => {
    profileEditorInput.focus();
    profileEditorInput.select();
  });
}

function closeProfileEditor() {
  profileEditorMode = null;
  profileEditorTargetId = null;
  profileEditor.hidden = true;
  profileEditorInput.value = "";
}

function submitProfileEditor() {
  if (!shellState) {
    closeProfileEditor();
    return;
  }
  if (!profileEditorMode) return;
  const name = profileEditorInput.value.trim();
  if (!name) {
    window.alert("Profile name cannot be empty.");
    return;
  }
  if (profileEditorMode === "rename") {
    if (profileEditorTargetId === null) {
      closeProfileEditor();
      return;
    }
    const activeProfile = deriveActiveContext(shellState).activeProfile;
    const currentName = profileDisplayName(activeProfile);
    if (name === currentName) {
      closeProfileEditor();
      return;
    }
    send(`rename_profile ${profileEditorTargetId} ${name}`);
    closeProfileEditor();
    closeProfileMenu();
    queueStateRefresh();
    return;
  }
  send(`new_profile ${name}`);
  closeProfileEditor();
  closeProfileMenu();
  queueStateRefresh();
}

function openProfileMenu() {
  if (profileMenuButton.disabled || profileMenuOpen) return;
  profileMenuOpen = true;
  profileMenuPopover.hidden = false;
  profileMenuButton.setAttribute("aria-expanded", "true");
}

function openProfileMenuAndFocus(first) {
  openProfileMenu();
  if (!profileMenuOpen) return;
  if (first) {
    focusProfileMenuItem(0);
  } else {
    const items = profileMenuItems();
    if (items.length > 0) {
      focusProfileMenuItem(items.length - 1);
    }
  }
}

function closeProfileMenu() {
  if (!profileMenuOpen) return;
  profileMenuOpen = false;
  profileMenuPopover.hidden = true;
  profileMenuButton.setAttribute("aria-expanded", "false");
  closeProfileEditor();
}

function toggleProfileMenu() {
  if (profileMenuOpen) {
    closeProfileMenu();
  } else {
    openProfileMenu();
  }
}

function createWorkspace() {
  const name = nextWorkspaceName();
  send(`new_workspace ${name}`);
  queueStateRefresh();
}

function startWorkspaceRename() {
  if (!shellState) return;
  const { activeWorkspace } = deriveActiveContext(shellState);
  if (!activeWorkspace) return;
  workspaceTitleWrap.classList.remove("suppress-hint");
  editingWorkspaceId = activeWorkspace.id;
  editingWorkspaceOriginalName = activeWorkspace.name || "";
  workspaceTitleWrap.classList.add("editing");
  workspaceTitleInput.value = editingWorkspaceOriginalName;
  workspaceTitleInput.focus();
  workspaceTitleInput.select();
}

function cancelWorkspaceRename() {
  editingWorkspaceId = null;
  editingWorkspaceOriginalName = "";
  workspaceTitleWrap.classList.remove("editing");
  workspaceTitleWrap.classList.add("suppress-hint");
  workspaceTitleInput.value = "";
}

function commitWorkspaceRename() {
  if (editingWorkspaceId === null) return;
  const workspaceId = editingWorkspaceId;
  const originalName = editingWorkspaceOriginalName.trim();
  const trimmed = workspaceTitleInput.value.trim();
  cancelWorkspaceRename();
  if (!trimmed || trimmed === originalName) return;
  pendingWorkspaceRenameId = workspaceId;
  send(`rename_workspace ${workspaceId} ${trimmed}`);
  queueStateRefresh();
}

function deleteActiveWorkspace() {
  if (!shellState) return;
  const { activeWorkspace, orderedWorkspaces } = deriveActiveContext(shellState);
  if (!activeWorkspace) return;
  if (orderedWorkspaces.length <= 1) {
    window.alert("At least one workspace must remain.");
    return;
  }
  const confirmed = window.confirm(
    `Delete workspace "${activeWorkspace.name}" and all of its tabs?`
  );
  if (!confirmed) return;
  send(`delete_workspace ${activeWorkspace.id}`);
  queueStateRefresh();
}

function createTabInActiveWorkspace() {
  if (!shellState) return;
  const { activeWorkspace } = deriveActiveContext(shellState);
  if (!activeWorkspace) return;
  openCommandPanel({ createsTab: true });
}

function openArchivePanel() {
  openCommandPanel({ initial: "done:", createsTab: false });
}

function toggleSplitView() {
  if (!shellState) return;
  const { activeWorkspace, activeTab, orderedTabs } = deriveActiveContext(shellState);
  if (!activeWorkspace) return;
  if (activeWorkspace.split_enabled) {
    postBridgeCommand({ type: "clear_split", workspace_id: Number(activeWorkspace.id) });
    return;
  }
  const secondary = orderedTabs.find((tab) => !activeTab || String(tab.id) !== String(activeTab.id));
  if (secondary) {
    postBridgeCommand({
      type: "set_secondary_tab",
      workspace_id: Number(activeWorkspace.id),
      tab_id: Number(secondary.id),
    });
  } else {
    openCommandPanel({ createsTab: true });
  }
}

function setKeyboardMode(mode) {
  const next = mode === "insert" ? "insert" : "browser";
  if (keyboardMode === next) return;
  keyboardMode = next;
  modeIndicator.textContent = next.toUpperCase();
  modeIndicator.classList.toggle("insert", next === "insert");
  syncWebsiteKeybindingsActive();
}

function toggleFocusMode() {
  focusMode = !focusMode;
  document.body.classList.toggle("focus-mode", focusMode);
  focusToggle.setAttribute("aria-pressed", focusMode ? "true" : "false");
  postBridgeCommand({ type: "set_focus_mode", active: focusMode });
}

function activateRelativeTab(step) {
  if (!shellState) return;
  const { activeTab, orderedTabs } = deriveActiveContext(shellState);
  if (!activeTab || orderedTabs.length < 2) return;
  const index = orderedTabs.findIndex((tab) => String(tab.id) === String(activeTab.id));
  const next = orderedTabs[(index + step + orderedTabs.length) % orderedTabs.length];
  if (next) postBridgeCommand({ type: "activate_tab", tab_id: Number(next.id) });
}

function activateRelativeWorkspace(step) {
  if (!shellState) return;
  const { activeWorkspace, orderedWorkspaces } = deriveActiveContext(shellState);
  if (!activeWorkspace || orderedWorkspaces.length < 2) return;
  const index = orderedWorkspaces.findIndex((workspace) => String(workspace.id) === String(activeWorkspace.id));
  const next = orderedWorkspaces[(index + step + orderedWorkspaces.length) % orderedWorkspaces.length];
  if (next) postBridgeCommand({ type: "switch_workspace", workspace_id: Number(next.id) });
}

function snoozeTab(tab) {
  if (!tab || tab.locked) return;
  const choice = window.prompt("Snooze until: later today, tomorrow, next week, or enter a local date/time", "tomorrow");
  if (!choice) return;
  const now = new Date();
  let wake;
  if (choice.trim().toLowerCase() === "later today") {
    wake = new Date(now);
    wake.setHours(Math.max(now.getHours() + 2, 17), 0, 0, 0);
  } else if (choice.trim().toLowerCase() === "tomorrow") {
    wake = new Date(now);
    wake.setDate(wake.getDate() + 1);
    wake.setHours(9, 0, 0, 0);
  } else if (choice.trim().toLowerCase() === "next week") {
    wake = new Date(now);
    wake.setDate(wake.getDate() + 7);
    wake.setHours(9, 0, 0, 0);
  } else {
    wake = new Date(choice);
  }
  if (!Number.isFinite(wake.getTime()) || wake.getTime() <= Date.now()) return;
  postBridgeCommand({ type: "snooze_tab", tab_id: Number(tab.id), wake_at_ms: wake.getTime() });
}

function snoozeActiveTab() {
  if (!shellState) return;
  snoozeTab(deriveActiveContext(shellState).activeTab);
}

function restoreMostRecentCompletedTab() {
  if (!shellState) return;
  const completed = (shellState.tabs || [])
    .filter((tab) => tab.status?.kind === "done")
    .sort((left, right) => (right.status?.completed_at_ms || 0) - (left.status?.completed_at_ms || 0));
  const tab = completed[0];
  if (!tab) return;
  postBridgeCommand({ type: "restore_tab", tab_id: Number(tab.id) });
  postBridgeCommand({ type: "activate_tab", tab_id: Number(tab.id) });
}

function handleWorkspaceClick(event) {
  const target = event.target.closest(".workspace-item");
  if (!target) return;
  const workspaceId = target.dataset.workspaceId;
  if (!workspaceId) return;
  send(`switch_workspace ${workspaceId}`);
  queueStateRefresh();
}

function handleTabClick(event) {
  const closeTarget = event.target.closest(".tab-close");
  if (closeTarget) {
    event.preventDefault();
    event.stopPropagation();
    const tabId = closeTarget.dataset.tabId;
    if (!tabId) return;
    send(`close_tab ${tabId}`);
    queueStateRefresh();
    return;
  }

  const target = event.target.closest(".tab-item");
  if (!target) return;
  if (target.classList.contains("active")) return;
  const tabId = target.dataset.tabId;
  if (!tabId) return;
  const tabUrl = normalizeUrl(target.dataset.tabUrl || "");
  if (tabUrl && document.activeElement !== input) {
    setActiveUri(tabUrl, false);
  }
  send(`activate_tab ${tabId}`);
  queueStateRefresh();
}

function handleTabContextMenu(event) {
  const target = event.target.closest(".tab-item");
  if (!target || !shellState) return;
  event.preventDefault();
  const tab = (shellState.tabs || []).find((candidate) => String(candidate.id) === String(target.dataset.tabId));
  if (!tab) return;
  const action = window.prompt(
    "Tab action: rename, subtab, pin, unpin, lock, unlock, snooze, done, split",
    tab.locked ? "unlock" : "rename"
  );
  if (!action) return;
  switch (action.trim().toLowerCase()) {
    case "rename": {
      const title = window.prompt("Tab name (empty restores the observed title)", tab.title || "");
      if (title !== null) postBridgeCommand({ type: "rename_tab", tab_id: Number(tab.id), title });
      break;
    }
    case "subtab": openCommandPanel({ createsTab: true, parentTabId: tab.id }); break;
    case "pin": postBridgeCommand({ type: "pin_tab", tab_id: Number(tab.id), pinned: true }); break;
    case "unpin": postBridgeCommand({ type: "pin_tab", tab_id: Number(tab.id), pinned: false }); break;
    case "lock": postBridgeCommand({ type: "set_tab_locked", tab_id: Number(tab.id), locked: true }); break;
    case "unlock": postBridgeCommand({ type: "set_tab_locked", tab_id: Number(tab.id), locked: false }); break;
    case "snooze": {
      snoozeTab(tab);
      break;
    }
    case "done": postBridgeCommand({ type: "complete_tab", tab_id: Number(tab.id), completed_at_ms: Date.now() }); break;
    case "split": {
      const workspace = (shellState.workspaces || []).find((item) => String(item.id) === String(tab.workspace_id));
      if (workspace) postBridgeCommand({ type: "set_secondary_tab", workspace_id: Number(workspace.id), tab_id: Number(tab.id) });
      break;
    }
    default: break;
  }
}

function handleTabDragStart(event) {
  const target = event.target.closest(".tab-item");
  if (!target || !event.dataTransfer) return;
  event.dataTransfer.effectAllowed = "move";
  event.dataTransfer.setData("text/switchboard-tab", target.dataset.tabId || "");
}

function handleTabDrop(event) {
  const target = event.target.closest(".tab-item");
  if (!target || !event.dataTransfer || !shellState) return;
  const draggedId = event.dataTransfer.getData("text/switchboard-tab");
  const targetId = target.dataset.tabId;
  if (!draggedId || !targetId || draggedId === targetId) return;
  event.preventDefault();
  const tabs = shellState.tabs || [];
  const dragged = tabs.find((tab) => String(tab.id) === String(draggedId));
  const destination = tabs.find((tab) => String(tab.id) === String(targetId));
  const workspace = (shellState.workspaces || []).find((item) => String(item.id) === String(destination?.workspace_id));
  if (!dragged || !destination || !workspace) return;
  const asChild = event.altKey;
  const parentId = asChild ? destination.id : destination.parent_tab_id;
  const siblings = (workspace.tab_order || [])
    .map((id) => tabs.find((tab) => String(tab.id) === String(id)))
    .filter((tab) => tab && String(tab.parent_tab_id || "") === String(parentId || ""));
  const index = asChild ? siblings.length : Math.max(0, siblings.findIndex((tab) => String(tab.id) === String(destination.id)));
  postBridgeCommand({
    type: "move_tab_tree",
    tab_id: Number(dragged.id),
    workspace_id: Number(workspace.id),
    parent_tab_id: parentId ? Number(parentId) : null,
    sibling_index: index,
  });
}

function handleShortcutCloseTab(event) {
  const binding = keybindingSetting(
    KEYBINDING_CLOSE_TAB_SETTING_KEY,
    DEFAULT_KEYBINDING_CLOSE_TAB
  );
  if (!keybindingMatchesEvent(binding, event)) return false;
  event.preventDefault();
  closeActiveTabFromShortcut();
  return true;
}

function handleShortcutCommandPalette(event) {
  const binding = keybindingSetting(
    KEYBINDING_COMMAND_PALETTE_SETTING_KEY,
    DEFAULT_KEYBINDING_COMMAND_PALETTE
  );
  if (!keybindingMatchesEvent(binding, event)) return false;
  event.preventDefault();
  openCommandPanel();
  return true;
}

function handleShortcutFocusNavigation(event) {
  const binding = keybindingSetting(
    KEYBINDING_FOCUS_NAVIGATION_SETTING_KEY,
    DEFAULT_KEYBINDING_FOCUS_NAVIGATION
  );
  if (!keybindingMatchesEvent(binding, event)) return false;
  event.preventDefault();
  focusTopNavigationInput();
  return true;
}

function handleShortcutToggleDevTools(event) {
  const binding = keybindingSetting(
    KEYBINDING_TOGGLE_DEVTOOLS_SETTING_KEY,
    DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS
  );
  if (!keybindingMatchesEvent(binding, event)) return false;
  event.preventDefault();
  toggleDevTools();
  return true;
}

input.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    navigateFromInput();
  }
});
backButton.addEventListener("click", goBack);
forwardButton.addEventListener("click", goForward);
reloadButton.addEventListener("click", reload);
devtoolsToggle.addEventListener("click", () => {
  toggleDevTools();
});
settingsToggle.addEventListener("click", (event) => {
  event.stopPropagation();
  toggleSettingsPanel();
});
settingsClose.addEventListener("click", () => {
  closeSettingsPanel();
});
settingsBackdrop.addEventListener("click", () => {
  closeSettingsPanel();
});
settingsPanel.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  event.preventDefault();
  closeSettingsPanel();
  settingsToggle.focus();
});
settingsSearchEngine.addEventListener("change", () => {
  const next = normalizeSearchEngine(settingsSearchEngine.value);
  settingsSearchEngine.value = next;
  commitTextSetting(SEARCH_ENGINE_SETTING_KEY, next);
});
settingsNewTabBehavior.addEventListener("change", () => {
  const next = normalizeNewTabBehavior(settingsNewTabBehavior.value);
  settingsNewTabBehavior.value = next;
  settingsCustomUrlField.hidden = next !== "custom";
  commitTextSetting(NEW_TAB_BEHAVIOR_SETTING_KEY, next);
});
settingsHomepage.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  settingsHomepage.blur();
});
settingsHomepage.addEventListener("blur", () => {
  commitHomepageSetting();
});
settingsCustomUrl.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  settingsCustomUrl.blur();
});
settingsCustomUrl.addEventListener("blur", () => {
  commitCustomNewTabUrlSetting();
});
settingsKeybindingCloseTab.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  settingsKeybindingCloseTab.blur();
});
settingsKeybindingCloseTab.addEventListener("blur", () => {
  commitKeybindingSetting(
    settingsKeybindingCloseTab,
    KEYBINDING_CLOSE_TAB_SETTING_KEY,
    DEFAULT_KEYBINDING_CLOSE_TAB
  );
});
settingsKeybindingCommand.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  settingsKeybindingCommand.blur();
});
settingsKeybindingCommand.addEventListener("blur", () => {
  commitKeybindingSetting(
    settingsKeybindingCommand,
    KEYBINDING_COMMAND_PALETTE_SETTING_KEY,
    DEFAULT_KEYBINDING_COMMAND_PALETTE
  );
});
settingsKeybindingFocusNav.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  settingsKeybindingFocusNav.blur();
});
settingsKeybindingFocusNav.addEventListener("blur", () => {
  commitKeybindingSetting(
    settingsKeybindingFocusNav,
    KEYBINDING_FOCUS_NAVIGATION_SETTING_KEY,
    DEFAULT_KEYBINDING_FOCUS_NAVIGATION
  );
});
settingsKeybindingDevTools.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  settingsKeybindingDevTools.blur();
});
settingsKeybindingDevTools.addEventListener("blur", () => {
  commitKeybindingSetting(
    settingsKeybindingDevTools,
    KEYBINDING_TOGGLE_DEVTOOLS_SETTING_KEY,
    DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS
  );
});
commandBackdrop.addEventListener("click", () => {
  closeCommandPanel();
});
commandForm.addEventListener("submit", (event) => {
  event.preventDefault();
  navigateFromCommandPanel();
});
commandInput.addEventListener("input", () => {
  commandSelectedIndex = 0;
  requestLazySearch();
});
commandPanel.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    closeCommandPanel();
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (commandSearchResults.length === 0) return;
    const direction = event.key === "ArrowDown" ? 1 : -1;
    commandSelectedIndex = (commandSelectedIndex + direction + commandSearchResults.length) % commandSearchResults.length;
    renderSearchResults(commandSearchResults);
  }
});
profileNew.addEventListener("click", createProfile);
profileMenuButton.addEventListener("click", (event) => {
  event.stopPropagation();
  toggleProfileMenu();
});
profileMenuButton.addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown") {
    event.preventDefault();
    openProfileMenuAndFocus(true);
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    openProfileMenuAndFocus(false);
    return;
  }
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    toggleProfileMenu();
    if (profileMenuOpen) {
      focusProfileMenuItem(0);
    }
  }
});
profileMenuList.addEventListener("click", (event) => {
  const selected = event.target.closest(".profile-menu-item");
  if (!selected) return;
  const selectedProfileId = selected.dataset.profileId || "";
  closeProfileMenu();
  switchProfile(selectedProfileId);
});
profileMenuPopover.addEventListener("keydown", (event) => {
  if (!profileMenuOpen) return;
  if (event.key === "ArrowDown") {
    event.preventDefault();
    focusNextProfileMenuItem(1);
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    focusNextProfileMenuItem(-1);
    return;
  }
  if (event.key === "Home") {
    event.preventDefault();
    focusProfileMenuItem(0);
    return;
  }
  if (event.key === "End") {
    event.preventDefault();
    const items = profileMenuItems();
    if (items.length > 0) {
      focusProfileMenuItem(items.length - 1);
    }
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    closeProfileMenu();
    profileMenuButton.focus();
  }
});
profileRename.addEventListener("click", (event) => {
  event.stopPropagation();
  renameActiveProfile();
});
profileDelete.addEventListener("click", (event) => {
  event.stopPropagation();
  deleteActiveProfile();
});
profileEditor.addEventListener("submit", (event) => {
  event.preventDefault();
  submitProfileEditor();
});
profileEditorCancel.addEventListener("click", (event) => {
  event.stopPropagation();
  closeProfileEditor();
});
profileEditorInput.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  event.preventDefault();
  closeProfileEditor();
});
workspaceNew.addEventListener("click", createWorkspace);
workspaceDelete.addEventListener("click", deleteActiveWorkspace);
workspaceTitleWrap.addEventListener("click", () => {
  if (workspaceTitleWrap.classList.contains("disabled")) return;
  if (workspaceTitleWrap.classList.contains("editing")) return;
  startWorkspaceRename();
});
workspaceTitleWrap.addEventListener("keydown", (event) => {
  if (workspaceTitleWrap.classList.contains("disabled")) return;
  if (workspaceTitleWrap.classList.contains("editing")) return;
  if (event.key !== "Enter" && event.key !== " ") return;
  event.preventDefault();
  startWorkspaceRename();
});
workspaceTitleInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    commitWorkspaceRename();
    workspaceTitleInput.blur();
    workspaceTitleWrap.blur();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    cancelWorkspaceRename();
    return;
  }
});
workspaceTitleInput.addEventListener("blur", () => {
  commitWorkspaceRename();
});
workspaceTitleInput.addEventListener("click", (event) => {
  event.stopPropagation();
});
workspaceTitleWrap.addEventListener("mouseleave", () => {
  workspaceTitleWrap.classList.remove("suppress-hint");
});
workspaceList.addEventListener("click", handleWorkspaceClick);
tabList.addEventListener("click", handleTabClick);
tabList.addEventListener("contextmenu", handleTabContextMenu);
tabList.addEventListener("dragstart", handleTabDragStart);
tabList.addEventListener("dragover", (event) => event.preventDefault());
tabList.addEventListener("drop", handleTabDrop);
tabList.addEventListener("scroll", () => {
  if (virtualTabs.length <= 1) return;
  scheduleVirtualTabListRender();
}, { passive: true });
tabNew.addEventListener("click", createTabInActiveWorkspace);
archiveOpen.addEventListener("click", openArchivePanel);
splitToggle.addEventListener("click", toggleSplitView);
splitToggle.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  if (!shellState) return;
  const { activeWorkspace } = deriveActiveContext(shellState);
  if (activeWorkspace?.split_enabled) {
    postBridgeCommand({ type: "swap_split", workspace_id: Number(activeWorkspace.id) });
  }
});
splitRatio.addEventListener("change", () => {
  if (!shellState) return;
  const { activeWorkspace } = deriveActiveContext(shellState);
  if (!activeWorkspace?.split_enabled) return;
  postBridgeCommand({
    type: "set_split_ratio",
    workspace_id: Number(activeWorkspace.id),
    ratio: Number(splitRatio.value) / 100,
  });
});
focusToggle.addEventListener("click", toggleFocusMode);
document.addEventListener("focusin", () => {
  syncWebsiteKeybindingsActive();
});
document.addEventListener("focusout", () => {
  window.setTimeout(syncWebsiteKeybindingsActive, 0);
});
document.addEventListener("pointerdown", () => {
  setWebsiteKeybindingsActive(false);
}, { capture: true });
document.addEventListener("pointerdown", (event) => {
  if (!profileMenuOpen) return;
  if (event.target.closest(".profile-menu")) return;
  closeProfileMenu();
});
document.addEventListener("pointerdown", (event) => {
  if (!settingsPanelOpen) return;
  if (event.target.closest("#settings-panel")) return;
  if (event.target.closest("#settings-toggle")) return;
  closeSettingsPanel();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    closeProfileMenu();
    closeCommandPanel();
    closeSettingsPanel();
    setKeyboardMode("browser");
    return;
  }

  if (commandPanelOpen || settingsPanelOpen) return;
  if (handleShortcutCommandPalette(event)) return;
  if (handleShortcutFocusNavigation(event)) return;
  if (handleShortcutCloseTab(event)) return;
  if (handleShortcutToggleDevTools(event)) return;
  if (keyboardMode === "insert") return;
  if (shouldIgnoreGlobalShortcutTarget(event.target)) return;

  if (!event.metaKey && !event.ctrlKey && !event.altKey) {
    const key = event.key.toLowerCase();
    if (key === "i") {
      event.preventDefault();
      setKeyboardMode("insert");
      return;
    }
    if (key === "j") {
      event.preventDefault();
      activateRelativeTab(1);
      return;
    }
    if (key === "k") {
      event.preventDefault();
      activateRelativeTab(-1);
      return;
    }
    if (key === "w") {
      event.preventDefault();
      activateRelativeWorkspace(1);
      return;
    }
    if (key === "d") {
      event.preventDefault();
      closeActiveTabFromShortcut();
      return;
    }
    if (key === "h") {
      event.preventDefault();
      snoozeActiveTab();
      return;
    }
    if (key === "f") {
      event.preventDefault();
      toggleFocusMode();
      return;
    }
    if (key === "z") {
      event.preventDefault();
      restoreMostRecentCompletedTab();
      return;
    }
  }

  const hasPrimaryModifier = event.metaKey || event.ctrlKey;
  if (!hasPrimaryModifier || !event.shiftKey || event.altKey) return;

  const keyLower = event.key.toLowerCase();
  if (keyLower === "p") {
    event.preventDefault();
    openProfileMenuAndFocus(true);
    return;
  }
  if (keyLower === "n") {
    event.preventDefault();
    createProfile();
    return;
  }
  if (keyLower === "r") {
    event.preventDefault();
    renameActiveProfile();
  }
});

window.__switchboardHostShortcut = (action) => {
  const normalized = String(action || "").trim().toLowerCase();
  if (normalized === "reload") {
    postBridgeCommand({ type: "reload" });
    return true;
  }
  if (normalized === "command_palette") {
    openCommandPanel();
    return true;
  }
  if (normalized === "focus_navigation") {
    focusTopNavigationInput();
    return true;
  }
  if (normalized === "close_tab") {
    closeActiveTabFromShortcut();
    return true;
  }
  if (normalized === "toggle_devtools") {
    toggleDevTools();
    return true;
  }
  if (normalized === "enter_browser_mode") { setKeyboardMode("browser"); return true; }
  if (normalized === "enter_insert_mode") { setKeyboardMode("insert"); return true; }
  if (normalized === "next_tab") { activateRelativeTab(1); return true; }
  if (normalized === "previous_tab") { activateRelativeTab(-1); return true; }
  if (normalized === "next_workspace") { activateRelativeWorkspace(1); return true; }
  if (normalized === "complete_tab") { closeActiveTabFromShortcut(); return true; }
  if (normalized === "snooze_tab") { snoozeActiveTab(); return true; }
  if (normalized === "toggle_focus") { toggleFocusMode(); return true; }
  if (normalized === "restore_completed") { restoreMostRecentCompletedTab(); return true; }
  if (normalized === "toggle_split") { toggleSplitView(); return true; }
  return false;
};

renderUri();
send("ui_ready 0.1.0-dev");
window.addEventListener("focus", () => {
  syncShellStateFromHost(true);
  syncActiveUriFromHost();
  syncWebsiteKeybindingsActive();
});
window.addEventListener("blur", () => {
  syncWebsiteKeybindingsActive();
});
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    syncShellStateFromHost(true);
    syncActiveUriFromHost();
  }
  syncWebsiteKeybindingsActive();
});
window.addEventListener("resize", () => {
  scheduleVirtualTabListRender();
});
syncWebsiteKeybindingsActive();

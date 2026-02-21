const marker = "__switchboard_intent__";
const key = "switchboard.active_uri";

const input = document.getElementById("url");
const backButton = document.getElementById("nav-back");
const forwardButton = document.getElementById("nav-forward");
const workspaceList = document.getElementById("workspace-list");
const workspaceNew = document.getElementById("workspace-new");
const workspaceTitle = document.getElementById("workspace-title");
const workspaceRename = document.getElementById("workspace-rename");
const workspaceDelete = document.getElementById("workspace-delete");
const tabList = document.getElementById("tab-list");
const tabNew = document.getElementById("tab-new");

let backStack = [];
let forwardStack = [];
let activeUri = normalizeUrl(localStorage.getItem(key)) || "https://youtube.com";
let shellRevision = -1;
let shellState = null;

function send(payload) {
  try {
    return window.prompt(marker, payload);
  } catch (_error) {
    return "";
  }
}

function normalizeUrl(value) {
  const raw = (value || "").trim();
  if (!raw) return "";
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(raw)) return raw;
  if (raw.includes("://")) return raw;
  return `https://${raw}`;
}

function renderUri() {
  input.value = activeUri;
  backButton.disabled = backStack.length === 0;
  forwardButton.disabled = forwardStack.length === 0;
}

function setActiveUri(next, pushHistory) {
  if (!next) return;
  if (next === activeUri) {
    renderUri();
    return;
  }
  if (pushHistory && activeUri) {
    backStack.push(activeUri);
    forwardStack = [];
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
  const next = normalizeUrl(input.value);
  if (!next) return;
  navigateTo(next, true);
}

function goBack() {
  if (backStack.length === 0) return;
  if (activeUri) {
    forwardStack.push(activeUri);
  }
  const previous = backStack.pop();
  navigateTo(previous, false);
}

function goForward() {
  if (forwardStack.length === 0) return;
  if (activeUri) {
    backStack.push(activeUri);
  }
  const next = forwardStack.pop();
  navigateTo(next, false);
}

function syncActiveUriFromHost() {
  if (document.hidden || document.activeElement === input) return;
  const response = send("query_active_uri");
  const hostUri = normalizeUrl(response);
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

function workspaceBadge(name) {
  const trimmed = (name || "").trim();
  if (!trimmed) return "W";
  return trimmed.slice(0, 1).toUpperCase();
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
    activeProfile,
    activeWorkspace,
    activeTab,
    orderedWorkspaces,
    orderedTabs,
  };
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

function renderTabList(orderedTabs, activeTabId) {
  tabList.innerHTML = "";
  if (orderedTabs.length === 0) {
    const empty = document.createElement("div");
    empty.className = "tab-empty";
    empty.textContent = "No tabs yet.";
    tabList.appendChild(empty);
    return;
  }

  orderedTabs.forEach((tab) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "tab-item";
    button.dataset.tabId = String(tab.id);
    button.dataset.tabUrl = tab.url || "";
    if (tab.id === activeTabId) {
      button.classList.add("active");
    }

    const icon = document.createElement("span");
    icon.className = "tab-icon";
    icon.textContent = tabLabel(tab).slice(0, 1).toUpperCase();
    button.appendChild(icon);

    const content = document.createElement("span");
    content.className = "tab-copy";

    const title = document.createElement("span");
    title.className = "tab-title";
    title.textContent = tabLabel(tab);
    content.appendChild(title);

    const url = document.createElement("span");
    url.className = "tab-url";
    url.textContent = tab.url || "about:blank";
    content.appendChild(url);

    button.appendChild(content);

    tabList.appendChild(button);
  });
}

function renderShellState(state) {
  const { activeWorkspace, activeTab, orderedWorkspaces, orderedTabs } = deriveActiveContext(state);
  const activeWorkspaceId = activeWorkspace ? activeWorkspace.id : null;
  const activeTabId = activeWorkspace ? activeWorkspace.active_tab_id : null;

  workspaceTitle.textContent = activeWorkspace ? activeWorkspace.name : "No Workspace";
  renderWorkspaceRail(orderedWorkspaces, activeWorkspaceId);
  renderTabList(orderedTabs, activeTabId);
  tabNew.disabled = !activeWorkspaceId;
  workspaceRename.disabled = !activeWorkspaceId;
  workspaceDelete.disabled = !activeWorkspaceId || orderedWorkspaces.length <= 1;

  if (activeTab && activeTab.url && document.activeElement !== input) {
    setActiveUri(normalizeUrl(activeTab.url), false);
  }
}

function syncShellStateFromHost(force) {
  const raw = send("query_shell_state");
  const next = parseShellState(raw);
  if (!next) return;
  if (!force && next.revision === shellRevision) return;
  shellRevision = next.revision;
  shellState = next;
  renderShellState(next);
}

function queueStateRefresh() {
  window.setTimeout(() => {
    syncShellStateFromHost(true);
    syncActiveUriFromHost();
  }, 180);
}

function nextWorkspaceName() {
  if (!shellState || !Array.isArray(shellState.workspaces)) {
    return "Workspace";
  }
  return `Workspace ${shellState.workspaces.length + 1}`;
}

function createWorkspace() {
  const name = nextWorkspaceName();
  send(`new_workspace ${name}`);
  queueStateRefresh();
}

function renameActiveWorkspace() {
  if (!shellState) return;
  const { activeWorkspace } = deriveActiveContext(shellState);
  if (!activeWorkspace) return;
  const nextName = window.prompt("Rename workspace", activeWorkspace.name || "");
  if (nextName === null) return;
  const trimmed = nextName.trim();
  if (!trimmed || trimmed === activeWorkspace.name) return;
  send(`rename_workspace ${activeWorkspace.id} ${trimmed}`);
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
  send(`new_tab ${activeWorkspace.id}`);
  queueStateRefresh();
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

function startHostSyncLoop() {
  function tick() {
    syncActiveUriFromHost();
    syncShellStateFromHost(false);
    window.setTimeout(tick, 1200);
  }
  window.setTimeout(tick, 1200);
}

input.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    navigateFromInput();
  }
});
backButton.addEventListener("click", goBack);
forwardButton.addEventListener("click", goForward);
workspaceNew.addEventListener("click", createWorkspace);
workspaceRename.addEventListener("click", renameActiveWorkspace);
workspaceDelete.addEventListener("click", deleteActiveWorkspace);
workspaceList.addEventListener("click", handleWorkspaceClick);
tabList.addEventListener("click", handleTabClick);
tabNew.addEventListener("click", createTabInActiveWorkspace);

renderUri();
send("ui_ready 0.1.0-dev");
syncShellStateFromHost(true);
syncActiveUriFromHost();
startHostSyncLoop();
window.addEventListener("focus", () => {
  syncShellStateFromHost(true);
  syncActiveUriFromHost();
});
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    syncShellStateFromHost(true);
    syncActiveUriFromHost();
  }
});

const marker = "__switchboard_intent__";
const key = "switchboard.active_uri";

const input = document.getElementById("url");
const backButton = document.getElementById("nav-back");
const forwardButton = document.getElementById("nav-forward");
const workspaceList = document.getElementById("workspace-list");
const workspaceNew = document.getElementById("workspace-new");
const workspaceTitleWrap = document.getElementById("workspace-title-wrap");
const workspaceTitle = document.getElementById("workspace-title");
const workspaceTitleInput = document.getElementById("workspace-title-input");
const workspaceDelete = document.getElementById("workspace-delete");
const tabList = document.getElementById("tab-list");
const tabNew = document.getElementById("tab-new");

const TAB_ROW_HEIGHT = 56;
const TAB_OVERSCAN = 6;
const TAB_LIST_PADDING_Y = 8;

let backStack = [];
let forwardStack = [];
let activeUri = normalizeUrl(localStorage.getItem(key)) || "https://youtube.com";
let shellRevision = -1;
let shellState = null;
let editingWorkspaceId = null;
let editingWorkspaceOriginalName = "";
let pendingWorkspaceRenameId = null;
let virtualTabs = [];
let virtualActiveTabId = null;
let lastRenderedWorkspaceId = null;
let virtualRenderPending = false;
let virtualDataEpoch = 0;
let virtualRenderKey = "";

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

function createTabButton(tab, isActive) {
  const label = tabLabel(tab);
  const button = document.createElement("button");
  button.type = "button";
  button.className = "tab-item";
  button.dataset.tabId = String(tab.id);
  button.dataset.tabUrl = tab.url || "";
  if (isActive) {
    button.classList.add("active");
  }
  if (tab.loading) {
    button.classList.add("loading");
  }

  const icon = document.createElement("span");
  icon.className = "tab-icon";
  icon.textContent = label.slice(0, 1).toUpperCase();
  button.appendChild(icon);

  const content = document.createElement("span");
  content.className = "tab-copy";

  const title = document.createElement("span");
  title.className = "tab-title";
  title.textContent = label;
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
  const { activeWorkspace, activeTab, orderedWorkspaces, orderedTabs } = deriveActiveContext(state);
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

  renderWorkspaceRail(orderedWorkspaces, activeWorkspaceId);
  if (lastRenderedWorkspaceId !== activeWorkspaceId) {
    tabList.scrollTop = 0;
    lastRenderedWorkspaceId = activeWorkspaceId;
  }
  setVirtualTabState(orderedTabs, activeTabId);
  renderVirtualTabList();

  tabNew.disabled = !activeWorkspaceId;
  workspaceTitleWrap.classList.toggle("disabled", !activeWorkspaceId);
  workspaceTitleWrap.setAttribute("tabindex", activeWorkspaceId ? "0" : "-1");
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
tabList.addEventListener("scroll", () => {
  if (virtualTabs.length <= 1) return;
  scheduleVirtualTabListRender();
}, { passive: true });
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
window.addEventListener("resize", () => {
  scheduleVirtualTabListRender();
});

const marker = "__switchboard_intent__";
const key = "switchboard.active_uri";
const input = document.getElementById("url");
const current = document.getElementById("current");
const backButton = document.getElementById("nav-back");
const forwardButton = document.getElementById("nav-forward");

let backStack = [];
let forwardStack = [];

function send(payload) { window.prompt(marker, payload); }

function normalizeUrl(value) {
  const raw = (value || "").trim();
  if (!raw) return "";
  if (raw.startsWith("http://") || raw.startsWith("https://")) return raw;
  return `https://${raw}`;
}

let activeUri = normalizeUrl(localStorage.getItem(key)) || "https://youtube.com";

function renderUri(force_input_update) {
  input.value = activeUri;
  current.textContent = `Current URI: ${activeUri}`;
  backButton.disabled = backStack.length === 0;
  forwardButton.disabled = forwardStack.length === 0;
}

function navigateTo(next, push_history) {
  if (!next) {
    return;
  }
  if (next === activeUri) {
    renderUri();
    return;
  }

  if (push_history && activeUri) {
    backStack.push(activeUri);
  }
  if (push_history) {
    forwardStack = [];
  }

  activeUri = next;
  localStorage.setItem(key, activeUri);
  renderUri(true);
  send(`navigate ${activeUri}`);
}

function navigateFromInput() {
  const next = normalizeUrl(input.value);
  if (!next) return;
  navigateTo(next, true);
}

function goBack() {
  if (backStack.length === 0) {
    return;
  }
  if (activeUri) {
    forwardStack.push(activeUri);
  }
  const previous = backStack.pop();
  navigateTo(previous, false);
}

function goForward() {
  if (forwardStack.length === 0) {
    return;
  }
  if (activeUri) {
    backStack.push(activeUri);
  }
  const next = forwardStack.pop();
  navigateTo(next, false);
}

function syncActiveUriFromHost() {
  if (document.hidden || document.activeElement === input) {
    return;
  }
  const response = window.prompt(marker, "query_active_uri");
  const hostUri = normalizeUrl(response);
  if (!hostUri || hostUri === activeUri) {
    return;
  }

  if (activeUri) {
    backStack.push(activeUri);
  }
  forwardStack = [];
  activeUri = hostUri;
  localStorage.setItem(key, activeUri);
  renderUri(true);
}

function startHostSyncLoop() {
  function tick() {
    syncActiveUriFromHost();
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

renderUri(true);
send("ui_ready 0.1.0-dev");
syncActiveUriFromHost();
startHostSyncLoop();
window.addEventListener("focus", syncActiveUriFromHost);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    syncActiveUriFromHost();
  }
});

const marker = "__switchboard_intent__";
const key = "switchboard.active_uri";
const input = document.getElementById("url");
const current = document.getElementById("current");

function send(payload) { window.prompt(marker, payload); }

function normalizeUrl(value) {
  const raw = (value || "").trim();
  if (!raw) return "";
  if (raw.startsWith("http://") || raw.startsWith("https://")) return raw;
  return `https://${raw}`;
}

let activeUri = localStorage.getItem(key) || "https://youtube.com";

function renderUri() {
  input.placeholder = activeUri;
  current.textContent = `Current URI: ${activeUri}`;
}

function navigate() {
  const next = normalizeUrl(input.value);
  if (!next) return;
  activeUri = next;
  localStorage.setItem(key, activeUri);
  renderUri();
  input.value = "";
  send(`navigate ${activeUri}`);
}

input.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    navigate();
  }
});

renderUri();
send("ui_ready 0.1.0-dev");

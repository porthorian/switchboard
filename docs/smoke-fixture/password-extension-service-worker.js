const CACHE_NAME = "switchboard-password-extension-qa-v1";

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE_NAME));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", () => {
  // The QA worker intentionally does not intercept or synthesize requests.
});

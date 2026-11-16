// Keeps the app's own files so the page reloads without the network. The
// show itself lives on the server and is not cached: only the shell is.
const CACHE = "mazzalab-shell-v1";
const SHELL = ["/", "/icon.svg", "/icon-192.png", "/icon-512.png", "/manifest.webmanifest"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== self.location.origin) return;
  if (url.pathname.startsWith("/api/") || url.pathname === "/ws" || request.headers.has("range")) return;
  // Network first, so a new build is picked up; the cache answers when it is down.
  event.respondWith(
    fetch(request)
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          // Every page of the app is the same shell, whatever its query.
          const key = request.mode === "navigate" ? "/" : request;
          caches.open(CACHE).then((cache) => cache.put(key, copy));
        }
        return response;
      })
      .catch(() => caches.match(request.mode === "navigate" ? "/" : request).then((hit) => hit ?? Response.error())),
  );
});

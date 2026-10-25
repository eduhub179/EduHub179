// Service worker of EduHub: shows push notifications and opens the site on click.
//
// Push payload (from the backend WebPushSender): {"title", "body", "url", "tag"}.
// Lives in public/ so it is served from the site root: its scope is the whole site.
// Keep it dependency-free — it runs outside the React bundle.

self.addEventListener('install', () => {
    // A new version takes over right away instead of waiting for all tabs to close.
    self.skipWaiting();
});

self.addEventListener('activate', (event) => {
    event.waitUntil(self.clients.claim());
});

self.addEventListener('push', (event) => {
    let data = {};
    try {
        data = event.data ? event.data.json() : {};
    } catch {
        data = { body: event.data ? event.data.text() : '' };
    }

    const title = data.title || 'EduHub';
    event.waitUntil(
        self.registration.showNotification(title, {
            body: data.body || '',
            // Same tag replaces the previous notification about the same homework.
            tag: data.tag || undefined,
            renotify: Boolean(data.tag),
            // Homework is easy to miss in a banner that fades in seconds: keep it until the
            // student clicks or closes it. Chrome on macOS shows such notifications through
            // its separate "alerts" service; Safari ignores the flag (system style applies).
            requireInteraction: true,
            icon: '/icons/icon-192.png',
            badge: '/icons/icon-192.png',
            data: { url: data.url || '/main' },
        }),
    );
});

self.addEventListener('notificationclick', (event) => {
    event.notification.close();
    const target = new URL(event.notification.data?.url || '/main', self.location.origin).href;

    event.waitUntil(
        (async () => {
            // Prefer an already open tab of the site: focus it and go to the page.
            const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
            for (const client of windows) {
                if (new URL(client.url).origin === self.location.origin) {
                    await client.focus();
                    if ('navigate' in client) {
                        await client.navigate(target);
                    }
                    return;
                }
            }
            await self.clients.openWindow(target);
        })(),
    );
});

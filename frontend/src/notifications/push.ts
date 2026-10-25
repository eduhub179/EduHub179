import {
  fetchVapidPublicKey,
  registerPushSubscription,
  unregisterPushSubscription,
  type PushSubscriptionDto,
} from '../api/notificationsApi';

/**
 * Browser push notifications: service worker + PushManager subscription, kept in sync
 * with the backend (`/api/notifications/subscriptions`).
 *
 * States:
 * - unsupported — no service worker / PushManager (old browser; iPhone Safari outside
 *   an installed PWA), or not a secure context (http on a non-localhost host);
 * - denied — the user blocked notifications: only the browser settings can undo it;
 * - enabled — this browser is subscribed;
 * - disabled — supported, but not subscribed yet.
 */
export type PushState = 'unsupported' | 'denied' | 'enabled' | 'disabled';

const SERVICE_WORKER_URL = '/sw.js';

export function isPushSupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    window.isSecureContext &&
    'serviceWorker' in navigator &&
    'PushManager' in window &&
    'Notification' in window
  );
}

/** True on iPhone/iPad Safari when the site is not installed to the home screen. */
export function needsInstallForPush(): boolean {
  const isIos = /iPad|iPhone|iPod/.test(navigator.userAgent);
  const isStandalone =
    window.matchMedia('(display-mode: standalone)').matches ||
    (navigator as Navigator & { standalone?: boolean }).standalone === true;
  return isIos && !isStandalone;
}

async function currentSubscription(): Promise<PushSubscription | null> {
  const registration = await navigator.serviceWorker.getRegistration(SERVICE_WORKER_URL);
  return registration ? registration.pushManager.getSubscription() : null;
}

function toDto(subscription: PushSubscription): PushSubscriptionDto {
  const json = subscription.toJSON();
  return {
    endpoint: json.endpoint ?? subscription.endpoint,
    keys: { p256dh: json.keys?.p256dh ?? '', auth: json.keys?.auth ?? '' },
  };
}

/** base64url → bytes for `applicationServerKey`. */
function base64UrlToBytes(value: string): Uint8Array<ArrayBuffer> {
  const base64 = value.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(value.length / 4) * 4, '=');
  const binary = atob(base64);
  const bytes = new Uint8Array(new ArrayBuffer(binary.length));
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export async function getPushState(): Promise<PushState> {
  if (!isPushSupported()) return 'unsupported';
  if (Notification.permission === 'denied') return 'denied';
  if (Notification.permission !== 'granted') return 'disabled';
  return (await currentSubscription()) ? 'enabled' : 'disabled';
}

/** Steps of `enablePush`, shown to the user so a stuck step is visible. */
export type EnableStep = 'permission' | 'save';

/** Rejects with `message` if `promise` does not settle in `ms`. */
function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), ms);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        window.clearTimeout(timer);
        reject(error);
      },
    );
  });
}

/**
 * Waits until this registration has an active worker. Unlike `navigator.serviceWorker.ready`
 * it watches our own registration, so it cannot wait for some other worker forever.
 */
function waitUntilActive(registration: ServiceWorkerRegistration): Promise<ServiceWorkerRegistration> {
  if (registration.active) return Promise.resolve(registration);
  const worker = registration.installing ?? registration.waiting;
  if (!worker) return Promise.resolve(registration);
  return new Promise((resolve, reject) => {
    worker.addEventListener('statechange', () => {
      if (worker.state === 'activated') resolve(registration);
      if (worker.state === 'redundant') reject(new Error('Service worker не установился (redundant).'));
    });
  });
}

/** How long the browser may take to create a subscription once permission is granted. */
const SUBSCRIBE_AFTER_GRANT_MS = 15_000;

/**
 * No timeout while the user decides in the permission prompt; but once permission is
 * granted, a subscription that does not arrive within 15 s is reported as an error
 * (seen in Safari: `subscribe()` that never settles) instead of hanging forever.
 */
function afterGrantTimeout(subscribing: Promise<PushSubscription>): Promise<PushSubscription> {
  return new Promise<PushSubscription>((resolve, reject) => {
    let grantedAt: number | null = null;
    const watchdog = window.setInterval(() => {
      if (Notification.permission !== 'granted') return;
      grantedAt ??= Date.now();
      if (Date.now() - grantedAt > SUBSCRIBE_AFTER_GRANT_MS) {
        window.clearInterval(watchdog);
        reject(new Error(
          'Разрешение получено, но браузер не создал подписку за 15 секунд. ' +
            'Откройте консоль браузера (в Safari: «Разработка» → «Показать веб-инспектор» → «Консоль») ' +
            'и посмотрите ошибки, связанные с push или service worker.',
        ));
      }
    }, 500);
    subscribing.then(
      (subscription) => {
        window.clearInterval(watchdog);
        resolve(subscription);
      },
      (error: unknown) => {
        window.clearInterval(watchdog);
        reject(error);
      },
    );
  });
}

/** Service worker and VAPID key, prepared before the click (see `preparePush`). */
export interface PreparedPush {
  registration: ServiceWorkerRegistration;
  applicationServerKey: Uint8Array<ArrayBuffer>;
}

/**
 * Registers the service worker and loads the VAPID key in advance — call when the
 * settings block appears. Safari accepts `pushManager.subscribe()` only as a direct
 * response to a click; doing this slow work inside the click handler made Safari
 * silently refuse the subscription.
 */
export async function preparePush(): Promise<PreparedPush> {
  const [registration, key] = await Promise.all([
    withTimeout(
      navigator.serviceWorker.register(SERVICE_WORKER_URL).then(waitUntilActive),
      15_000,
      'Service worker не запустился за 15 секунд. Обновите страницу и попробуйте снова.',
    ),
    fetchVapidPublicKey(),
  ]);
  return { registration, applicationServerKey: base64UrlToBytes(key) };
}

/**
 * Subscribes this browser and registers the subscription on the backend.
 * Must be called straight from the click handler with `prepared` ready: the first thing
 * it does is `pushManager.subscribe()`, which itself shows the permission prompt — the
 * way Safari requires (and Chrome/Firefox accept). Re-sends an existing subscription
 * (idempotent), so it also repairs a browser the backend forgot about.
 *
 * Waiting for the user's answer has no timeout; the backend request has one.
 */
export async function enablePush(
  prepared: PreparedPush,
  onStep: (step: EnableStep) => void = () => undefined,
): Promise<PushState> {
  if (!isPushSupported()) return 'unsupported';

  onStep('permission');
  let subscription: PushSubscription;
  try {
    subscription =
      (await prepared.registration.pushManager.getSubscription()) ??
      (await afterGrantTimeout(
        prepared.registration.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: prepared.applicationServerKey,
        }),
      ));
  } catch (error) {
    console.error('push: subscribe failed', error);
    if (Notification.permission === 'denied') return 'denied';
    const name = error instanceof DOMException ? error.name : '';
    if (name === 'NotAllowedError') {
      throw new Error('Браузер не разрешил подписку. Нажмите кнопку ещё раз и выберите «Разрешить».');
    }
    throw new Error(
      'Браузер не смог подписаться на push-сервис' +
        (error instanceof Error && error.message ? `: ${error.message}` : '.') +
        ' В Brave включите «Use Google services for push messaging»; сборки Chromium без сервисов Google push не поддерживают.',
    );
  }

  onStep('save');
  await withTimeout(
    registerPushSubscription(toDto(subscription)),
    15_000,
    'Сервер не ответил за 15 секунд. Проверьте, что бэкенд запущен, и попробуйте снова.',
  );
  return 'enabled';
}

/**
 * Unsubscribes this browser and tells the backend. Best effort on the backend part:
 * the browser subscription is removed even if the request fails (e.g. session expired).
 */
export async function disablePush(): Promise<PushState> {
  if (!isPushSupported()) return 'unsupported';
  const subscription = await currentSubscription();
  if (subscription) {
    try {
      await unregisterPushSubscription(toDto(subscription));
    } finally {
      await subscription.unsubscribe();
    }
  }
  return Notification.permission === 'denied' ? 'denied' : 'disabled';
}

/**
 * On logout: unlink this browser from the user on the backend, but keep the browser's
 * subscription. The user stops getting notifications here; whoever logs in next is
 * linked automatically (`syncPushOnStartup`) without enabling notifications again.
 * To turn notifications off completely, use `disablePush` (profile button).
 * Best effort: a failed request (expired session) must not block logging out.
 */
export async function detachPushOnLogout(): Promise<void> {
  if (!isPushSupported()) return;
  const subscription = await currentSubscription();
  if (subscription) {
    await unregisterPushSubscription(toDto(subscription)).catch(() => undefined);
  }
}

/**
 * On app start and after login: keep the service worker up to date and link this
 * browser's subscription (if notifications were ever enabled here) to the current user.
 * Idempotent and cheap; also heals a subscription the backend forgot.
 */
export async function syncPushOnStartup(): Promise<void> {
  if (!isPushSupported() || Notification.permission !== 'granted') return;
  const registration = await navigator.serviceWorker.getRegistration(SERVICE_WORKER_URL);
  if (!registration) return;
  await registration.update().catch(() => undefined);
  const subscription = await registration.pushManager.getSubscription();
  if (subscription) {
    await registerPushSubscription(toDto(subscription)).catch(() => undefined);
  }
}

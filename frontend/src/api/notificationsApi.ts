import { apiFetch } from './client';

/** The browser's `PushSubscription.toJSON()` shape. */
export interface PushSubscriptionDto {
  endpoint: string;
  keys: { p256dh: string; auth: string };
}

/** Public VAPID key for `pushManager.subscribe`; 503 if push is off on the server. */
export async function fetchVapidPublicKey(): Promise<string> {
  const { public_key } = await apiFetch<{ public_key: string }>('/api/notifications/vapid-public-key');
  return public_key;
}

export async function registerPushSubscription(subscription: PushSubscriptionDto): Promise<void> {
  await apiFetch<void>('/api/notifications/subscriptions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(subscription),
  });
}

export async function unregisterPushSubscription(subscription: PushSubscriptionDto): Promise<void> {
  await apiFetch<{ removed: boolean }>('/api/notifications/subscriptions', {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(subscription),
  });
}

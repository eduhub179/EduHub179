declare global {
  interface ImportMetaEnv {
    readonly VITE_API_BASE_URL?: string;
  }
}

/**
 * API origin. Default: the site's own origin (relative /api/... URLs): in development
 * Vite proxies them to the backend (vite.config.ts), in production a reverse proxy does.
 * Set VITE_API_BASE_URL only if the API lives on another host.
 */
const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? '';

/** Absolute URL of an API path, for links the browser opens itself (e.g. file downloads). */
export function apiUrl(path: string): string {
  return `${API_BASE_URL}${path}`;
}

/**
 * Dispatched on `window` when the backend answers 401: the session cookie expired or
 * was revoked. AuthProvider then forgets the stored user and sends them to the login page.
 */
export const SESSION_EXPIRED_EVENT = 'eduhub:session-expired';

export async function apiFetch<T>(
  input: string,
  init?: RequestInit,
): Promise<T> {
  const headers = new Headers(init?.headers ?? {});

  if (!headers.has('Content-Type') && !(init?.body instanceof FormData)) {
    headers.set('Content-Type', 'application/json');
  }

  const response = await fetch(`${API_BASE_URL}${input}`, {
    ...init,
    credentials: init?.credentials ?? 'include',
    headers,
  });

  if (!response.ok) {
    if (response.status === 401) {
      window.dispatchEvent(new Event(SESSION_EXPIRED_EVENT));
    }
    let errorMessage = `Request failed (${response.status})`;
    // Our handlers answer {"error": "..."}; axum's own rejections (malformed JSON,
    // missing fields, too large body) answer plain text — keep that text too.
    const body = await response.text().catch(() => '');
    try {
      const payload = JSON.parse(body);
      if (payload && typeof payload.error === 'string') {
        errorMessage = payload.error;
      }
    } catch {
      if (body.trim()) {
        errorMessage = body.trim().slice(0, 300);
      }
    }

    throw new Error(errorMessage);
  }

  const text = await response.text();
  if (!text) {
    return undefined as T;
  }

  return JSON.parse(text) as T;
}

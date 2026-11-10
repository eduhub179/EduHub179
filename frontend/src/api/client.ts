import { ApiError, NETWORK_ERROR, UNEXPECTED_ERROR, userMessage } from './errors';

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

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}${input}`, {
      ...init,
      credentials: init?.credentials ?? 'include',
      headers,
    });
  } catch {
    // The browser's own text ("Failed to fetch", "Load failed") is not for people.
    throw new ApiError(NETWORK_ERROR, 0);
  }

  if (!response.ok) {
    if (response.status === 401) {
      window.dispatchEvent(new Event(SESSION_EXPIRED_EVENT));
    }
    // Only a person-friendly text leaves this function (see `errors.ts`): never the raw
    // backend, axum or nginx answer.
    const body = await response.text().catch(() => '');
    let backendText: string | null = null;
    try {
      const payload = JSON.parse(body);
      if (payload && typeof payload.error === 'string') {
        backendText = payload.error;
      }
    } catch {
      // Not JSON (axum rejection, nginx HTML page): the status decides the text.
    }
    throw new ApiError(userMessage(response.status, backendText), response.status, backendText);
  }

  const text = await response.text().catch(() => '');
  if (!text) {
    return undefined as T;
  }

  try {
    return JSON.parse(text) as T;
  } catch {
    // A proxy answered with a page instead of the API.
    throw new ApiError(UNEXPECTED_ERROR, response.status);
  }
}

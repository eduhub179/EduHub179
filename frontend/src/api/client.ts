declare global {
  interface ImportMetaEnv {
    readonly VITE_API_BASE_URL?: string;
  }
}

const API_BASE_URL =
  import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080';

/** Absolute URL of an API path, for links the browser opens itself (e.g. file downloads). */
export function apiUrl(path: string): string {
  return `${API_BASE_URL}${path}`;
}

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

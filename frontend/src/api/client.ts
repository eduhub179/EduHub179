declare global {
  interface ImportMetaEnv {
    readonly VITE_API_BASE_URL?: string;
  }
}

const API_BASE_URL =
  import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080';

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
    let errorMessage = 'Request failed';
    try {
      const payload = await response.json();
      if (payload && typeof payload.error === 'string') {
        errorMessage = payload.error;
      }
    } catch {
      // Ignore JSON parse failures and keep generic message.
    }

    throw new Error(errorMessage);
  }

  const text = await response.text();
  if (!text) {
    return undefined as T;
  }

  return JSON.parse(text) as T;
}

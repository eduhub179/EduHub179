declare global {
  interface ImportMeta {
    readonly env: {
      readonly VITE_API_BASE_URL?: string;
    };
  }
}

const API_BASE_URL =
  import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080';

export async function apiFetch<T>(
  input: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${input}`, {
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
    ...init,
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

  return (await response.json()) as T;
}

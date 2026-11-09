import { apiFetch } from './client';

export interface LoginRequest {
  login: string;
  password: string;
}

export interface PublicUserDto {
  id: string;
  login: string;
  /** School email (login + @179.ru). */
  email: string;
  role: string;
  first_name: string;
  last_name: string;
  middle_name?: string | null;
  is_active: boolean;
  class_id?: string | null;
}

export interface LoginResponse {
  token: string;
  user: PublicUserDto;
}

export interface InitSessionRequest {
  user_id: string;
  auth_token: string;
  remember_me?: boolean;
}

export interface InitSessionResponse {
  ok: boolean;
  user_id: string;
  expires_at: string;
}

export interface LogoutResponse {
  ok: boolean;
}

export async function login(payload: LoginRequest): Promise<LoginResponse> {
  return apiFetch<LoginResponse>('/api/auth/login', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

export async function initSession(
  payload: InitSessionRequest,
): Promise<InitSessionResponse> {
  return apiFetch<InitSessionResponse>('/api/auth/session/init', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    credentials: 'include',
    body: JSON.stringify(payload),
  });
}

export async function logoutSession(): Promise<LogoutResponse> {
  return apiFetch<LogoutResponse>('/api/auth/logout', {
    method: 'POST',
    credentials: 'include',
  });
}

// ---------------------------------------------------------------------------
// Sign-in by school email: password, or (first login) emailed code + new password
// ---------------------------------------------------------------------------

export type EmailLoginMethod = 'password' | 'code';

export interface EmailStartResponse {
  /** Normalized address. */
  email: string;
  /** `password` — ask for it; `code` — first login, a code was emailed. */
  method: EmailLoginMethod;
  /** For `code`: false if the previous code (sent < 1 min ago) is still the one to enter. */
  code_sent: boolean;
}

export interface EmailSessionResponse {
  user: PublicUserDto;
  expires_at: string;
}

const postJson = <T>(path: string, body: unknown): Promise<T> =>
  apiFetch<T>(path, {
    method: 'POST',
    credentials: 'include',
    body: JSON.stringify(body),
  });

/** Step 1: does the account have a password, or is this the first login (code emailed)? */
export function startEmailLogin(email: string): Promise<EmailStartResponse> {
  return postJson('/api/auth/email/start', { email });
}

/** A new code; once a minute (429 otherwise). */
export function resendLoginCode(email: string): Promise<{ ok: boolean }> {
  return postJson('/api/auth/email/code/resend', { email });
}

/** The emailed code is right: a one-time token (15 min) for choosing the password. */
export function verifyLoginCode(email: string, code: string): Promise<{ setup_token: string }> {
  return postJson('/api/auth/email/code/verify', { email, code });
}

/** Saves the first password and logs in (sets the session cookie). */
export function setFirstPassword(payload: {
  email: string;
  setup_token: string;
  password: string;
  remember_me?: boolean;
}): Promise<EmailSessionResponse> {
  return postJson('/api/auth/email/password', payload);
}

/** Logs in with email and password (sets the session cookie). */
export function loginWithEmail(payload: {
  email: string;
  password: string;
  remember_me?: boolean;
}): Promise<EmailSessionResponse> {
  return postJson('/api/auth/email/login', payload);
}

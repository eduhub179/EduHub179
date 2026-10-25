import { apiFetch } from './client';

export interface LoginRequest {
  login: string;
  password: string;
}

export interface PublicUserDto {
  id: string;
  login: string;
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

export interface SendAuthCodeRequest {
  email: string;
}

export interface SendAuthCodeResponse {
  email: string;
  message: string;
}

export interface VerifyAuthCodeRequest {
  email: string;
  code: string;
}

export interface VerifyAuthCodeResponse {
  valid: boolean;
  user_id?: string | null;
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

export interface UpdateProfileClassRequest {
  class_id: string;
}

export type UpdateProfileClassResponse = PublicUserDto;

export async function login(payload: LoginRequest): Promise<LoginResponse> {
  return apiFetch<LoginResponse>('/api/auth/login', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

export async function sendAuthCode(
  payload: SendAuthCodeRequest,
): Promise<SendAuthCodeResponse> {
  return apiFetch<SendAuthCodeResponse>('/api/auth/code/send', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

export async function verifyAuthCode(
  payload: VerifyAuthCodeRequest,
): Promise<VerifyAuthCodeResponse> {
  return apiFetch<VerifyAuthCodeResponse>('/api/auth/code/verify', {
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

export async function updateProfileClass(
  payload: UpdateProfileClassRequest,
): Promise<UpdateProfileClassResponse> {
  return apiFetch<UpdateProfileClassResponse>('/api/auth/profile/class', {
    method: 'PATCH',
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

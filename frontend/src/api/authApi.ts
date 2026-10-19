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

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

export async function login(payload: LoginRequest): Promise<LoginResponse> {
  return apiFetch<LoginResponse>('/api/auth/login', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

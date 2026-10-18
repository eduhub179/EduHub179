import { apiFetch } from './client';

export interface SchoolClassDto {
  id: string;
  graduation_year: number;
  letter: string;
  name: string;
  is_active: boolean;
}

export interface SchoolClassesResponse {
  classes: SchoolClassDto[];
}

export async function fetchSchoolClasses(): Promise<SchoolClassesResponse> {
  return apiFetch<SchoolClassesResponse>('/api/classes');
}

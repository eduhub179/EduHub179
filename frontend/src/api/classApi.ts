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

export function formatClassName(name: string, date = new Date()): string {
  const trimmedName = name.trim();
  const graduationYearMatch = trimmedName.match(/^(\d{4})\s*([а-я])$/i);
  const gradeMatch = trimmedName.match(/^(\d{1,2})\s*([а-я])$/i);
  const letter = graduationYearMatch?.[2] ?? gradeMatch?.[2];
  if (!letter) {
    return name;
  }

  const schoolYear = date.getFullYear() - (date.getMonth() < 7 ? 1 : 0);
  const grade = graduationYearMatch
    ? schoolYear + 12 - Number(graduationYearMatch[1])
    : Number(gradeMatch?.[1]);
  return `${grade}${letter.toLocaleUpperCase('ru-RU')}`;
}

export function getSchoolClassLabel(schoolClass: SchoolClassDto, date = new Date()): string {
  return formatClassName(`${schoolClass.graduation_year}${schoolClass.letter}`, date);
}

export async function fetchSchoolClasses(): Promise<SchoolClassesResponse> {
  return apiFetch<SchoolClassesResponse>('/api/classes');
}

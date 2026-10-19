import { apiFetch } from './client';
import { assertCanAccessClassSchedule, assertCanAccessStudentSchedule } from './access';

export interface HomeworkDto {
  id: string;
  text: string;
}

export interface ScheduleLessonDto {
  id: string;
  start_time: string;
  end_time: string;
  subject_name: string;
  teachers: string[];
  cabinet?: string | null;
  homework?: HomeworkDto | null;
}

export interface ScheduleDayDto {
  date: string;
  lessons: ScheduleLessonDto[];
}

export interface WeeklyScheduleDto {
  week_start_date: string;
  days: ScheduleDayDto[];
}

export interface DayScheduleDto {
  date: string;
  lessons: ScheduleLessonDto[];
}

export async function fetchWeekSchedule(
  date: string,
  studentId: string,
): Promise<WeeklyScheduleDto> {
  assertCanAccessStudentSchedule(studentId);
  return apiFetch<WeeklyScheduleDto>(
    `/api/schedule/week?date=${encodeURIComponent(date)}&student_id=${encodeURIComponent(studentId)}`,
  );
}

export async function fetchDaySchedule(
  date: string,
  studentId: string,
): Promise<DayScheduleDto> {
  assertCanAccessStudentSchedule(studentId);
  return apiFetch<DayScheduleDto>(
    `/api/schedule/day?date=${encodeURIComponent(date)}&student_id=${encodeURIComponent(studentId)}`,
  );
}

export async function fetchClassWeekSchedule(
  date: string,
  classId: string,
): Promise<WeeklyScheduleDto> {
  assertCanAccessClassSchedule(classId);
  return apiFetch<WeeklyScheduleDto>(
    `/api/classes/schedule/week?date=${encodeURIComponent(date)}&class_id=${encodeURIComponent(classId)}`,
  );
}

export async function fetchClassDaySchedule(
  date: string,
  classId: string,
): Promise<DayScheduleDto> {
  assertCanAccessClassSchedule(classId);
  return apiFetch<DayScheduleDto>(
    `/api/classes/schedule/day?date=${encodeURIComponent(date)}&class_id=${encodeURIComponent(classId)}`,
  );
}

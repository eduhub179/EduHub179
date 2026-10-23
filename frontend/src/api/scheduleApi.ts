import { apiFetch } from './client';
import {
  assertCanAccessClassSchedule,
  assertCanAccessStudentSchedule,
  requireAuthenticatedUser,
} from './access';

export interface HomeworkFileDto {
  id: string;
  file_name: string;
  mime_type: string;
  size_bytes: number;
  sort_order: number;
}

export interface HomeworkDto {
  id: string;
  text: string;
  /** Contents: homeworkFileUrl(homework.id, file.id). */
  files: HomeworkFileDto[];
}

export interface ScheduleLessonDto {
  id: string;
  start_time: string;
  end_time: string;
  subject_name: string;
  /** Class ("2027б") or group the lesson is for. */
  target_name: string;
  /** Who gives the lesson: the substitute teacher if there is one. */
  teachers: string[];
  /** The usual teachers, only when the lesson has a substitute. */
  usual_teachers?: string[];
  cabinet?: string | null;
  status?: 'scheduled' | 'completed' | 'cancelled';
  note?: string | null;
  /** Added by an admin to this date only (one-off or moved lesson). */
  is_extra?: boolean;
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

/** Week containing `date` for a teacher: lessons of all courses they teach. */
export async function fetchTeacherWeekSchedule(
  date: string,
  teacherId: string,
): Promise<WeeklyScheduleDto> {
  requireAuthenticatedUser();
  return apiFetch<WeeklyScheduleDto>(
    `/api/teachers/schedule/week?date=${encodeURIComponent(date)}&teacher_id=${encodeURIComponent(teacherId)}`,
  );
}

/** One day for a teacher. */
export async function fetchTeacherDaySchedule(
  date: string,
  teacherId: string,
): Promise<DayScheduleDto> {
  requireAuthenticatedUser();
  return apiFetch<DayScheduleDto>(
    `/api/teachers/schedule/day?date=${encodeURIComponent(date)}&teacher_id=${encodeURIComponent(teacherId)}`,
  );
}

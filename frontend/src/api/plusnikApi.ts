import { apiFetch } from './client';
import {
  requireAuthenticatedUser,
  requireStudentUser,
  requireTeacherOrAdminUser,
} from './access';

/** Plusnik API: sheets of a lesson (course), tasks, the teacher's matrix, pluses. */

export type PlusnikSheetStatus = 'draft' | 'published' | 'archived';

/** A course the teacher teaches: subject + class or group. */
export interface PlusnikLessonDto {
  lesson_id: string;
  subject_name: string;
  /** Class ("2027б") or group name. */
  target_name: string;
}

export interface PlusnikTaskDto {
  id: string;
  /** "1а", "2б*", "10". */
  number: string;
  sort_order: number;
}

export interface PlusnikSheetDto {
  id: string;
  lesson_id: string;
  name: string;
  /** YYYY-MM-DD. */
  issue_date: string;
  /** RFC 3339 or null. */
  deadline: string | null;
  status: PlusnikSheetStatus;
  created_by: string;
  tasks: PlusnikTaskDto[];
}

export interface PlusnikPlusDto {
  record_id: string;
  task_id: string;
  student_id: string;
  granted_by: string;
  granted_at: string;
}

export interface PlusnikMatrixDto {
  /** Sheet with tasks (columns). */
  sheet: PlusnikSheetDto;
  /** Students of the lesson (rows), sorted by name. */
  students: { id: string; name: string }[];
  /** Active pluses (filled cells). */
  pluses: PlusnikPlusDto[];
}

export interface PlusnikSheetProgressDto {
  /** Sheet without tasks. */
  sheet: PlusnikSheetDto;
  subject_name: string;
  total_tasks: number;
  solved_tasks: number;
  solved_task_ids: string[];
}

export interface CreatePlusnikSheetRequest {
  name: string;
  /** YYYY-MM-DD. */
  issue_date: string;
  /** RFC 3339, e.g. 2026-10-27T18:00:00+03:00. */
  deadline?: string | null;
  /** Defaults to draft. */
  status?: PlusnikSheetStatus;
  /** Task numbers in display order. */
  tasks?: string[];
}

/** Only the given fields change; `deadline: null` clears the deadline. */
export interface UpdatePlusnikSheetRequest {
  name?: string;
  issue_date?: string;
  deadline?: string | null;
  status?: PlusnikSheetStatus;
}

export interface GrantPlusResponse {
  plus: PlusnikPlusDto;
  /** False if the plus was already there. */
  created: boolean;
}

export interface RevokePlusResponse {
  /** False if there was no active plus. */
  revoked: boolean;
}

const jsonInit = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});

/** Courses where the current teacher can manage sheets and pluses. */
export async function fetchPlusnikLessons(): Promise<PlusnikLessonDto[]> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikLessonDto[]>('/api/plusnik/lessons');
}

/** Sheets of a course, newest first. Students get only published ones. */
export async function fetchPlusnikSheets(lessonId: string): Promise<PlusnikSheetDto[]> {
  requireAuthenticatedUser();
  return apiFetch<PlusnikSheetDto[]>(`/api/plusnik/lessons/${lessonId}/sheets`);
}

export async function createPlusnikSheet(
  lessonId: string,
  payload: CreatePlusnikSheetRequest,
): Promise<PlusnikSheetDto> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikSheetDto>(
    `/api/plusnik/lessons/${lessonId}/sheets`,
    jsonInit('POST', payload),
  );
}

export async function updatePlusnikSheet(
  sheetId: string,
  payload: UpdatePlusnikSheetRequest,
): Promise<PlusnikSheetDto> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikSheetDto>(`/api/plusnik/sheets/${sheetId}`, jsonInit('PATCH', payload));
}

/** Fails with 409 if the sheet has pluses: archive it instead. */
export async function deletePlusnikSheet(sheetId: string): Promise<void> {
  requireTeacherOrAdminUser();
  await apiFetch<void>(`/api/plusnik/sheets/${sheetId}`, { method: 'DELETE' });
}

/** The teacher's screen: tasks × students with active pluses. */
export async function fetchPlusnikMatrix(sheetId: string): Promise<PlusnikMatrixDto> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikMatrixDto>(`/api/plusnik/sheets/${sheetId}/matrix`);
}

/** Appends a task to the end of the sheet. */
export async function addPlusnikTask(sheetId: string, number: string): Promise<PlusnikTaskDto> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikTaskDto>(
    `/api/plusnik/sheets/${sheetId}/tasks`,
    jsonInit('POST', { number }),
  );
}

/** Fails with 409 if the task has pluses. */
export async function deletePlusnikTask(taskId: string): Promise<void> {
  requireTeacherOrAdminUser();
  await apiFetch<void>(`/api/plusnik/tasks/${taskId}`, { method: 'DELETE' });
}

/** Puts a plus; safe to repeat (returns created: false). Only on published sheets. */
export async function grantPlus(taskId: string, studentId: string): Promise<GrantPlusResponse> {
  requireTeacherOrAdminUser();
  return apiFetch<GrantPlusResponse>(`/api/plusnik/tasks/${taskId}/pluses/${studentId}`, {
    method: 'PUT',
  });
}

/** Removes a plus; the record stays in history with the optional comment. */
export async function revokePlus(
  taskId: string,
  studentId: string,
  comment?: string,
): Promise<RevokePlusResponse> {
  requireTeacherOrAdminUser();
  return apiFetch<RevokePlusResponse>(
    `/api/plusnik/tasks/${taskId}/pluses/${studentId}`,
    comment ? jsonInit('DELETE', { comment }) : { method: 'DELETE' },
  );
}

/** The student's dashboard: solved/total per published sheet. */
export async function fetchMyPlusnikProgress(): Promise<PlusnikSheetProgressDto[]> {
  requireStudentUser();
  return apiFetch<PlusnikSheetProgressDto[]>('/api/plusnik/me');
}

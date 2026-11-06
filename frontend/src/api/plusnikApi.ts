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

/** A cell of the matrix. */
export interface PlusnikCell {
  task_id: string;
  student_id: string;
}

export interface PlusBatchRequest {
  grant?: PlusnikCell[];
  revoke?: PlusnikCell[];
  /** Stored on every revoked record. */
  comment?: string;
}

export interface PlusBatchResponse {
  /** Pluses actually added (present ones are skipped). */
  granted: number;
  /** Pluses actually removed (absent ones are skipped). */
  revoked: number;
  /** All active pluses of the sheet after the change. */
  pluses: PlusnikPlusDto[];
}

export interface PlusnikCourseSummaryDto {
  lesson: PlusnikLessonDto;
  /** Published and archived sheets, oldest first. */
  sheets: {
    id: string;
    name: string;
    issue_date: string;
    deadline: string | null;
    status: PlusnikSheetStatus;
    total_tasks: number;
  }[];
  /** Sorted by name; `solved[i]` is for `sheets[i]`. */
  students: { id: string; name: string; solved: number[]; solved_total: number }[];
  total_tasks: number;
}

export interface PlusnikHistoryEntryDto {
  record_id: string;
  task_id: string;
  task_number: string;
  student_id: string;
  student_name: string;
  granted_by: string;
  granted_by_name: string;
  granted_at: string;
  /** Null while the plus is active. */
  revoked_at: string | null;
  revoked_by: string | null;
  revoked_by_name: string | null;
  revoke_comment: string | null;
}

/**
 * Courses where the current user can manage sheets and pluses: every active course of the
 * school for teachers and admins alike. With `classId`, only courses of that class — whole-class courses and
 * group courses its students attend.
 */
export async function fetchPlusnikLessons(classId?: string): Promise<PlusnikLessonDto[]> {
  requireTeacherOrAdminUser();
  const query = classId ? `?class_id=${encodeURIComponent(classId)}` : '';
  return apiFetch<PlusnikLessonDto[]>(`/api/plusnik/lessons${query}`);
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

/**
 * The plus table: tasks × students with active pluses. Teachers of the course and admins
 * get any sheet, students of the course only published ones (read-only).
 */
export async function fetchPlusnikMatrix(sheetId: string): Promise<PlusnikMatrixDto> {
  requireAuthenticatedUser();
  return apiFetch<PlusnikMatrixDto>(`/api/plusnik/sheets/${sheetId}/matrix`);
}

/**
 * Adds a task. Without `position` (0-based) it goes to the end; tasks after it shift down.
 */
export async function addPlusnikTask(
  sheetId: string,
  number: string,
  position?: number,
): Promise<PlusnikTaskDto> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikTaskDto>(
    `/api/plusnik/sheets/${sheetId}/tasks`,
    jsonInit('POST', { number, position }),
  );
}

/** Adds several tasks at once (all or nothing); returns all tasks of the sheet in order. */
export async function addPlusnikTasks(
  sheetId: string,
  numbers: string[],
  position?: number,
): Promise<PlusnikTaskDto[]> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikTaskDto[]>(
    `/api/plusnik/sheets/${sheetId}/tasks/batch`,
    jsonInit('POST', { numbers, position }),
  );
}

/** Renames a task ("1а" → "1а*"); its pluses stay. 409 if the number is taken. */
export async function renamePlusnikTask(taskId: string, number: string): Promise<PlusnikTaskDto> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikTaskDto>(`/api/plusnik/tasks/${taskId}`, jsonInit('PATCH', { number }));
}

/** New order of the sheet's tasks: must list every task exactly once. */
export async function reorderPlusnikTasks(
  sheetId: string,
  taskIds: string[],
): Promise<PlusnikTaskDto[]> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikTaskDto[]>(
    `/api/plusnik/sheets/${sheetId}/tasks/order`,
    jsonInit('PUT', { task_ids: taskIds }),
  );
}

/** Deletes several tasks, all or nothing (409 if any has pluses); returns the rest. */
export async function deletePlusnikTasks(
  sheetId: string,
  taskIds: string[],
): Promise<PlusnikTaskDto[]> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikTaskDto[]>(
    `/api/plusnik/sheets/${sheetId}/tasks`,
    jsonInit('DELETE', { task_ids: taskIds }),
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

/**
 * Many pluses of one sheet in one transaction: a whole column, a whole row, any cells.
 * Repeat-safe. Grants need a published sheet; revokes work on any status.
 */
export async function applyPluses(
  sheetId: string,
  payload: PlusBatchRequest,
): Promise<PlusBatchResponse> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusBatchResponse>(
    `/api/plusnik/sheets/${sheetId}/pluses`,
    jsonInit('POST', payload),
  );
}

/** Students × sheets of a course with solved counts (published and archived sheets). */
export async function fetchPlusnikCourseSummary(lessonId: string): Promise<PlusnikCourseSummaryDto> {
  requireTeacherOrAdminUser();
  return apiFetch<PlusnikCourseSummaryDto>(`/api/plusnik/lessons/${lessonId}/summary`);
}

/** Every plus ever put on the sheet (revoked included), newest first, with names. */
export async function fetchPlusnikHistory(
  sheetId: string,
  filter: { studentId?: string; taskId?: string } = {},
): Promise<PlusnikHistoryEntryDto[]> {
  requireTeacherOrAdminUser();
  const params = new URLSearchParams();
  if (filter.studentId) params.set('student_id', filter.studentId);
  if (filter.taskId) params.set('task_id', filter.taskId);
  const query = params.toString() ? `?${params.toString()}` : '';
  return apiFetch<PlusnikHistoryEntryDto[]>(`/api/plusnik/sheets/${sheetId}/history${query}`);
}

/** The student's dashboard: solved/total per published sheet. */
export async function fetchMyPlusnikProgress(): Promise<PlusnikSheetProgressDto[]> {
  requireStudentUser();
  return apiFetch<PlusnikSheetProgressDto[]>('/api/plusnik/me');
}

import { apiFetch, apiUrl } from './client';
import type { HomeworkFileDto } from './scheduleApi';
import { requireTeacherUser } from './access';

export interface CreateHomeworkRequest {
  lesson_instance_id: string;
  created_by: string;
  created_by_role: 'student' | 'teacher' | 'admin' | string;
  text_content?: string | null;
  status: 'draft' | 'published' | 'archived' | string;
}

export interface UpdateHomeworkTextRequest {
  text_content?: string | null;
}

export interface HomeworkResponse {
  id: string;
  lesson_instance_id: string;
  created_by: string;
  created_by_role: string;
  text_content?: string | null;
  status: string;
  locked_by_teacher: boolean;
  last_edited_by?: string | null;
  created_at: string;
}

export async function createHomework(
  payload: CreateHomeworkRequest,
): Promise<HomeworkResponse> {
  requireTeacherUser();
  return apiFetch<HomeworkResponse>('/api/homework', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

export async function updateHomeworkText(
  homeworkId: string,
  payload: UpdateHomeworkTextRequest,
): Promise<HomeworkResponse> {
  requireTeacherUser();
  return apiFetch<HomeworkResponse>(`/api/homework/${homeworkId}/text`, {
    method: 'PATCH',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

export interface AttachLessonFileResponse {
  homework_id: string;
  /** True if the lesson had no homework and it was created for this file. */
  homework_created: boolean;
  file: HomeworkFileDto;
}

/** Uploads a file and attaches it to the lesson's homework (created if the lesson has none). */
export async function attachLessonFile(
  lessonInstanceId: string,
  file: File,
): Promise<AttachLessonFileResponse> {
  requireTeacherUser();
  const body = new FormData();
  body.append('file', file);
  return apiFetch<AttachLessonFileResponse>(`/api/lessons/${lessonInstanceId}/files`, {
    method: 'POST',
    body,
  });
}

/** Deletes the homework with its text and all attached files. */
export async function deleteHomework(homeworkId: string): Promise<void> {
  requireTeacherUser();
  await apiFetch<void>(`/api/homework/${homeworkId}`, { method: 'DELETE' });
}

export interface DeleteHomeworkFileResponse {
  /** True if the homework had no text and this was its last file, so it was deleted too. */
  homework_deleted: boolean;
}

export async function deleteHomeworkFile(fileId: string): Promise<DeleteHomeworkFileResponse> {
  requireTeacherUser();
  return apiFetch<DeleteHomeworkFileResponse>(`/api/homework/files/${fileId}`, {
    method: 'DELETE',
  });
}

/** Link for opening/downloading a file; the session cookie authorizes the request. */
export function homeworkFileUrl(homeworkId: string, fileId: string): string {
  return apiUrl(`/api/homework/${homeworkId}/files/${fileId}`);
}

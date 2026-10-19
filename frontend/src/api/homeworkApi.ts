import { apiFetch } from './client';
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

import { apiFetch } from './client';

export interface CreateHomeworkRequest {
  lesson_instance_id: string;
  text: string;
}

export interface UpdateHomeworkTextRequest {
  text: string;
}

export interface HomeworkResponse {
  id: string;
  lesson_instance_id: string;
  text: string;
  created_at?: string;
  updated_at?: string;
}

export async function createHomework(
  payload: CreateHomeworkRequest,
): Promise<HomeworkResponse> {
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
  return apiFetch<HomeworkResponse>(`/api/homework/${homeworkId}/text`, {
    method: 'PATCH',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

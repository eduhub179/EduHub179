import { apiFetch, apiUrl } from './client';
import { requireAuthenticatedUser } from './access';

/**
 * Course materials API. A course is a subject for a class or group ("Матанализ — 2027в");
 * its materials (files and links) are grouped by optional topics.
 * Access is checked by the backend: staff view any course, students the courses they
 * attend; `can_manage` tells whether to show editing controls.
 */

export interface CourseDto {
  lesson_id: string;
  subject_name: string;
  /** Class ("2027в") or group name. */
  target_name: string;
  /** The current user may add / edit / delete materials. */
  can_manage: boolean;
}

export interface MaterialTopicDto {
  id: string;
  name: string;
  sort_order: number;
  material_count: number;
}

export interface MaterialTopicsDto {
  topics: MaterialTopicDto[];
  /** Materials without a topic ("Без темы"). */
  without_topic_count: number;
  total_count: number;
}

export interface MaterialDto {
  id: string;
  lesson_id: string;
  /** null = "Без темы". */
  topic_id: string | null;
  title: string;
  description: string | null;
  kind: 'file' | 'link';
  /** For links. */
  url: string | null;
  /** For files; contents via materialFileUrl(id). */
  file: { file_name: string; mime_type: string; size_bytes: number } | null;
  sort_order: number;
  created_by: string;
  created_at: string;
}

/** Which materials to load: all of the course, those without a topic, or one topic. */
export type MaterialTopicSelector = 'all' | 'none' | { topicId: string };

export interface AddMaterialLinkRequest {
  title: string;
  /** http(s) only. */
  url: string;
  description?: string | null;
  topic_id?: string | null;
}

export interface UploadMaterialOptions {
  /** Defaults to the file name. */
  title?: string;
  description?: string;
  topicId?: string | null;
}

/** Only the given fields change; `description: null` clears it, `topic_id: null` moves to "Без темы". */
export interface UpdateMaterialRequest {
  title?: string;
  description?: string | null;
  topic_id?: string | null;
  sort_order?: number;
}

const jsonInit = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});

/** Courses of a class: staff get all of them, a student only those they attend. */
export async function fetchClassCourses(classId: string): Promise<CourseDto[]> {
  requireAuthenticatedUser();
  return apiFetch<CourseDto[]>(`/api/courses?class_id=${encodeURIComponent(classId)}`);
}

export async function fetchCourse(lessonId: string): Promise<CourseDto> {
  requireAuthenticatedUser();
  return apiFetch<CourseDto>(`/api/courses/${lessonId}`);
}

/** Topics with material counts (one request for the whole sidebar). */
export async function fetchMaterialTopics(lessonId: string): Promise<MaterialTopicsDto> {
  requireAuthenticatedUser();
  return apiFetch<MaterialTopicsDto>(`/api/courses/${lessonId}/material-topics`);
}

export async function createMaterialTopic(lessonId: string, name: string): Promise<MaterialTopicDto> {
  requireAuthenticatedUser();
  return apiFetch<MaterialTopicDto>(
    `/api/courses/${lessonId}/material-topics`,
    jsonInit('POST', { name }),
  );
}

export async function updateMaterialTopic(
  topicId: string,
  payload: { name?: string; sort_order?: number },
): Promise<MaterialTopicDto> {
  requireAuthenticatedUser();
  return apiFetch<MaterialTopicDto>(`/api/material-topics/${topicId}`, jsonInit('PATCH', payload));
}

/** Its materials are not deleted: they move to "Без темы". */
export async function deleteMaterialTopic(topicId: string): Promise<void> {
  requireAuthenticatedUser();
  await apiFetch<void>(`/api/material-topics/${topicId}`, { method: 'DELETE' });
}

export async function fetchMaterials(
  lessonId: string,
  selector: MaterialTopicSelector = 'all',
): Promise<MaterialDto[]> {
  requireAuthenticatedUser();
  const query = selector === 'all'
    ? ''
    : `?topic=${encodeURIComponent(selector === 'none' ? 'none' : selector.topicId)}`;
  return apiFetch<MaterialDto[]>(`/api/courses/${lessonId}/materials${query}`);
}

export async function addMaterialLink(
  lessonId: string,
  payload: AddMaterialLinkRequest,
): Promise<MaterialDto> {
  requireAuthenticatedUser();
  return apiFetch<MaterialDto>(
    `/api/courses/${lessonId}/materials/links`,
    jsonInit('POST', payload),
  );
}

export async function uploadMaterialFile(
  lessonId: string,
  file: File,
  options: UploadMaterialOptions = {},
): Promise<MaterialDto> {
  requireAuthenticatedUser();
  const body = new FormData();
  body.append('file', file);
  if (options.title?.trim()) body.append('title', options.title.trim());
  if (options.description?.trim()) body.append('description', options.description.trim());
  if (options.topicId) body.append('topic_id', options.topicId);
  return apiFetch<MaterialDto>(`/api/courses/${lessonId}/materials/files`, {
    method: 'POST',
    body,
  });
}

export async function updateMaterial(
  materialId: string,
  payload: UpdateMaterialRequest,
): Promise<MaterialDto> {
  requireAuthenticatedUser();
  return apiFetch<MaterialDto>(`/api/materials/${materialId}`, jsonInit('PATCH', payload));
}

export async function deleteMaterial(materialId: string): Promise<void> {
  requireAuthenticatedUser();
  await apiFetch<void>(`/api/materials/${materialId}`, { method: 'DELETE' });
}

/** Link for opening/downloading a material file; the browser caches it (immutable). */
export function materialFileUrl(materialId: string): string {
  return apiUrl(`/api/materials/${materialId}/file`);
}

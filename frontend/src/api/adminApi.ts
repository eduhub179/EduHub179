import { apiFetch } from './client';

/** Admin panel API (`/api/admin/*`). Every call needs a session of an admin. */

export type UserRole = 'student' | 'teacher' | 'admin';

export interface AdminUserDto {
  id: string;
  login: string;
  role: UserRole;
  last_name: string;
  first_name: string;
  middle_name: string | null;
  is_active: boolean;
  class_id: string | null;
  class_name: string | null;
  telegram_bound: boolean;
}

export interface AdminUserListDto {
  users: AdminUserDto[];
  truncated: boolean;
}

export interface UserFilter {
  q?: string;
  role?: UserRole | '';
  classId?: string;
  includeInactive?: boolean;
}

export interface NewUserInput {
  login: string;
  role: UserRole;
  last_name: string;
  first_name: string;
  middle_name?: string | null;
  class_id?: string | null;
}

/** Only the fields that change; `class_id: null` removes the class. */
export interface UserChangeInput {
  role?: UserRole;
  class_id?: string | null;
  is_active?: boolean;
  last_name?: string;
  first_name?: string;
  middle_name?: string | null;
}

export interface PersonDto {
  id: string;
  name: string;
}

export type LessonStatus = 'scheduled' | 'completed' | 'cancelled';

export interface AdminLessonDto {
  id: string;
  date: string;
  start_time: string;
  end_time: string;
  subject_name: string;
  target_name: string;
  teachers: PersonDto[];
  substitute_teacher: PersonDto | null;
  note: string | null;
  cabinet: { id: string; number: number } | null;
  status: LessonStatus;
  has_homework: boolean;
  /** Added by an admin to this date only (one-off or moved lesson). */
  is_extra: boolean;
}

/** Only the fields that change; `null` clears a cabinet override, substitute or note. */
export interface LessonChangeInput {
  status?: LessonStatus;
  cabinet_id?: string | null;
  substitute_teacher_id?: string | null;
  note?: string | null;
}

export interface CabinetDto {
  id: string;
  number: number;
  description: string | null;
}

export interface WeekDto {
  week_start_date: string;
  status: 'draft' | 'published';
  copied_from: string | null;
}

export type WeekSource = 'empty' | 'templates' | 'copy';

export type AnnouncementStatus = 'draft' | 'published' | 'archived';
export type AnnouncementPriority = 'low' | 'normal' | 'high' | 'urgent';

export interface AnnouncementDto {
  id: string;
  title: string | null;
  content: string | null;
  status: AnnouncementStatus;
  priority: AnnouncementPriority;
  expires_at: string | null;
  created_at: string;
}

export interface AdminAnnouncementDto extends AnnouncementDto {
  class_ids: string[];
  group_ids: string[];
  user_ids: string[];
  author_name: string;
}

export interface AnnouncementInput {
  title: string | null;
  content: string | null;
  status: AnnouncementStatus;
  priority: AnnouncementPriority;
  expires_at: string | null;
  /** All three empty = everyone. */
  class_ids: string[];
  group_ids: string[];
  user_ids: string[];
}

const json = (body: unknown): RequestInit['body'] => JSON.stringify(body);

export async function fetchAdminUsers(filter: UserFilter): Promise<AdminUserListDto> {
  const params = new URLSearchParams();
  if (filter.q?.trim()) params.set('q', filter.q.trim());
  if (filter.role) params.set('role', filter.role);
  if (filter.classId) params.set('class_id', filter.classId);
  if (filter.includeInactive) params.set('include_inactive', 'true');
  return apiFetch<AdminUserListDto>(`/api/admin/users?${params.toString()}`);
}

export async function createAdminUser(input: NewUserInput): Promise<{ id: string }> {
  return apiFetch<{ id: string }>('/api/admin/users', { method: 'POST', body: json(input) });
}

/** Sets the user's website password (8–128 characters). */
export async function setAdminUserPassword(userId: string, password: string): Promise<void> {
  await apiFetch(`/api/admin/users/${encodeURIComponent(userId)}/password`, {
    method: 'PUT',
    body: json({ password }),
  });
}

export async function updateAdminUser(userId: string, change: UserChangeInput): Promise<void> {
  await apiFetch(`/api/admin/users/${encodeURIComponent(userId)}`, {
    method: 'PATCH',
    body: json(change),
  });
}

export async function fetchCabinets(): Promise<CabinetDto[]> {
  const response = await apiFetch<{ cabinets: CabinetDto[] }>('/api/admin/cabinets');
  return response.cabinets;
}

export async function createCabinet(number: number, description: string | null): Promise<CabinetDto> {
  return apiFetch<CabinetDto>('/api/admin/cabinets', {
    method: 'POST',
    body: json({ number, description }),
  });
}

export async function fetchAdminLessons(date: string): Promise<AdminLessonDto[]> {
  const response = await apiFetch<{ date: string; lessons: AdminLessonDto[] }>(
    `/api/admin/lessons?date=${encodeURIComponent(date)}`,
  );
  return response.lessons;
}

export async function updateAdminLesson(
  instanceId: string,
  change: LessonChangeInput,
): Promise<AdminLessonDto> {
  return apiFetch<AdminLessonDto>(`/api/admin/lessons/${encodeURIComponent(instanceId)}`, {
    method: 'PATCH',
    body: json(change),
  });
}

export async function fetchWeeks(): Promise<WeekDto[]> {
  const response = await apiFetch<{ weeks: WeekDto[] }>('/api/admin/weeks');
  return response.weeks;
}

export async function createWeek(
  weekStartDate: string,
  source: WeekSource,
  copyFrom?: string,
): Promise<WeekDto> {
  return apiFetch<WeekDto>('/api/admin/weeks', {
    method: 'POST',
    body: json({ week_start_date: weekStartDate, source, copy_from: copyFrom ?? null }),
  });
}

export async function setWeekStatus(
  weekStartDate: string,
  status: WeekDto['status'],
): Promise<WeekDto> {
  return apiFetch<WeekDto>(`/api/admin/weeks/${encodeURIComponent(weekStartDate)}`, {
    method: 'PATCH',
    body: json({ status }),
  });
}

export async function fetchAdminAnnouncements(): Promise<AdminAnnouncementDto[]> {
  const response = await apiFetch<{ announcements: AdminAnnouncementDto[] }>(
    '/api/admin/announcements',
  );
  return response.announcements;
}

export async function createAnnouncement(input: AnnouncementInput): Promise<AnnouncementDto> {
  return apiFetch<AnnouncementDto>('/api/admin/announcements', {
    method: 'POST',
    body: json(input),
  });
}

export async function updateAnnouncement(
  id: string,
  input: AnnouncementInput,
): Promise<AnnouncementDto> {
  return apiFetch<AnnouncementDto>(`/api/admin/announcements/${encodeURIComponent(id)}`, {
    method: 'PUT',
    body: json(input),
  });
}

export async function deleteAnnouncement(id: string): Promise<void> {
  await apiFetch(`/api/admin/announcements/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Announcements the current user should see (any role). */
export async function fetchMyAnnouncements(): Promise<AnnouncementDto[]> {
  const response = await apiFetch<{ announcements: AnnouncementDto[] }>('/api/announcements');
  return response.announcements;
}

// ---------------------------------------------------------------------------
// Timetable: courses, regular slots, one-off and moved lessons
// ---------------------------------------------------------------------------

export type DayCode = 'mon' | 'tue' | 'wed' | 'thu' | 'fri' | 'sat';
export type Parity = 'every' | 'odd' | 'even';

export interface NamedDto {
  id: string;
  name: string;
}

/** A course: class or group + subject, with its usual teachers. */
export interface TimetableCourseDto {
  id: string;
  subject: NamedDto;
  class_id: string | null;
  group_id: string | null;
  target_name: string;
  teachers: PersonDto[];
}

/** A slot of the regular timetable. */
export interface SlotDto {
  id: string;
  course_id: string;
  subject_name: string;
  target_name: string;
  teachers: PersonDto[];
  day: DayCode;
  start_time: string;
  end_time: string;
  parity: Parity;
  cabinet_id: string | null;
  cabinet_number: number | null;
}

export interface ClassTimetableDto {
  courses: TimetableCourseDto[];
  slots: SlotDto[];
}

export interface SlotInput {
  course_id: string;
  day: DayCode;
  start_time: string;
  end_time: string;
  parity: Parity;
  cabinet_id: string | null;
  /** Also change already created weeks from this date on. */
  apply_from: string | null;
}

/** What a timetable change did to already created weeks. */
export interface TimetableChangeDto {
  changed: number;
  /** Left as they were because they have homework. */
  kept: number;
}

export interface LessonPlaceInput {
  date: string;
  start_time: string;
  end_time: string;
  cabinet_id: string | null;
}

export async function fetchSubjects(): Promise<NamedDto[]> {
  const response = await apiFetch<{ subjects: NamedDto[] }>('/api/admin/subjects');
  return response.subjects;
}

export async function createSubject(name: string): Promise<NamedDto> {
  return apiFetch<NamedDto>('/api/admin/subjects', { method: 'POST', body: json({ name }) });
}

export async function fetchGroups(): Promise<NamedDto[]> {
  const response = await apiFetch<{ groups: NamedDto[] }>('/api/admin/groups');
  return response.groups;
}

export async function fetchClassTimetable(classId: string): Promise<ClassTimetableDto> {
  return apiFetch<ClassTimetableDto>(
    `/api/admin/classes/${encodeURIComponent(classId)}/timetable`,
  );
}

export async function createCourse(input: {
  class_id: string | null;
  group_id: string | null;
  subject_id: string;
  teacher_ids: string[];
}): Promise<TimetableCourseDto> {
  return apiFetch<TimetableCourseDto>('/api/admin/courses', { method: 'POST', body: json(input) });
}

export async function setCourseTeachers(courseId: string, teacherIds: string[]): Promise<TimetableCourseDto> {
  return apiFetch<TimetableCourseDto>(`/api/admin/courses/${encodeURIComponent(courseId)}/teachers`, {
    method: 'PUT',
    body: json({ teacher_ids: teacherIds }),
  });
}

export async function addSlot(input: SlotInput): Promise<TimetableChangeDto> {
  return apiFetch<TimetableChangeDto>('/api/admin/slots', { method: 'POST', body: json(input) });
}

export async function updateSlot(slotId: string, input: SlotInput): Promise<TimetableChangeDto> {
  return apiFetch<TimetableChangeDto>(`/api/admin/slots/${encodeURIComponent(slotId)}`, {
    method: 'PUT',
    body: json(input),
  });
}

export async function removeSlot(slotId: string, applyFrom: string | null): Promise<TimetableChangeDto> {
  const query = applyFrom ? `?apply_from=${encodeURIComponent(applyFrom)}` : '';
  return apiFetch<TimetableChangeDto>(`/api/admin/slots/${encodeURIComponent(slotId)}${query}`, {
    method: 'DELETE',
  });
}

/** Adds a lesson of `courseId` to one date only. */
export async function addOneOffLesson(
  courseId: string,
  place: LessonPlaceInput,
): Promise<AdminLessonDto> {
  return apiFetch<AdminLessonDto>('/api/admin/lessons', {
    method: 'POST',
    body: json({ course_id: courseId, ...place }),
  });
}

/** Moves one lesson (with its homework); answers with the lesson at its new place. */
export async function moveLesson(instanceId: string, place: LessonPlaceInput): Promise<AdminLessonDto> {
  return apiFetch<AdminLessonDto>(`/api/admin/lessons/${encodeURIComponent(instanceId)}/move`, {
    method: 'POST',
    body: json(place),
  });
}

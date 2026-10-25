import { authService, type User } from '../auth/authService';

const normalizeRole = (role: User['role'] | undefined): string | null =>
  typeof role === 'string' ? role.trim().toLowerCase() : null;

export function hasTeacherRole(user: User | null | undefined): boolean {
  return normalizeRole(user?.role) === 'teacher';
}

export function hasAdminRole(user: User | null | undefined): boolean {
  return normalizeRole(user?.role) === 'admin';
}

export function hasTeacherOrAdminRole(user: User | null | undefined): boolean {
  return hasTeacherRole(user) || hasAdminRole(user);
}

export function hasStudentRole(user: User | null | undefined): boolean {
  return normalizeRole(user?.role) === 'student';
}

export function getAuthorizedUser(): User | null {
  return authService.getCurrentUser();
}

export function requireAuthenticatedUser(): User {
  const user = getAuthorizedUser();
  if (!user?.id) {
    throw new Error('Нет доступа. Войдите в аккаунт.');
  }
  return user;
}

export function requireTeacherUser(): User {
  const user = requireAuthenticatedUser();
  if (!hasTeacherRole(user)) {
    throw new Error('Нет доступа. Это действие доступно только учителю.');
  }
  return user;
}

export function requireTeacherOrAdminUser(): User {
  const user = requireAuthenticatedUser();
  if (!hasTeacherOrAdminRole(user)) {
    throw new Error('Нет доступа. Это действие доступно только учителю.');
  }
  return user;
}

export function requireStudentUser(): User {
  const user = requireAuthenticatedUser();
  if (!hasStudentRole(user)) {
    throw new Error('Нет доступа. Этот раздел доступен только ученику.');
  }
  return user;
}

export function assertCanAccessStudentSchedule(studentId: string): User {
  const user = requireAuthenticatedUser();

  if (hasStudentRole(user) && user.id !== studentId) {
    throw new Error('Нет доступа к расписанию другого ученика.');
  }

  return user;
}

export function assertCanAccessClassSchedule(classId: string): User {
  const user = requireAuthenticatedUser();

  if (!classId.trim()) {
    throw new Error('Выберите класс для просмотра расписания.');
  }

  return user;
}

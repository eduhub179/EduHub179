import { authService, type User } from '../auth/authService';

const normalizeRole = (role: User['role']): string | null =>
  typeof role === 'string' ? role.trim().toLowerCase() : null;

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
  const role = normalizeRole(user.role);
  if (role !== 'teacher') {
    throw new Error('Нет доступа. Это действие доступно только учителю.');
  }
  return user;
}

export function assertCanAccessStudentSchedule(studentId: string): User {
  const user = requireAuthenticatedUser();
  const role = normalizeRole(user.role);

  if (role === 'student' && user.id !== studentId) {
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

import {
    loginWithEmail,
    logoutSession,
    setFirstPassword,
    updateProfileClass,
    type PublicUserDto,
} from '../api/authApi';

export interface User {
    id: string | null;
    name: string;
    email: string;
    role: 'student' | 'teacher' | 'admin' | null;
    classId: string | null;
    schoolClass: string | null;
}

export interface Credentials {
    email: string;
    password: string;
    /** Keep the session for 30 days instead of 12 hours. */
    rememberMe?: boolean;
}

/** The first login: the token from the emailed code and the chosen password. */
export interface FirstPassword {
    email: string;
    setupToken: string;
    password: string;
    rememberMe?: boolean;
}

export interface AuthService {
    getCurrentUser(): User | null;
    login(credentials: Credentials): Promise<User>;
    /** Saves the first password (after the emailed code) and logs in. */
    completeFirstLogin(data: FirstPassword): Promise<User>;
    updateSchoolClass(schoolClass: string): Promise<User>;
    logout(): Promise<void>;
    /** Drops the locally stored user without calling the backend (the session is already gone). */
    forgetCurrentUser(): void;
}

const CURRENT_USER_KEY = 'eduhub.demo.currentUser';
class LocalStorageAuthService implements AuthService {
    getCurrentUser(): User | null {
        const storedUser = localStorage.getItem(CURRENT_USER_KEY);
        if (!storedUser) {
            return null;
        }
        try {
            return this.parseUser(storedUser);
        } catch {
            // Corrupted or old-format record (saved before `id` existed, without a server session):
            // drop it and treat the user as logged out instead of crashing the whole app on render.
            localStorage.removeItem(CURRENT_USER_KEY);
            return null;
        }
    }

    async login(credentials: Credentials): Promise<User> {
        const session = await loginWithEmail({
            email: credentials.email.trim(),
            password: credentials.password,
            remember_me: credentials.rememberMe ?? true,
        });
        return this.remember(session.user);
    }

    async completeFirstLogin(data: FirstPassword): Promise<User> {
        const session = await setFirstPassword({
            email: data.email.trim(),
            setup_token: data.setupToken,
            password: data.password,
            remember_me: data.rememberMe ?? true,
        });
        return this.remember(session.user);
    }

    /** Stores the logged-in user (the session itself is the HttpOnly cookie). */
    private remember(dto: PublicUserDto): User {
        const user: User = {
            id: dto.id,
            name: [dto.first_name, dto.middle_name, dto.last_name].filter(Boolean).join(' '),
            email: dto.email,
            role: (dto.role as User['role']) ?? null,
            classId: dto.class_id ?? null,
            schoolClass: dto.class_id ?? null,
        };
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(user));
        return user;
    }

    async updateSchoolClass(schoolClass: string): Promise<User> {
        const currentUser = this.getCurrentUser();
        if (!currentUser) {
            throw new Error('Войдите в аккаунт, чтобы изменить профиль.');
        }

        const updated = await updateProfileClass({ class_id: schoolClass });

        const updatedUser: User = {
            ...currentUser,
            role: (updated.role as User['role']) ?? currentUser.role,
            classId: updated.class_id ?? null,
            schoolClass: updated.class_id ?? null,
        };
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(updatedUser));
        return updatedUser;
    }

    async logout(): Promise<void> {
        try {
            await logoutSession();
        } finally {
            localStorage.removeItem(CURRENT_USER_KEY);
        }
    }

    forgetCurrentUser(): void {
        localStorage.removeItem(CURRENT_USER_KEY);
    }

    private parseUser(value: string): User {
        const user: unknown = JSON.parse(value);
        if (
            typeof user !== 'object' ||
            user === null ||
            !('id' in user) ||
            (user.id !== null && typeof user.id !== 'string') ||
            !('name' in user) ||
            typeof user.name !== 'string' ||
            !('email' in user) ||
            typeof user.email !== 'string' ||
            ('role' in user && user.role !== null && typeof user.role !== 'string') ||
            ('classId' in user && user.classId !== null && typeof user.classId !== 'string') ||
            ('schoolClass' in user &&
                user.schoolClass !== null &&
                typeof user.schoolClass !== 'string')
        ) {
            throw new Error('Не удалось прочитать данные аккаунта.');
        }
        return {
            id: 'id' in user && typeof user.id === 'string' ? user.id : null,
            name: user.name,
            email: user.email,
            role: 'role' in user && typeof user.role === 'string'
                ? (user.role as User['role'])
                : null,
            classId: 'classId' in user && typeof user.classId === 'string'
                ? user.classId
                : null,
            schoolClass: 'schoolClass' in user && typeof user.schoolClass === 'string'
                ? user.schoolClass
                : null,
        };
    }
}

// Replace this demo adapter with an API-backed implementation when authentication is added.
export const authService: AuthService = new LocalStorageAuthService();

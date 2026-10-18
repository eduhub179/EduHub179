import { login as loginWithApi } from '../api/authApi';

export interface User {
    name: string;
    email: string;
    schoolClass: string | null;
}

export interface RegistrationData extends Omit<User, 'schoolClass'> {
    password: string;
}

export interface Credentials {
    email: string;
    password: string;
}

export interface AuthService {
    getCurrentUser(): User | null;
    register(data: RegistrationData): User;
    login(credentials: Credentials): Promise<User>;
    updateSchoolClass(schoolClass: string): User;
    logout(): void;
}

const CURRENT_USER_KEY = 'eduhub.demo.currentUser';
const AUTH_TOKEN_KEY = 'eduhub.auth.token';

class LocalStorageAuthService implements AuthService {
    getCurrentUser(): User | null {
        const storedUser = localStorage.getItem(CURRENT_USER_KEY);
        return storedUser ? this.parseUser(storedUser) : null;
    }

    register(data: RegistrationData): User {
        const user: User = { name: data.name, email: data.email, schoolClass: null };
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(user));
        return user;
    }

    async login(credentials: Credentials): Promise<User> {
        const session = await loginWithApi({
            login: credentials.email.trim(),
            password: credentials.password,
        });
        const user: User = {
            name: [session.user.first_name, session.user.middle_name, session.user.last_name]
                .filter(Boolean)
                .join(' '),
            email: credentials.email.trim(),
            schoolClass: session.user.class_id ?? null,
        };

        localStorage.setItem(AUTH_TOKEN_KEY, session.token);
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(user));
        return user;
    }

    updateSchoolClass(schoolClass: string): User {
        const currentUser = this.getCurrentUser();
        if (!currentUser) {
            throw new Error('Войдите в аккаунт, чтобы изменить профиль.');
        }

        const updatedUser: User = { ...currentUser, schoolClass };
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(updatedUser));
        return updatedUser;
    }

    logout(): void {
        localStorage.removeItem(CURRENT_USER_KEY);
        localStorage.removeItem(AUTH_TOKEN_KEY);
    }

    private parseUser(value: string): User {
        const user: unknown = JSON.parse(value);
        if (
            typeof user !== 'object' ||
            user === null ||
            !('name' in user) ||
            typeof user.name !== 'string' ||
            !('email' in user) ||
            typeof user.email !== 'string' ||
            ('schoolClass' in user &&
                user.schoolClass !== null &&
                typeof user.schoolClass !== 'string')
        ) {
            throw new Error('Не удалось прочитать данные аккаунта.');
        }
        return {
            name: user.name,
            email: user.email,
            schoolClass: 'schoolClass' in user && typeof user.schoolClass === 'string'
                ? user.schoolClass
                : null,
        };
    }
}

// Replace this demo adapter with an API-backed implementation when authentication is added.
export const authService: AuthService = new LocalStorageAuthService();

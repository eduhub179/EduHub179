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
    login(credentials: Credentials): User;
    updateSchoolClass(schoolClass: string): User;
    logout(): void;
}

type StoredAccount = RegistrationData & { schoolClass?: string | null };

const ACCOUNTS_KEY = 'eduhub.demo.accounts';
const CURRENT_USER_KEY = 'eduhub.demo.currentUser';

class LocalStorageAuthService implements AuthService {
    getCurrentUser(): User | null {
        const storedUser = localStorage.getItem(CURRENT_USER_KEY);
        return storedUser ? this.parseUser(storedUser) : null;
    }

    register(data: RegistrationData): User {
        const accounts = this.getAccounts();
        const user: User = { name: data.name, email: data.email, schoolClass: null };
        localStorage.setItem(ACCOUNTS_KEY, JSON.stringify([...accounts, { ...data, schoolClass: null }]));
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(user));
        return user;
    }

    login(credentials: Credentials): User {
        const normalizedEmail = credentials.email.toLowerCase();
        const account = this.getAccounts().find(
            (storedAccount) =>
                storedAccount.email.toLowerCase() === normalizedEmail &&
                storedAccount.password === credentials.password,
        );

        if (!account) {
            throw new Error('Неверная почта или пароль.');
        }

        const user: User = {
            name: account.name,
            email: account.email,
            schoolClass: account.schoolClass ?? null,
        };
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(user));
        return user;
    }

    updateSchoolClass(schoolClass: string): User {
        const currentUser = this.getCurrentUser();
        if (!currentUser) {
            throw new Error('Войдите в аккаунт, чтобы изменить профиль.');
        }

        const accounts = this.getAccounts();
        const accountIndex = accounts.findIndex(
            (account) => account.email.toLowerCase() === currentUser.email.toLowerCase(),
        );
        if (accountIndex < 0) {
            throw new Error('Не удалось найти аккаунт для обновления.');
        }

        accounts[accountIndex] = { ...accounts[accountIndex], schoolClass };
        const updatedUser: User = { ...currentUser, schoolClass };
        localStorage.setItem(ACCOUNTS_KEY, JSON.stringify(accounts));
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(updatedUser));
        return updatedUser;
    }

    logout(): void {
        localStorage.removeItem(CURRENT_USER_KEY);
    }

    private getAccounts(): StoredAccount[] {
        const storedAccounts = localStorage.getItem(ACCOUNTS_KEY);
        if (!storedAccounts) {
            return [];
        }

        const accounts: unknown = JSON.parse(storedAccounts);
        if (!Array.isArray(accounts) || !accounts.every(this.isStoredAccount)) {
            throw new Error('Не удалось прочитать данные аккаунтов.');
        }
        return accounts;
    }

    private isStoredAccount(value: unknown): value is StoredAccount {
        return (
            typeof value === 'object' &&
            value !== null &&
            'name' in value &&
            typeof value.name === 'string' &&
            'email' in value &&
            typeof value.email === 'string' &&
            'password' in value &&
            typeof value.password === 'string' &&
            (!('schoolClass' in value) ||
                value.schoolClass === null ||
                typeof value.schoolClass === 'string')
        );
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

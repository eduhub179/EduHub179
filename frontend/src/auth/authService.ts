export interface User {
    name: string;
    email: string;
}

export interface RegistrationData extends User {
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
    logout(): void;
}

type StoredAccount = RegistrationData;

const ACCOUNTS_KEY = 'eduhub.demo.accounts';
const CURRENT_USER_KEY = 'eduhub.demo.currentUser';

class LocalStorageAuthService implements AuthService {
    getCurrentUser(): User | null {
        const storedUser = localStorage.getItem(CURRENT_USER_KEY);
        return storedUser ? this.parseUser(storedUser) : null;
    }

    register(data: RegistrationData): User {
        const accounts = this.getAccounts();
        const user = { name: data.name, email: data.email };
        localStorage.setItem(ACCOUNTS_KEY, JSON.stringify([...accounts, data]));
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

        const user = { name: account.name, email: account.email };
        localStorage.setItem(CURRENT_USER_KEY, JSON.stringify(user));
        return user;
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
            typeof value.password === 'string'
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
            typeof user.email !== 'string'
        ) {
            throw new Error('Не удалось прочитать данные аккаунта.');
        }
        return { name: user.name, email: user.email };
    }
}

// Replace this demo adapter with an API-backed implementation when authentication is added.
export const authService: AuthService = new LocalStorageAuthService();

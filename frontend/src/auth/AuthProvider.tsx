import React, { createContext, useContext, useState } from 'react';
import type { ReactNode } from 'react';
import { authService } from './authService';
import type { Credentials, RegistrationData, User } from './authService';

interface AuthContextValue {
    user: User | null;
    register: (data: RegistrationData) => void;
    login: (credentials: Credentials) => Promise<void>;
    updateSchoolClass: (schoolClass: string) => Promise<void>;
    logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

interface AuthProviderProps {
    children: ReactNode;
}

export const AuthProvider: React.FC<AuthProviderProps> = ({ children }) => {
    const [user, setUser] = useState<User | null>(() => authService.getCurrentUser());

    const register = (data: RegistrationData) => setUser(authService.register(data));
    const login = async (credentials: Credentials) => setUser(await authService.login(credentials));
    const updateSchoolClass = async (schoolClass: string) =>
        setUser(await authService.updateSchoolClass(schoolClass));
    const logout = async () => {
        await authService.logout();
        setUser(null);
    };

    return (
        <AuthContext.Provider value={{ user, register, login, updateSchoolClass, logout }}>
            {children}
        </AuthContext.Provider>
    );
};

export const useAuth = (): AuthContextValue => {
    const context = useContext(AuthContext);
    if (!context) {
        throw new Error('useAuth must be used within an AuthProvider.');
    }
    return context;
};

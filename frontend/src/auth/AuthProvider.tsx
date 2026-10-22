import React, { createContext, useContext, useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { SESSION_EXPIRED_EVENT } from '../api/client';
import { detachPushOnLogout, syncPushOnStartup } from '../notifications/push';
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
    const navigate = useNavigate();

    // The backend rejected our session (expired or revoked): stop pretending to be logged in.
    // Only when a user is stored — a 401 for a wrong password or code on the login and
    // register pages must not navigate anywhere.
    useEffect(() => {
        const handleSessionExpired = () => {
            if (!authService.getCurrentUser()) {
                return;
            }
            authService.forgetCurrentUser();
            setUser(null);
            navigate('/login', { replace: true, state: { sessionExpired: true } });
        };
        window.addEventListener(SESSION_EXPIRED_EVENT, handleSessionExpired);
        // Keeps the service worker fresh and re-sends an existing push subscription.
        if (authService.getCurrentUser()) {
            void syncPushOnStartup().catch(() => undefined);
        }
        return () => window.removeEventListener(SESSION_EXPIRED_EVENT, handleSessionExpired);
    }, [navigate]);

    const register = (data: RegistrationData) => setUser(authService.register(data));
    const login = async (credentials: Credentials) => {
        setUser(await authService.login(credentials));
        // This browser already has notifications on: link them to the user who just logged in.
        void syncPushOnStartup().catch(() => undefined);
    };
    const updateSchoolClass = async (schoolClass: string) =>
        setUser(await authService.updateSchoolClass(schoolClass));
    const logout = async () => {
        // Unlink this browser from the user (the subscription itself stays for the next login):
        // the next person on a shared computer must not get this user's homework.
        await detachPushOnLogout();
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

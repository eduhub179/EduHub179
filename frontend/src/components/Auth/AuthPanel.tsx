import React from 'react';
import { Link } from 'react-router-dom';

type AuthMode = 'login' | 'register';

interface AuthPanelProps {
    mode: AuthMode;
    titleId: string;
    children: React.ReactNode;
}

const AuthPanel: React.FC<AuthPanelProps> = ({ mode, titleId, children }) => (
    <section className="auth-panel" aria-labelledby={titleId}>
        <div className="auth-panel__brand" aria-label="EduHub">
            <span className="auth-panel__brand-mark">E</span>
            <span>EduHub</span>
        </div>

        <nav className="auth-tabs" aria-label="Авторизация">
            <Link
                aria-current={mode === 'login' ? 'page' : undefined}
                className={`auth-tabs__tab${mode === 'login' ? ' auth-tabs__tab--active' : ''}`}
                to="/login"
            >
                Вход
            </Link>
            <Link
                aria-current={mode === 'register' ? 'page' : undefined}
                className={`auth-tabs__tab${mode === 'register' ? ' auth-tabs__tab--active' : ''}`}
                to="/register"
            >
                Регистрация
            </Link>
        </nav>

        <div className="auth-panel__content">{children}</div>
    </section>
);

export default AuthPanel;

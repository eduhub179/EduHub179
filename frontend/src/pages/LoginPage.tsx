import React, { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import type { Credentials } from '../auth/authService';
import Header from '../components/Header';
import AuthPanel from '../components/Auth/AuthPanel';
import LoginForm from '../components/Login/LoginForm';
import '../css/index.css';
import '../css/RegisterPage.css';
import '../css/AuthPage.css';

const LoginPage: React.FC = () => {
    const { login } = useAuth();
    const navigate = useNavigate();
    const [error, setError] = useState('');

    const handleLogin = (credentials: Credentials) => {
        setError('');
        login(credentials);
        navigate('/main');
    };

    return (
        <>
            <Header />
            <main className="auth-page">
                <AuthPanel mode="login" titleId="login-title">
                    <h1 id="login-title">Войти</h1>
                    <p className="register-card__description">
                        Войдите в свой аккаунт EduHub.
                    </p>
                    {error && <p className="register-card__error" role="alert">{error}</p>}
                    <LoginForm onError={setError} onLogin={handleLogin} />
                    <p className="register-card__signin">
                        Нет аккаунта? <Link to="/register">Зарегистрироваться</Link>
                    </p>
                </AuthPanel>
            </main>
        </>
    );
};

export default LoginPage;

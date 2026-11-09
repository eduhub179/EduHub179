import React from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import type { Credentials, FirstPassword } from '../auth/authService';
import Header from '../components/Header';
import AuthPanel from '../components/Auth/AuthPanel';
import LoginForm from '../components/Login/LoginForm';
import '../css/index.css';
import '../css/RegisterPage.css';
import '../css/AuthPage.css';

const LoginPage: React.FC = () => {
    const { login, completeFirstLogin } = useAuth();
    const navigate = useNavigate();
    const location = useLocation();
    const sessionExpired = (location.state as { sessionExpired?: boolean } | null)?.sessionExpired === true;

    const handleLogin = async (credentials: Credentials) => {
        await login(credentials);
        navigate('/main');
    };

    const handleFirstLogin = async (data: FirstPassword) => {
        await completeFirstLogin(data);
        navigate('/main');
    };

    return (
        <>
            <Header />
            <main className="auth-page">
                <AuthPanel mode="login" titleId="login-title">
                    <h1 id="login-title">Вход</h1>
                    <p className="register-card__description">
                        Войдите по школьной почте. В первый раз мы пришлём на неё код,
                        и вы зададите пароль.
                    </p>
                    {sessionExpired && (
                        <p className="register-card__error" role="status">
                            Сессия истекла. Войдите снова.
                        </p>
                    )}
                    <LoginForm onFirstLogin={handleFirstLogin} onLogin={handleLogin} />
                    <p className="register-card__signin">
                        Первый раз? <Link to="/register">Задать пароль</Link>
                    </p>
                </AuthPanel>
            </main>
        </>
    );
};

export default LoginPage;

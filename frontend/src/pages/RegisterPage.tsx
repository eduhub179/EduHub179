import React, { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import type { FirstPassword } from '../auth/authService';
import Header from '../components/Header';
import AuthPanel from '../components/Auth/AuthPanel';
import RegisterForm from '../components/Register/RegisterForm';
import '../css/index.css';
import '../css/RegisterPage.css';
import '../css/AuthPage.css';

const RegisterPage: React.FC = () => {
    const { completeFirstLogin } = useAuth();
    const navigate = useNavigate();
    const [error, setError] = useState('');

    const handleRegister = async (data: FirstPassword) => {
        setError('');
        await completeFirstLogin(data);
        navigate('/main');
    };

    return (
        <>
            <Header />
            <main className="auth-page">
                <AuthPanel mode="register" titleId="register-title">
                    <h1 id="register-title">Первый вход</h1>
                    <p className="register-card__description">
                        Аккаунт уже создан школой. Подтвердите школьную почту кодом
                        и задайте пароль — дальше входите с ним.
                    </p>
                    {error && <p className="register-card__error" role="alert">{error}</p>}
                    <RegisterForm onError={setError} onRegister={handleRegister} />
                    <p className="register-card__signin">
                        Уже есть аккаунт? <Link to="/login">Войти</Link>
                    </p>
                </AuthPanel>
            </main>
        </>
    );
};

export default RegisterPage;

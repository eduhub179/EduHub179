import React, { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import type { RegistrationData } from '../auth/authService';
import Header from '../components/Header';
import AuthPanel from '../components/Auth/AuthPanel';
import RegisterForm from '../components/Register/RegisterForm';
import '../css/index.css';
import '../css/RegisterPage.css';
import '../css/AuthPage.css';

const RegisterPage: React.FC = () => {
    const { register } = useAuth();
    const navigate = useNavigate();
    const [error, setError] = useState('');

    const handleRegister = (data: RegistrationData) => {
        setError('');
        register(data);
        navigate('/main');
    };

    return (
        <>
            <Header />
            <main className="auth-page">
                <AuthPanel mode="register" titleId="register-title">
                    <h1 id="register-title">Создать аккаунт</h1>
                    <p className="register-card__description">
                        Зарегистрируйтесь, чтобы продолжить работу с EduHub.
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

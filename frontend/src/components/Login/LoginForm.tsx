import React from 'react';
import type { FormEvent } from 'react';
import type { Credentials } from '../../auth/authService';

interface LoginFormProps {
    onLogin: (credentials: Credentials) => void;
    onError: (message: string) => void;
}

const LoginForm: React.FC<LoginFormProps> = ({ onLogin, onError }) => {
    const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const formData = new FormData(event.currentTarget);

        try {
            onLogin({
                email: String(formData.get('email')),
                password: String(formData.get('password')),
            });
        } catch (error) {
            onError(error instanceof Error ? error.message : 'Не удалось войти в аккаунт.');
        }
    };

    return (
        <form className="register-form" onSubmit={handleSubmit}>
            <label className="register-form__field">
                <span>Электронная почта</span>
                <input
                    autoComplete="email"
                    name="email"
                    placeholder="name@example.com"
                    required
                    type="text"
                />
            </label>

            <label className="register-form__field">
                <span>Пароль</span>
                <input
                    autoComplete="current-password"
                    name="password"
                    placeholder="Ваш пароль"
                    required
                    type="password"
                />
            </label>

            <button className="register-form__submit" type="submit">
                Войти
            </button>
        </form>
    );
};

export default LoginForm;

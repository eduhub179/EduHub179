import React, { useState } from 'react';
import type { FormEvent } from 'react';
import type { Credentials } from '../../auth/authService';
import {errorText} from '../../api/errors';

/**
 * Sign-in with school email and password in one step.
 * The first login (no password yet: emailed code + new password) is on the "Регистрация" tab.
 */
interface LoginFormProps {
    onLogin: (credentials: Credentials) => Promise<void>;
}

const LoginForm: React.FC<LoginFormProps> = ({ onLogin }) => {
    const [error, setError] = useState('');
    const [busy, setBusy] = useState(false);
    const [showPassword, setShowPassword] = useState(false);

    const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const form = new FormData(event.currentTarget);
        setBusy(true);
        setError('');
        try {
            await onLogin({
                email: String(form.get('email') ?? '').trim(),
                password: String(form.get('password') ?? ''),
                rememberMe: true,
            });
        } catch (failure) {
            setError(errorText(failure, 'Не удалось войти.'));
        } finally {
            setBusy(false);
        }
    };

    return (
        <form className="register-form" onSubmit={handleSubmit}>
            <label className="register-form__field">
                <span>Школьная почта</span>
                <input
                    autoComplete="username"
                    autoFocus
                    inputMode="email"
                    name="email"
                    placeholder="name@179.ru"
                    required
                    type="email"
                />
            </label>

            <label className="register-form__field">
                <span>Пароль</span>
                <div className="register-form__password-control">
                    <input
                        autoComplete="current-password"
                        name="password"
                        placeholder="Ваш пароль"
                        required
                        type={showPassword ? 'text' : 'password'}
                    />
                    <button
                        aria-label={showPassword ? 'Скрыть пароль' : 'Показать пароль'}
                        aria-pressed={showPassword}
                        className="register-form__password-toggle"
                        onClick={() => setShowPassword((visible) => !visible)}
                        type="button"
                    >
                        <svg aria-hidden="true" viewBox="0 0 24 24">
                            {showPassword ? (
                                <>
                                    <path d="M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8"/>
                                    <path d="M9.9 5.2A10.8 10.8 0 0 1 12 5c6.4 0 10 7 10 7a15 15 0 0 1-3.1 3.8M6.2 6.2C3.5 8 2 12 2 12s3.6 7 10 7c1.3 0 2.5-.3 3.5-.8"/>
                                </>
                            ) : (
                                <>
                                    <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Z"/>
                                    <circle cx="12" cy="12" r="3"/>
                                </>
                            )}
                        </svg>
                    </button>
                </div>
            </label>

            {error && <p className="register-card__error" role="alert">{error}</p>}

            <button className="register-form__submit" disabled={busy} type="submit">
                {busy ? 'Входим…' : 'Войти'}
            </button>
        </form>
    );
};

export default LoginForm;

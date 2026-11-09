import React, { useState } from 'react';
import type { FormEvent } from 'react';
import { resendLoginCode, startEmailLogin, verifyLoginCode } from '../../api/authApi';
import type { Credentials, FirstPassword } from '../../auth/authService';

/**
 * Sign-in by school email:
 *  email → password (the account has one)
 *  email → code from the email → new password (first login; the password is mandatory)
 */
type Step =
    | { kind: 'email' }
    | { kind: 'password'; email: string }
    | { kind: 'code'; email: string; codeSent: boolean }
    | { kind: 'new-password'; email: string; setupToken: string };

interface LoginFormProps {
    onLogin: (credentials: Credentials) => Promise<void>;
    onFirstLogin: (data: FirstPassword) => Promise<void>;
}

const MIN_PASSWORD = 8;
const MAX_PASSWORD = 128;

const message = (error: unknown, fallback: string) =>
    error instanceof Error && error.message ? error.message : fallback;

const LoginForm: React.FC<LoginFormProps> = ({ onLogin, onFirstLogin }) => {
    const [step, setStep] = useState<Step>({ kind: 'email' });
    const [error, setError] = useState('');
    const [notice, setNotice] = useState('');
    const [busy, setBusy] = useState(false);
    const [rememberMe, setRememberMe] = useState(true);

    /** Runs a request with the form locked; shows its error. */
    const run = async (action: () => Promise<void>, fallback: string) => {
        setBusy(true);
        setError('');
        try {
            await action();
        } catch (failure) {
            setError(message(failure, fallback));
        } finally {
            setBusy(false);
        }
    };

    const backToEmail = () => {
        setStep({ kind: 'email' });
        setError('');
        setNotice('');
    };

    const handleEmail = (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const email = String(new FormData(event.currentTarget).get('email') ?? '').trim();
        void run(async () => {
            const started = await startEmailLogin(email);
            setNotice('');
            setStep(started.method === 'password'
                ? { kind: 'password', email: started.email }
                : { kind: 'code', email: started.email, codeSent: started.code_sent });
        }, 'Не удалось проверить почту.');
    };

    const handlePassword = (event: FormEvent<HTMLFormElement>, email: string) => {
        event.preventDefault();
        const password = String(new FormData(event.currentTarget).get('password') ?? '');
        void run(() => onLogin({ email, password, rememberMe }), 'Не удалось войти.');
    };

    const handleCode = (event: FormEvent<HTMLFormElement>, email: string) => {
        event.preventDefault();
        const code = String(new FormData(event.currentTarget).get('code') ?? '').trim();
        void run(async () => {
            const { setup_token } = await verifyLoginCode(email, code);
            setNotice('');
            setStep({ kind: 'new-password', email, setupToken: setup_token });
        }, 'Не удалось проверить код.');
    };

    const handleResend = (email: string) => {
        void run(async () => {
            await resendLoginCode(email);
            setNotice('Новый код отправлен.');
        }, 'Не удалось отправить код.');
    };

    const handleNewPassword = (event: FormEvent<HTMLFormElement>, email: string, setupToken: string) => {
        event.preventDefault();
        const form = new FormData(event.currentTarget);
        const password = String(form.get('password') ?? '');
        const repeat = String(form.get('passwordRepeat') ?? '');
        // Characters, as the server counts them (not UTF-16 units).
        const length = [...password].length;
        if (length < MIN_PASSWORD || length > MAX_PASSWORD) {
            setError(`Пароль должен быть от ${MIN_PASSWORD} до ${MAX_PASSWORD} символов.`);
            return;
        }
        if (password !== repeat) {
            setError('Пароли не совпадают.');
            return;
        }
        void run(
            () => onFirstLogin({ email, setupToken, password, rememberMe }),
            'Не удалось сохранить пароль.',
        );
    };

    const feedback = (
        <>
            {error && <p className="register-card__error" role="alert">{error}</p>}
            {notice && !error && <p className="auth-hint" role="status">{notice}</p>}
        </>
    );

    const rememberBox = (
        <label className="auth-remember">
            <input
                checked={rememberMe}
                onChange={(event) => setRememberMe(event.target.checked)}
                type="checkbox"
            />
            <span>Запомнить меня на этом устройстве</span>
        </label>
    );

    const emailLine = (email: string) => (
        <p className="auth-hint">
            {email}{' '}
            <button className="auth-link-button" disabled={busy} onClick={backToEmail} type="button">
                Изменить
            </button>
        </p>
    );

    switch (step.kind) {
        case 'email':
            return (
                <form className="register-form" onSubmit={handleEmail}>
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
                    {feedback}
                    <button className="register-form__submit" disabled={busy} type="submit">
                        {busy ? 'Проверяем…' : 'Продолжить'}
                    </button>
                </form>
            );

        case 'password':
            return (
                <form className="register-form" onSubmit={(event) => handlePassword(event, step.email)}>
                    {emailLine(step.email)}
                    {/* Lets password managers pair the password with the email. */}
                    <input autoComplete="username" hidden name="email" readOnly style={{ display: 'none' }} value={step.email} />
                    <label className="register-form__field">
                        <span>Пароль</span>
                        <input
                            autoComplete="current-password"
                            autoFocus
                            name="password"
                            placeholder="Ваш пароль"
                            required
                            type="password"
                        />
                    </label>
                    {rememberBox}
                    {feedback}
                    <button className="register-form__submit" disabled={busy} type="submit">
                        {busy ? 'Входим…' : 'Войти'}
                    </button>
                </form>
            );

        case 'code':
            return (
                <form className="register-form" onSubmit={(event) => handleCode(event, step.email)}>
                    {emailLine(step.email)}
                    <p className="register-card__description">
                        {step.codeSent
                            ? 'Это первый вход. Мы отправили на почту код из 6 цифр.'
                            : 'Это первый вход. Код уже отправлен на почту меньше минуты назад — введите его.'}
                    </p>
                    <label className="register-form__field">
                        <span>Код из письма</span>
                        <input
                            autoComplete="one-time-code"
                            autoFocus
                            inputMode="numeric"
                            maxLength={6}
                            name="code"
                            pattern="[0-9]{6}"
                            placeholder="123456"
                            required
                            type="text"
                        />
                    </label>
                    {feedback}
                    <div className="register-form__actions">
                        <button
                            className="register-form__back"
                            disabled={busy}
                            onClick={() => handleResend(step.email)}
                            type="button"
                        >
                            Отправить ещё раз
                        </button>
                        <button className="register-form__submit" disabled={busy} type="submit">
                            {busy ? 'Проверяем…' : 'Дальше'}
                        </button>
                    </div>
                </form>
            );

        case 'new-password':
            return (
                <form
                    className="register-form"
                    onSubmit={(event) => handleNewPassword(event, step.email, step.setupToken)}
                >
                    {emailLine(step.email)}
                    <p className="register-card__description">
                        Почта подтверждена. Придумайте пароль — дальше вы будете входить с ним.
                    </p>
                    <input autoComplete="username" hidden name="email" readOnly style={{ display: 'none' }} value={step.email} />
                    <label className="register-form__field">
                        <span>Новый пароль</span>
                        <input
                            autoComplete="new-password"
                            autoFocus
                            maxLength={MAX_PASSWORD}
                            minLength={MIN_PASSWORD}
                            name="password"
                            placeholder={`Не меньше ${MIN_PASSWORD} символов`}
                            required
                            type="password"
                        />
                    </label>
                    <label className="register-form__field">
                        <span>Повторите пароль</span>
                        <input
                            autoComplete="new-password"
                            maxLength={MAX_PASSWORD}
                            minLength={MIN_PASSWORD}
                            name="passwordRepeat"
                            required
                            type="password"
                        />
                    </label>
                    {rememberBox}
                    {feedback}
                    <button className="register-form__submit" disabled={busy} type="submit">
                        {busy ? 'Сохраняем…' : 'Сохранить и войти'}
                    </button>
                </form>
            );
    }
};

export default LoginForm;

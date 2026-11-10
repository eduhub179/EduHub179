import React, { useRef, useState } from 'react';
import type { FormEvent } from 'react';
import { resendLoginCode, startEmailLogin, verifyLoginCode } from '../../api/authApi';
import type { FirstPassword } from '../../auth/authService';
import {errorText} from '../../api/errors';

/**
 * First login ("registration"): accounts are created by the school, the person confirms
 * the school email with a code and chooses the password, then is logged in.
 *
 * Server steps: /api/auth/email/start (sends the code) → /email/code/verify (code → one-time
 * token) → `onRegister` = /email/password (saves the password, starts the session).
 */
interface RegisterFormProps {
    onRegister: (data: FirstPassword) => Promise<void>;
    onError: (message: string) => void;
}

const MIN_PASSWORD = 8;
const MAX_PASSWORD = 128;

const RegisterForm: React.FC<RegisterFormProps> = ({ onRegister, onError }) => {
    const emailInputRef = useRef<HTMLInputElement>(null);
    const [email, setEmail] = useState('');
    const [isSendingCode, setIsSendingCode] = useState(false);
    const [codeSentMessage, setCodeSentMessage] = useState('');
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [showPassword, setShowPassword] = useState(false);
    const [showPasswordConfirmation, setShowPasswordConfirmation] = useState(false);
    // The code is single-use: once it is accepted, keep its token, so a failed save
    // (network, server) can be retried without a new code.
    const [setupToken, setSetupToken] = useState<{ email: string; token: string } | null>(null);

    const handleSendCode = async () => {
        if (!emailInputRef.current?.reportValidity()) {
            return;
        }

        setIsSendingCode(true);
        onError('');
        try {
            const address = email.trim();
            if (codeSentMessage) {
                await resendLoginCode(address);
                setCodeSentMessage('Новый код отправлен на почту.');
            } else {
                const started = await startEmailLogin(address);
                if (started.method === 'password') {
                    onError('У этого аккаунта уже есть пароль. Войдите на вкладке «Вход».');
                    return;
                }
                setCodeSentMessage(started.code_sent
                    ? 'Код отправлен на почту.'
                    : 'Код уже отправлен меньше минуты назад — проверьте почту.');
            }
            setSetupToken(null);
        } catch (error) {
            onError(errorText(error, 'Не удалось отправить код.'));
        } finally {
            setIsSendingCode(false);
        }
    };

    const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const formData = new FormData(event.currentTarget);
        const normalizedEmail = String(formData.get('email') ?? '').trim();
        const password = String(formData.get('password') ?? '');

        // Characters, as the server counts them (not UTF-16 units).
        const length = [...password].length;
        if (length < MIN_PASSWORD || length > MAX_PASSWORD) {
            onError(`Пароль должен быть от ${MIN_PASSWORD} до ${MAX_PASSWORD} символов.`);
            return;
        }
        if (password !== String(formData.get('passwordConfirmation') ?? '')) {
            onError('Пароли не совпадают.');
            return;
        }

        setIsSubmitting(true);
        onError('');
        try {
            let token = setupToken?.email === normalizedEmail ? setupToken.token : null;
            if (!token) {
                const verified = await verifyLoginCode(
                    normalizedEmail,
                    String(formData.get('verificationCode') ?? '').trim(),
                );
                token = verified.setup_token;
                setSetupToken({ email: normalizedEmail, token });
            }

            await onRegister({
                email: normalizedEmail,
                setupToken: token,
                password,
                rememberMe: true,
            });
        } catch (error) {
            onError(errorText(error, 'Не удалось задать пароль.'));
        } finally {
            setIsSubmitting(false);
        }
    };

    return (
        <form className="register-form" onSubmit={handleSubmit}>
            <label className="register-form__field">
                <span>Электронная почта</span>
                <input
                    autoComplete="email"
                    ref={emailInputRef}
                    name="email"
                    placeholder="name@179.ru"
                    required
                    type="email"
                    value={email}
                    onChange={(event) => {
                        setEmail(event.target.value);
                        setCodeSentMessage('');
                        setSetupToken(null);
                    }}
                />
            </label>

            <label className="register-form__field">
                <span>Пароль</span>
                <div className="register-form__password-control">
                    <input
                        autoComplete="new-password"
                        maxLength={MAX_PASSWORD}
                        minLength={MIN_PASSWORD}
                        name="password"
                        placeholder={`Не меньше ${MIN_PASSWORD} символов`}
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

            <label className="register-form__field">
                <span>Повторите пароль</span>
                <div className="register-form__password-control">
                    <input
                        autoComplete="new-password"
                        name="passwordConfirmation"
                        placeholder="Введите пароль ещё раз"
                        required
                        type={showPasswordConfirmation ? 'text' : 'password'}
                    />
                    <button
                        aria-label={showPasswordConfirmation ? 'Скрыть пароль' : 'Показать пароль'}
                        aria-pressed={showPasswordConfirmation}
                        className="register-form__password-toggle"
                        onClick={() => setShowPasswordConfirmation((visible) => !visible)}
                        type="button"
                    >
                        <svg aria-hidden="true" viewBox="0 0 24 24">
                            {showPasswordConfirmation ? (
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

            <div className="register-form__code-row">
                <label className="register-form__field">
                    <span>Код подтверждения</span>
                    <input
                        autoComplete="one-time-code"
                        inputMode="numeric"
                        name="verificationCode"
                        placeholder="Введите код"
                        required
                        type="text"
                    />
                </label>
                <button
                    className="register-form__send-code"
                    onClick={handleSendCode}
                    disabled={isSendingCode}
                    type="button"
                >
                    {isSendingCode ? 'Отправка…' : codeSentMessage ? 'Отправить повторно' : 'Отправить код'}
                </button>
            </div>
            {codeSentMessage && (
                <p className="register-card__code-hint" role="status">
                    {codeSentMessage}
                </p>
            )}
            <button className="register-form__submit" disabled={isSubmitting} type="submit">
                {isSubmitting ? 'Проверка…' : 'Задать пароль и войти'}
            </button>
        </form>
    );
};

export default RegisterForm;

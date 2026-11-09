import React, { useRef, useState } from 'react';
import type { FormEvent } from 'react';
import { sendAuthCode, verifyAuthCode } from '../../api/authApi';
import type { RegistrationData } from '../../auth/authService';

interface RegisterFormProps {
    onRegister: (data: RegistrationData) => void;
    onError: (message: string) => void;
}

const RegisterForm: React.FC<RegisterFormProps> = ({ onRegister, onError }) => {
    const emailInputRef = useRef<HTMLInputElement>(null);
    const [email, setEmail] = useState('');
    const [isSendingCode, setIsSendingCode] = useState(false);
    const [codeSentMessage, setCodeSentMessage] = useState('');
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [showPassword, setShowPassword] = useState(false);
    const [showPasswordConfirmation, setShowPasswordConfirmation] = useState(false);

    const handleSendCode = async () => {
        if (!emailInputRef.current?.reportValidity()) {
            return;
        }

        setIsSendingCode(true);
        setCodeSentMessage('');
        onError('');
        try {
            const response = await sendAuthCode({ email: email.trim() });
            setCodeSentMessage(response.message);
        } catch (error) {
            onError(error instanceof Error ? error.message : 'Не удалось отправить код.');
        } finally {
            setIsSendingCode(false);
        }
    };

    const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const formData = new FormData(event.currentTarget);
        const normalizedEmail = String(formData.get('email') ?? '').trim();
        const password = String(formData.get('password') ?? '');

        if (password !== String(formData.get('passwordConfirmation') ?? '')) {
            onError('Пароли не совпадают.');
            return;
        }

        setIsSubmitting(true);
        onError('');
        try {
            const verification = await verifyAuthCode({
                email: normalizedEmail,
                code: String(formData.get('verificationCode') ?? '').trim(),
            });
            if (!verification.valid) {
                onError('Неверный код подтверждения.');
                return;
            }

            onRegister({
                name: normalizedEmail,
                email: normalizedEmail,
                password,
            });
        } catch (error) {
            onError(error instanceof Error ? error.message : 'Не удалось создать аккаунт.');
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
                    placeholder="name@example.com"
                    required
                    type="email"
                    value={email}
                    onChange={(event) => {
                        setEmail(event.target.value);
                        setCodeSentMessage('');
                    }}
                />
            </label>

            <label className="register-form__field">
                <span>Пароль</span>
                <div className="register-form__password-control">
                    <input
                        autoComplete="new-password"
                        name="password"
                        placeholder="Введите пароль"
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
                {isSubmitting ? 'Проверка…' : 'Создать аккаунт'}
            </button>
        </form>
    );
};

export default RegisterForm;

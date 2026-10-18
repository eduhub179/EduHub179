import React, { useState } from 'react';
import type { FormEvent } from 'react';
import type { RegistrationData } from '../../auth/authService';

interface RegisterFormProps {
    onRegister: (data: RegistrationData) => void;
    onError: (message: string) => void;
}

const RegisterForm: React.FC<RegisterFormProps> = ({ onRegister, onError }) => {
    const [codeSendCount, setCodeSendCount] = useState(0);

    const handleSendCode = () => {
        setCodeSendCount((count) => count + 1);
    };

    const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const formData = new FormData(event.currentTarget);
        const email = String(formData.get('email') ?? '');
        const name = String(formData.get('name') ?? '').trim();

        try {
            onRegister({
                name,
                email,
                password: String(formData.get('password') ?? ''),
            });
        } catch (error) {
            onError(error instanceof Error ? error.message : 'Не удалось создать аккаунт.');
        }
    };

    return (
        <form className="register-form" onSubmit={handleSubmit}>
            <label className="register-form__field">
                <span>Никнейм</span>
                <input
                    autoComplete="nickname"
                    name="name"
                    placeholder="Например, admin"
                    required
                    type="text"
                />
            </label>

            <label className="register-form__field">
                <span>Электронная почта</span>
                <input
                    autoComplete="email"
                    name="email"
                    placeholder="name@example.com"
                    type="text"
                />
            </label>

            <label className="register-form__field">
                <span>Пароль</span>
                <input
                    autoComplete="new-password"
                    name="password"
                    placeholder="Введите пароль"
                    type="password"
                />
            </label>

            <label className="register-form__field">
                <span>Повторите пароль</span>
                <input
                    autoComplete="new-password"
                    name="passwordConfirmation"
                    placeholder="Введите пароль ещё раз"
                    type="password"
                />
            </label>

            <div className="register-form__code-row">
                <label className="register-form__field">
                    <span>Код подтверждения</span>
                    <input
                        autoComplete="one-time-code"
                        inputMode="numeric"
                        name="verificationCode"
                        placeholder="Введите код"
                        type="text"
                    />
                </label>
                <button
                    className="register-form__send-code"
                    onClick={handleSendCode}
                    type="button"
                >
                    {codeSendCount > 0 ? 'Отправить повторно' : 'Отправить код'}
                </button>
            </div>
            {codeSendCount > 0 && (
                <p className="register-card__code-hint" role="status">
                    Код отправлен. Его можно отправить повторно.
                </p>
            )}
            <button className="register-form__submit" type="submit">
                Создать аккаунт
            </button>
        </form>
    );
};

export default RegisterForm;

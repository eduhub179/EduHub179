import React, { useState } from 'react';
import type { FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import Header from '../components/Header';
import '../css/index.css';
import '../css/RegisterPage.css';

const ProfilePage: React.FC = () => {
    const { user, logout, updateSchoolClass } = useAuth();
    const navigate = useNavigate();
    const [schoolClass, setSchoolClass] = useState(user?.schoolClass ?? '');
    const [error, setError] = useState('');
    const [saved, setSaved] = useState(false);

    const handleLogout = () => {
        logout();
        navigate('/main');
    };

    const handleSave = (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const value = schoolClass.trim();
        if (!value) {
            setError('Укажите свой класс.');
            setSaved(false);
            return;
        }

        try {
            updateSchoolClass(value);
            setSchoolClass(value);
            setError('');
            setSaved(true);
        } catch (saveError) {
            setError(saveError instanceof Error ? saveError.message : 'Не удалось сохранить класс.');
            setSaved(false);
        }
    };

    return (
        <>
            <Header />
            <main className="register-page">
                <section className="register-card" aria-labelledby="profile-title">
                    <h1 id="profile-title">Профиль</h1>
                    {user ? (
                        <>
                            <p className="register-card__description">
                                {user.name}<br />{user.email}
                            </p>
                            <form className="register-form profile-form" onSubmit={handleSave}>
                                <label className="register-form__field">
                                    <span>Класс</span>
                                    <input
                                        autoComplete="off"
                                        name="schoolClass"
                                        onChange={(event) => {
                                            setSchoolClass(event.target.value);
                                            setSaved(false);
                                        }}
                                        placeholder="Например, 9А"
                                        value={schoolClass}
                                    />
                                </label>
                                {error && <p className="register-card__error" role="alert">{error}</p>}
                                {saved && <p className="profile-form__success" role="status">Класс сохранён.</p>}
                                <button className="register-form__submit" type="submit">
                                    Сохранить класс
                                </button>
                            </form>
                            <button className="register-form__submit" onClick={handleLogout} type="button">
                                Выйти
                            </button>
                        </>
                    ) : (
                        <p className="register-card__description">
                            Вы не вошли в аккаунт.
                        </p>
                    )}
                </section>
            </main>
        </>
    );
};

export default ProfilePage;

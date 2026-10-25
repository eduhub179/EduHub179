import React, {useState} from 'react';
import type {FormEvent} from 'react';
import {useEffect} from 'react';
import {useNavigate} from 'react-router-dom';
import {fetchSchoolClasses, getSchoolClassLabel} from '../api';
import type {SchoolClassDto} from '../api';
import {useAuth} from '../auth/AuthProvider';
import Header from '../components/Header';
import NotificationsSettings from '../components/NotificationsSettings';
import '../css/index.css';
import '../css/RegisterPage.css';

const ProfilePage: React.FC = () => {
    const {user, logout, updateSchoolClass} = useAuth();
    const navigate = useNavigate();
    const [classes, setClasses] = useState<SchoolClassDto[]>([]);
    const [schoolClassId, setSchoolClassId] = useState(user?.schoolClass ?? '');
    const [classesLoading, setClassesLoading] = useState(true);
    const [classesError, setClassesError] = useState('');
    const [error, setError] = useState('');
    const [saved, setSaved] = useState(false);

    useEffect(() => {
        let isCurrent = true;

        if (!user) {
            setClassesLoading(false);
            return () => {
                isCurrent = false;
            };
        }

        fetchSchoolClasses()
            .then(({classes: availableClasses}) => {
                if (isCurrent) {
                    setClasses(availableClasses);
                    const currentClass = availableClasses.find((schoolClass) =>
                        schoolClass.id === user?.schoolClass ||
                        schoolClass.name.trim().toLocaleLowerCase('ru-RU') ===
                        user?.schoolClass?.trim().toLocaleLowerCase('ru-RU'),
                    );
                    setSchoolClassId(currentClass?.id ?? '');
                    setClassesLoading(false);
                }
            })
            .catch((loadError: unknown) => {
                if (isCurrent) {
                    setClassesError(loadError instanceof Error ? loadError.message : 'Не удалось загрузить список классов.');
                    setClassesLoading(false);
                }
            });

        return () => {
            isCurrent = false;
        };
    }, [user?.schoolClass]);

    const handleLogout = async () => {
        await logout();
        navigate('/main');
    };

    const handleSave = async (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        const value = schoolClassId.trim();
        if (!value) {
            setError('Выберите свой класс.');
            setSaved(false);
            return;
        }

        try {
            await updateSchoolClass(value);
            setError('');
            setSaved(true);
        } catch (saveError) {
            setError(saveError instanceof Error ? saveError.message : 'Не удалось сохранить класс.');
            setSaved(false);
        }
    };

    return (
        <>
            <Header/>
            <main className="register-page">
                <section className="register-card" aria-labelledby="profile-title">
                    <h1 id="profile-title">Профиль</h1>
                    {user ? (
                        <>
                            <p className="register-card__description">
                                {user.name}<br/>{user.email}
                            </p>
                            <form className="register-form profile-form" onSubmit={handleSave}>
                                <label className="register-form__field">
                                    <span>Класс</span>
                                    <select
                                        name="schoolClass"
                                        onChange={(event) => {
                                            setSchoolClassId(event.target.value);
                                            setSaved(false);
                                        }}
                                        value={schoolClassId}
                                        disabled={classesLoading || classes.length === 0}
                                    >
                                        <option value="">Выберите класс</option>
                                        {classes.map((schoolClass) => (
                                            <option key={schoolClass.id} value={schoolClass.id}>
                                                {getSchoolClassLabel(schoolClass)}
                                            </option>
                                        ))}
                                    </select>
                                </label>
                                {classesLoading && <p>Загрузка списка классов...</p>}
                                {classesError && <p className="register-card__error" role="alert">{classesError}</p>}
                                {error && <p className="register-card__error" role="alert">{error}</p>}
                                {saved && <p className="profile-form__success" role="status">Класс сохранён.</p>}
                                <button className="register-form__submit" type="submit">
                                    Сохранить класс
                                </button>
                            </form>
                            <NotificationsSettings/>
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

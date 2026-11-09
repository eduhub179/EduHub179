import React from 'react';
import {useNavigate} from 'react-router-dom';
import {useAuth} from '../auth/AuthProvider';
import Header from '../components/Header';
import NotificationsSettings from '../components/NotificationsSettings';
import '../css/index.css';
import '../css/RegisterPage.css';

const ProfilePage: React.FC = () => {
    const {user, logout} = useAuth();
    const navigate = useNavigate();

    const handleLogout = async () => {
        await logout();
        navigate('/main');
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

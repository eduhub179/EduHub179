import React from 'react';
import { Link } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import '../css/Header.css';

const Header: React.FC = () => {
    const { user, logout } = useAuth();

    return (
        <header className="header">
            <div className="header__left">
                <Link to="/main" className="header__logo">
                    MyApp
                </Link>
            </div>

            <nav className="header__nav">
                <Link to="/math" className="header__link">
                    Математика
                </Link>

                <Link to="/main" className="header__link">
                    Главная
                </Link>

                <Link to="/schedule" className="header__link">
                    Расписание
                </Link>

                {user ? (
                    <>
                        <Link to="/profile" className="header__link">
                            Профиль
                        </Link>
                        <button className="header__link header__logout" onClick={logout} type="button">
                            Выйти
                        </button>
                    </>
                ) : (
                    <>
                        <Link to="/login" className="header__link">
                            Вход
                        </Link>
                        <Link to="/register" className="header__link">
                            Регистрация
                        </Link>
                    </>
                )}
            </nav>
        </header>
    );
};

export default Header;
import React from 'react';
import { Link } from 'react-router-dom';
import '../css/Header.css';

const Header: React.FC = () => {
    return (
        <header className="header">
            <div className="header__left">
                <Link to="/main" className="header__logo">
                    MyApp
                </Link>
            </div>

            <nav className="header__nav">
                <Link to="/main" className="header__link">
                    Главная
                </Link>

                <Link to="/schedule" className="header__link">
                    Расписание
                </Link>

                <Link to="/login" className="header__link">
                    Вход
                </Link>

                <Link to="/register" className="header__link">
                    Регистрация
                </Link>

                <Link to="/profile" className="header__link">
                    Профиль
                </Link>
            </nav>
        </header>
    );
};

export default Header;
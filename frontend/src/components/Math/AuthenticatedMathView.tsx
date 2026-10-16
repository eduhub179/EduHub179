import React from 'react';
import { Link } from 'react-router-dom';
import type { User } from '../../auth/authService';
import ClassParallelPicker from './ClassParallelPicker';
import MathClassSelector from './MathClassSelector';

interface AuthenticatedMathViewProps {
    user: User;
    schoolClass: string | undefined;
    onSelectClass: (schoolClass: string) => void;
}

const AuthenticatedMathView: React.FC<AuthenticatedMathViewProps> = ({
    user,
    schoolClass,
    onSelectClass,
}) => (
    <section className="math-content" aria-labelledby="math-title">
        <div className="math-content__top">
            <div>
                <p className="math-content__eyebrow">Учебный раздел</p>
                <h1 id="math-title">
                    Математика{schoolClass ? ` — ${schoolClass} класс` : ''}
                </h1>
            </div>
            {schoolClass && (
                <MathClassSelector schoolClass={schoolClass} onChange={onSelectClass} />
            )}
        </div>
        {schoolClass ? (
            <p className="math-content__description">
                Здесь будут собраны материалы по математике для вашего класса.
            </p>
        ) : user.schoolClass ? (
            <p className="math-content__description">
                Класс в профиле не входит в доступные параллели. Выберите класс ниже.
            </p>
        ) : (
            <>
                <p className="math-content__description">
                    Укажите класс в профиле, чтобы открыть свой раздел математики.
                </p>
                <Link className="math-content__link" to="/profile">
                    Перейти в профиль
                </Link>
            </>
        )}
        {!schoolClass && <ClassParallelPicker onSelectClass={onSelectClass} />}
    </section>
);

export default AuthenticatedMathView;

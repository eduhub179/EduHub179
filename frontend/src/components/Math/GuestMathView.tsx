import React from 'react';
import { Link } from 'react-router-dom';
import type {ClassOption} from '../ClassTable';
import ClassParallelPicker from './ClassParallelPicker';
import MathClassSelector from './MathClassSelector';

interface GuestMathViewProps {
    schoolClass: string | undefined;
    onSelectClass: (schoolClass: string) => void;
    classOptions: ClassOption[];
}

const GuestMathView: React.FC<GuestMathViewProps> = ({schoolClass, onSelectClass, classOptions}) => (
        <section className="math-content" aria-labelledby="math-title">
            <div className="math-content__top">
                <div>
                    <p className="math-content__eyebrow">Учебный раздел</p>
                    <h1 id="math-title">
                        Математика{schoolClass ? ` — ${schoolClass} класс` : ''}
                    </h1>
                </div>
                {schoolClass && (
                    <MathClassSelector classOptions={classOptions} schoolClass={schoolClass} onChange={onSelectClass} />
                )}
            </div>
            <p className="math-content__description">
                {schoolClass
                    ? 'Войдите в аккаунт или выберите другой класс.'
                    : 'Войдите в аккаунт или выберите свой класс.'}
            </p>
            <div className="math-content__actions">
                <Link className="math-content__link" to="/login">
                    Войти в аккаунт
                </Link>
            </div>

            {!schoolClass && <ClassParallelPicker classOptions={classOptions} onSelectClass={onSelectClass} />}
        </section>
);

export default GuestMathView;

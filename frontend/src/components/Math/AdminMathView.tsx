import React from 'react';
import ClassParallelPicker from './ClassParallelPicker';
import MathClassSelector from './MathClassSelector';

interface AdminMathViewProps {
    schoolClass: string | undefined;
    onSelectClass: (schoolClass: string) => void;
}

const AdminMathView: React.FC<AdminMathViewProps> = ({ schoolClass, onSelectClass }) => (
    <section className="math-content" aria-labelledby="math-title">
        <div className="math-content__top">
            <div>
                <p className="math-content__eyebrow">Панель администратора</p>
                <h1 id="math-title">
                    Материалы по математике{schoolClass ? ` — ${schoolClass} класс` : ''}
                </h1>
            </div>
            {schoolClass && (
                <MathClassSelector schoolClass={schoolClass} onChange={onSelectClass} />
            )}
        </div>
        <p className="math-content__description">
            Управление темами и файлами для раздела математики.
        </p>
        {!schoolClass && <ClassParallelPicker onSelectClass={onSelectClass} />}
    </section>
);

export default AdminMathView;

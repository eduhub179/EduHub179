import React from 'react';
import type {ClassOption} from '../ClassTable';
import ClassParallelPicker from './ClassParallelPicker';
import MathClassSelector from './MathClassSelector';

interface AdminMathViewProps {
    schoolClass: string | undefined;
    onSelectClass: (schoolClass: string) => void;
    classOptions: ClassOption[];
}

const AdminMathView: React.FC<AdminMathViewProps> = ({schoolClass, onSelectClass, classOptions}) => (
    <section className="math-content" aria-labelledby="math-title">
        <div className="math-content__top">
            <div>
                <p className="math-content__eyebrow">Панель администратора</p>
                <h1 id="math-title">
                    Материалы по математике{schoolClass ? ` — ${schoolClass} класс` : ''}
                </h1>
            </div>
            {schoolClass && (
                <MathClassSelector classOptions={classOptions} schoolClass={schoolClass} onChange={onSelectClass} />
            )}
        </div>
        <p className="math-content__description">
            Управление темами и файлами для раздела математики.
        </p>
        {!schoolClass && <ClassParallelPicker classOptions={classOptions} onSelectClass={onSelectClass} />}
    </section>
);

export default AdminMathView;

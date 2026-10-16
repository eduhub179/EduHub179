import React from 'react';
import classParallels from './mathClasses';

interface ClassParallelPickerProps {
    onSelectClass: (schoolClass: string) => void;
}

const ClassParallelPicker: React.FC<ClassParallelPickerProps> = ({ onSelectClass }) => (
    <div className="math-parallels" aria-label="Выбор класса">
        {classParallels.map(({ grade, classes }) => (
            <section className="math-parallel" key={grade} aria-labelledby={`parallel-${grade}`}>
                <h2 className="math-parallel__title" id={`parallel-${grade}`}>
                    {grade} класс
                </h2>
                <ul className="math-parallel__classes">
                    {classes.map((schoolClass) => (
                        <li key={schoolClass}>
                            <button
                                className="math-parallel__button"
                                onClick={() => onSelectClass(schoolClass)}
                                type="button"
                            >
                                {schoolClass}
                            </button>
                        </li>
                    ))}
                </ul>
            </section>
        ))}
    </div>
);

export default ClassParallelPicker;

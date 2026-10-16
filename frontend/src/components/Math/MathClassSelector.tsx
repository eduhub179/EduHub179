import React from 'react';
import { availableClasses } from './mathClasses';

interface MathClassSelectorProps {
    schoolClass: string;
    onChange: (schoolClass: string) => void;
}

const MathClassSelector: React.FC<MathClassSelectorProps> = ({ schoolClass, onChange }) => (
    <label className="math-content__select-label">
        <span>Класс</span>
        <select
            aria-label="Выбрать класс"
            onChange={(event) => onChange(event.target.value)}
            value={schoolClass}
        >
            {availableClasses.map((className) => (
                <option key={className} value={className}>
                    {className} класс
                </option>
            ))}
        </select>
    </label>
);

export default MathClassSelector;

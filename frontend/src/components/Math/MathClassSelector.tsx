import React, { useEffect, useRef, useState } from 'react';
import ClassTable, {type ClassOption} from '../ClassTable';
import '../../css/MathClassSelector.css';

interface MathClassSelectorProps {
    schoolClass: string;
    onChange: (schoolClass: string) => void;
    classOptions: ClassOption[];
}

const MathClassSelector: React.FC<MathClassSelectorProps> = ({schoolClass, onChange, classOptions}) => {
    const [isOpen, setIsOpen] = useState(false);
    const selectorRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!isOpen) {
            return;
        }

        const closeOnOutsideClick = (event: MouseEvent) => {
            if (!selectorRef.current?.contains(event.target as Node)) {
                setIsOpen(false);
            }
        };
        const closeOnEscape = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                setIsOpen(false);
            }
        };

        document.addEventListener('mousedown', closeOnOutsideClick);
        document.addEventListener('keydown', closeOnEscape);
        return () => {
            document.removeEventListener('mousedown', closeOnOutsideClick);
            document.removeEventListener('keydown', closeOnEscape);
        };
    }, [isOpen]);

    const selectClass = (nextClass: string) => {
        onChange(nextClass);
        setIsOpen(false);
    };

    return (
        <div className="math-class-selector" ref={selectorRef}>
            <span className="math-class-selector__label">Класс</span>
            <button
                aria-expanded={isOpen}
                aria-haspopup="dialog"
                aria-label={`Выбран класс ${schoolClass}. Нажмите, чтобы выбрать другой`}
                aria-controls="math-class-selector-options"
                className="math-class-selector__trigger"
                onClick={() => setIsOpen((open) => !open)}
                type="button"
            >
                {schoolClass}
                <span aria-hidden="true" className="math-class-selector__chevron">
                    {isOpen ? '▴' : '▾'}
                </span>
            </button>
            {isOpen && (
                <div
                    className="math-class-selector__menu"
                    id="math-class-selector-options"
                    role="dialog"
                    aria-label="Выбрать класс"
                >
                    <ClassTable
                        classOptions={classOptions}
                        onSelectClass={selectClass}
                        selectedClass={schoolClass}
                    />
                </div>
            )}
        </div>
    );
};

export default MathClassSelector;

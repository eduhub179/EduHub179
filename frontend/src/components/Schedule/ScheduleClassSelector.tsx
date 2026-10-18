import React, {useEffect, useMemo, useRef, useState} from 'react';
import type {SchoolClassDto} from '../../api/classApi';
import ClassTable from '../ClassTable';

interface ScheduleClassSelectorProps {
    classes: SchoolClassDto[];
    selectedClassId: string;
    onChange: (classId: string) => void;
}

const getSchoolYear = () => {
    const now = new Date();
    return now.getFullYear() - (now.getMonth() < 7 ? 1 : 0);
};

const getGrade = (graduationYear: number) => getSchoolYear() + 12 - graduationYear;

const getClassLabel = (schoolClass: SchoolClassDto) =>
    `${getGrade(schoolClass.graduation_year)}${schoolClass.letter.toLocaleUpperCase('ru-RU')}`;

const ScheduleClassSelector: React.FC<ScheduleClassSelectorProps> = ({
    classes,
    selectedClassId,
    onChange,
}) => {
    const [isOpen, setIsOpen] = useState(false);
    const selectorRef = useRef<HTMLDivElement>(null);
    const selectedClass = classes.find((schoolClass) => schoolClass.id === selectedClassId);
    const classOptions = useMemo(
        () => classes.map((schoolClass) => ({
            grade: getGrade(schoolClass.graduation_year),
            letter: schoolClass.letter.toLocaleUpperCase('ru-RU'),
            label: getClassLabel(schoolClass),
            value: schoolClass.id,
        })),
        [classes],
    );

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

    const selectClass = (classId: string) => {
        onChange(classId);
        setIsOpen(false);
    };

    return (
        <div className="schedule-class-selector" ref={selectorRef}>
            <span className="schedule-class-selector__label">Класс</span>
            <button
                aria-expanded={isOpen}
                aria-haspopup="dialog"
                aria-label={selectedClass
                    ? `Выбран ${getClassLabel(selectedClass)} класс. Нажмите, чтобы выбрать другой класс`
                    : 'Выбрать класс расписания'}
                aria-controls="schedule-class-selector-options"
                className="schedule-class-selector__trigger"
                onClick={() => setIsOpen((open) => !open)}
                type="button"
            >
                {selectedClass ? `${getClassLabel(selectedClass)} класс` : 'Выбрать класс'}
                <span aria-hidden="true" className="schedule-class-selector__chevron">
                    {isOpen ? '▴' : '▾'}
                </span>
            </button>
            {isOpen && (
                <div
                    className="schedule-class-selector__menu"
                    id="schedule-class-selector-options"
                    role="dialog"
                    aria-label="Выбрать класс расписания"
                >
                    {classes.length ? (
                        <ClassTable
                            classOptions={classOptions}
                            onSelectClass={selectClass}
                            selectedClass={selectedClassId}
                        />
                    ) : (
                        <p className="schedule-class-selector__empty">Список классов пуст.</p>
                    )}
                </div>
            )}
        </div>
    );
};

export default ScheduleClassSelector;

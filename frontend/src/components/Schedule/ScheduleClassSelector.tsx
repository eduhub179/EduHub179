import React, {useEffect, useMemo, useRef, useState} from 'react';
import {getSchoolClassLabel} from '../../api/classApi';
import type {SchoolClassDto} from '../../api/classApi';
import ClassTable from '../ClassTable';

interface ScheduleClassSelectorProps {
    classes: SchoolClassDto[];
    selectedClassId: string;
    onChange: (classId: string) => void;
    showTeacherSchedule?: boolean;
    isTeacherScheduleSelected?: boolean;
    onSelectTeacherSchedule?: () => void;
}

const ScheduleClassSelector: React.FC<ScheduleClassSelectorProps> = ({
    classes,
    selectedClassId,
    onChange,
    showTeacherSchedule = false,
    isTeacherScheduleSelected = false,
    onSelectTeacherSchedule,
}) => {
    const [isOpen, setIsOpen] = useState(false);
    const selectorRef = useRef<HTMLDivElement>(null);
    const selectedClass = classes.find((schoolClass) => schoolClass.id === selectedClassId);
    const classOptions = useMemo(
        () => classes.map((schoolClass) => ({
            grade: Number.parseInt(getSchoolClassLabel(schoolClass), 10),
            letter: schoolClass.letter.toLocaleUpperCase('ru-RU'),
            label: getSchoolClassLabel(schoolClass),
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
            <span className="schedule-class-selector__label">
                {showTeacherSchedule ? 'Расписание' : 'Класс'}
            </span>
            <button
                aria-expanded={isOpen}
                aria-haspopup="dialog"
                aria-label={isTeacherScheduleSelected
                    ? 'Выбрано моё расписание. Нажмите, чтобы выбрать расписание класса'
                    : selectedClass
                        ? `Выбран класс ${getSchoolClassLabel(selectedClass)}. Нажмите, чтобы выбрать другой`
                    : 'Выбрать класс расписания'}
                aria-controls="schedule-class-selector-options"
                className="schedule-class-selector__trigger"
                onClick={() => setIsOpen((open) => !open)}
                type="button"
            >
                {isTeacherScheduleSelected
                    ? 'Моё расписание'
                    : selectedClass
                        ? getSchoolClassLabel(selectedClass)
                        : 'Выбрать класс'}
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
                    {showTeacherSchedule && (
                        <button
                            className="schedule-class-selector__teacher-option"
                            type="button"
                            aria-pressed={isTeacherScheduleSelected}
                            onClick={() => {
                                onSelectTeacherSchedule?.();
                                setIsOpen(false);
                            }}
                        >
                            Моё расписание
                        </button>
                    )}
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

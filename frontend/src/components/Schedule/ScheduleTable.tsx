import React, {useState} from 'react';
import {useSchedule} from '../../hooks/useSchedule';
import type {ScheduleItem} from '../../types/ScheduleTypes.ts';
import '../../css/ScheduleTable.css';

interface FullScheduleTableProps {
    onLessonSelect: (lesson: ScheduleItem) => void;
    selectedClass: string;
}

const WEEKDAYS = ['Понедельник', 'Вторник', 'Среда', 'Четверг', 'Пятница', 'Суббота'];
const LESSONS_PER_DAY = 7;
const DEFAULT_LESSON_TIMES = [
    '08:30 – 09:15',
    '09:25 – 10:10',
    '10:20 – 11:05',
    '11:15 – 12:00',
    '12:10 – 12:55',
    '13:05 – 13:50',
    '14:00 – 14:45',
];

const FullScheduleTable: React.FC<FullScheduleTableProps> = ({onLessonSelect, selectedClass}) => {
    const {days, isLoading, error, hasClass} = useSchedule(selectedClass);
    const [hoveredDay, setHoveredDay] = useState<string | null>(null);
    const scheduleDays = WEEKDAYS.map((name) => ({
        name,
        lessons: (days.find((day) => day.name === name)?.items ?? [])
            .filter((lesson) => lesson.title)
            .slice()
            .sort((first, second) => first.startTime.localeCompare(second.startTime))
            .slice(0, LESSONS_PER_DAY),
    }));

    return (
        <div className="full-schedule-table">
            {isLoading ? (
                <p>Загрузка расписания...</p>
            ) : error ? (
                <p role="alert">{error}</p>
            ) : !hasClass ? (
                <p>Укажите класс в профиле, чтобы увидеть расписание.</p>
            ) : !days.length ? (
                <p>На этой неделе расписание не найдено.</p>
            ) : (
                <div className="full-schedule-table__scroll">
                    <table className="schedule-grid" onMouseLeave={() => setHoveredDay(null)}>
                        <thead>
                            <tr>
                                <th scope="col">Урок</th>
                                {scheduleDays.map((day) => (
                                    <th
                                        scope="col"
                                        key={day.name}
                                        className={hoveredDay === day.name ? 'schedule-grid__day-hovered' : undefined}
                                        onMouseEnter={() => setHoveredDay(day.name)}
                                    >
                                        {day.name}
                                    </th>
                                ))}
                            </tr>
                        </thead>
                        <tbody>
                            {Array.from({length: LESSONS_PER_DAY}, (_, lessonIndex) => {
                                const rowLesson = scheduleDays
                                    .map((day) => day.lessons[lessonIndex])
                                    .find(Boolean);

                                return (
                                    <tr key={lessonIndex}>
                                        <th scope="row">
                                            <div className="schedule-grid__row-heading">
                                                <span>Урок {lessonIndex + 1}</span>
                                                <time>
                                                    {rowLesson
                                                        ? `${rowLesson.startTime} – ${rowLesson.endTime}`
                                                        : DEFAULT_LESSON_TIMES[lessonIndex]}
                                                </time>
                                            </div>
                                        </th>
                                        {scheduleDays.map((day) => {
                                            const lesson = day.lessons[lessonIndex];
                                            return (
                                                <td
                                                    key={day.name}
                                                    className={hoveredDay === day.name ? 'schedule-grid__day-hovered' : undefined}
                                                    onMouseEnter={() => setHoveredDay(day.name)}
                                                >
                                                    {lesson && (
                                                        <button
                                                            className="schedule-grid__lesson"
                                                            type="button"
                                                            onClick={() => onLessonSelect(lesson)}
                                                            aria-label={`Открыть урок ${lesson.lessonNumber}: ${lesson.title}`}
                                                        >
                                                            <span className="schedule-grid__lesson-content">
                                                                <strong>{lesson.title}</strong>
                                                                {(lesson.cabinet?.trim() || lesson.teachers?.some((teacher) => teacher.trim())) && (
                                                                    <small className="schedule-grid__lesson-meta">
                                                                        {[
                                                                            lesson.cabinet?.trim(),
                                                                            lesson.teachers?.some((teacher) => teacher.trim()) &&
                                                                                lesson.teachers.filter((teacher) => teacher.trim()).join(', '),
                                                                        ].filter(Boolean).join(' · ')}
                                                                    </small>
                                                                )}
                                                            </span>
                                                            {lesson.homework && (
                                                                <span className="schedule-grid__homework-badge" aria-label="Есть домашнее задание">
                                                                    ДЗ
                                                                </span>
                                                            )}
                                                        </button>
                                                    )}
                                                </td>
                                            );
                                        })}
                                    </tr>
                                );
                            })}
                        </tbody>
                    </table>
                </div>
            )}
        </div>
    );
};

export default FullScheduleTable;
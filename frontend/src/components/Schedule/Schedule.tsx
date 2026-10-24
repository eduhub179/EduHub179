import React, {useState} from 'react';
import {Link} from 'react-router-dom';

import {useSchedule} from '../../hooks/useSchedule';
import {changeLabel, type ScheduleItem} from '../../types/ScheduleTypes.ts';
import '../../css/Schedule.css';

interface ScheduleProps {
    onLessonSelect: (lesson: ScheduleItem) => void;
}

const Schedule: React.FC<ScheduleProps> = ({onLessonSelect}) => {
    const [dayIndex, setDayIndex] = useState(0);
    const {days, isLoading, error, hasClass} = useSchedule();

    const changeDay = (direction: number) => {
        setDayIndex((current) => {
            const newIndex = current + direction;

            if (newIndex < 0) {
                return days.length - 1;
            }

            if (newIndex >= days.length) {
                return 0;
            }

            return newIndex;
        });
    };

    const day = days[dayIndex] ?? days[0];
    const scheduleData: ScheduleItem[] = day?.items ?? [];
    const scheduleGroups = scheduleData.reduce<ScheduleItem[][]>((groups, item) => {
        const group = groups.find((items) =>
            items[0].startTime === item.startTime && items[0].endTime === item.endTime,
        );

        if (group) {
            group.push(item);
        } else {
            groups.push([item]);
        }

        return groups;
    }, []);

    return (
        <aside className="schedule">
            <header className="schedule__header">
                <h2>Расписание уроков</h2>

                <div className="schedule__header-right">
                    <Link
                        to="/schedule"
                        className="schedule__full-link"
                    >
                        Всё расписание
                    </Link>

                    <div className="schedule__date">
                        <button
                            className="schedule__day-button"
                            onClick={() => changeDay(-1)}
                            aria-label="Предыдущий день"
                        >
                            ←
                        </button>

                        <span>{day?.name ?? '—'}</span>

                        <button
                            className="schedule__day-button"
                            onClick={() => changeDay(1)}
                            aria-label="Следующий день"
                        >
                            →
                        </button>
                    </div>
                </div>
            </header>

            {isLoading ? (
                <p>Загрузка расписания...</p>
            ) : error ? (
                <p role="alert">{error}</p>
            ) : !hasClass ? (
                <p>Укажите класс в профиле, чтобы увидеть расписание.</p>
            ) : !days.length ? (
                <p>На этой неделе расписание не найдено.</p>
            ) : !scheduleData.length ? (
                <p>На этот день уроков нет.</p>
            ) : (
                <ul className="schedule__list">
                    {scheduleGroups.map((group, index) => {
                        const item = group[0];

                        return (
                            <li key={`${item.startTime}-${item.endTime}`} className="schedule__item">
                                <div className="schedule__lesson-row">
                                    <div className="schedule__time">
                                        <span>{item.startTime}</span>
                                        <span>{item.endTime}</span>
                                    </div>

                                    <div className="schedule__lesson-group">
                                        {group.map((lesson, lessonIndex) => (
                                            <button
                                                className={`schedule__lesson-button${group.length > 1 && lessonIndex > 0 ? ' schedule__lesson-button--split' : ''}${lesson.cancelled ? ' schedule__lesson-button--cancelled' : ''}${changeLabel(lesson) ? ' schedule__lesson-button--changed' : ''}`}
                                                type="button"
                                                key={lesson.id}
                                                onClick={() => onLessonSelect(lesson)}
                                                aria-label={`Урок ${index + 1}: ${lesson.title || 'без названия'}, ${lesson.startTime}–${lesson.endTime}`}
                                            >
                                                <div className="schedule__details">
                                                    <div className="schedule__lesson-content">
                                                        <div className="schedule__lesson-number">
                                                            Урок {index + 1}
                                                        </div>

                                                        <div className={`schedule__title${lesson.cancelled ? ' schedule__title--cancelled' : ''}`}>
                                                            {lesson.title || '—'}
                                                            {lesson.cancelled && <span className="schedule__cancelled-badge">отменён</span>}
                                                            {changeLabel(lesson) && <span className="schedule__changed-badge">{changeLabel(lesson)}</span>}
                                                        </div>
                                                        {(lesson.cabinet?.trim() || lesson.teachers?.some((teacher) => teacher.trim())) && (
                                                            <div className="schedule__lesson-meta">
                                                                {[
                                                                    lesson.cabinet?.trim(),
                                                                    lesson.teachers?.some((teacher) => teacher.trim()) &&
                                                                    lesson.teachers.filter((teacher) => teacher.trim()).join(', '),
                                                                ].filter(Boolean).join(' · ')}
                                                            </div>
                                                        )}
                                                        {lesson.note && (
                                                            <div className="schedule__lesson-meta">{lesson.note}</div>
                                                        )}
                                                    </div>
                                                    {group.length > 1
                                                        ? lesson.homework && (
                                                            <span
                                                                className="schedule__homework-badge"
                                                                aria-label="Есть домашнее задание"
                                                            >
                                                                ДЗ
                                                            </span>
                                                        )
                                                        : lesson.homework && (
                                                            <span className="schedule__homework-button">
                                                                {lesson.homework.description.trim() ||
                                                                    (lesson.homework.files.length ? 'См. файлы' : 'Есть домашнее задание')}
                                                            </span>
                                                        )}
                                                </div>
                                            </button>
                                        ))}
                                    </div>
                                </div>
                            </li>
                        );
                    })}
                </ul>
            )}
        </aside>
    );
};

export default Schedule;
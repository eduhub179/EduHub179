import React, {useState} from 'react';
import {Link} from 'react-router-dom';

import {useSchedule} from '../../hooks/useSchedule';
import type {ScheduleItem} from '../../types/ScheduleTypes.ts';
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
                    {scheduleData.map((item) => (
                        <li key={item.id} className="schedule__item">
                            <button
                                className="schedule__lesson-button"
                                type="button"
                                onClick={() => onLessonSelect(item)}
                                aria-label={`Урок ${item.lessonNumber}: ${item.title || 'без названия'}, ${item.startTime}–${item.endTime}`}
                            >
                                <div className="schedule__time">
                                    <span>{item.startTime}</span>
                                    <span>{item.endTime}</span>
                                </div>

                                <div className="schedule__details">
                                    <div className="schedule__lesson-content">
                                        <div className="schedule__lesson-number">
                                            Урок {item.lessonNumber}
                                        </div>

                                        <div className="schedule__title">
                                            {item.title || '—'}
                                        </div>
                                        {(item.cabinet?.trim() || item.teachers?.some((teacher) => teacher.trim())) && (
                                            <div className="schedule__lesson-meta">
                                                {[
                                                    item.cabinet?.trim(),
                                                    item.teachers?.some((teacher) => teacher.trim()) &&
                                                        item.teachers.filter((teacher) => teacher.trim()).join(', '),
                                                ].filter(Boolean).join(' · ')}
                                            </div>
                                        )}
                                    </div>
                                    {item.homework && (
                                        <span className="schedule__homework-button">
                                            {item.homework.description}
                                        </span>
                                    )}
                                    {item.homework && (
                                        <span className="schedule__homework-badge" aria-label="Есть домашнее задание">
                                            ДЗ
                                        </span>
                                    )}
                                </div>
                            </button>
                        </li>
                    ))}
                </ul>
            )}
        </aside>
    );
};

export default Schedule;
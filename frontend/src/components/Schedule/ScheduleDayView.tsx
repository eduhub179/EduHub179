import React from 'react';
import {addDays, getLocalDate, mondayOf, useSchedule} from '../../hooks/useSchedule';
import type {ScheduleItem} from '../../types/ScheduleTypes';
import DayLessonList from './DayLessonList';

interface ScheduleDayViewProps {
    selectedClass: string;
    scheduleType: 'class' | 'teacher';
    /** The shown day, `YYYY-MM-DD`; its week is loaded. */
    date: string;
    onDateChange: (date: string) => void;
    onLessonSelect: (lesson: ScheduleItem) => void;
}

const chipFormat = new Intl.DateTimeFormat('ru-RU', {weekday: 'short', day: 'numeric'});

/** One day of the schedule: day chips Mon–Sat of the week and that day's lessons. */
const ScheduleDayView: React.FC<ScheduleDayViewProps> = ({
    selectedClass,
    scheduleType,
    date,
    onDateChange,
    onLessonSelect,
}) => {
    const {days, isLoading, error, hasClass} = useSchedule(selectedClass, scheduleType, date);
    const monday = mondayOf(date);
    const today = getLocalDate(new Date());
    const weekDays = Array.from({length: 6}, (_, index) => addDays(monday, index));
    const lessons = days.flatMap((day) => day.items).filter((lesson) => lesson.date === date);

    return (
        <div className="schedule-day-view">
            <div className="day-chips" role="group" aria-label="День недели">
                {weekDays.map((day) => (
                    <button
                        aria-pressed={day === date}
                        className="view-switch__option"
                        key={day}
                        onClick={() => onDateChange(day)}
                        type="button"
                    >
                        {chipFormat.format(new Date(`${day}T00:00:00`))}
                        {day === today && ' · сегодня'}
                    </button>
                ))}
            </div>
            {isLoading ? (
                <p>Загрузка расписания...</p>
            ) : error ? (
                <p role="alert">{error}</p>
            ) : !hasClass ? (
                <p>Выберите класс, чтобы увидеть расписание.</p>
            ) : !days.length ? (
                <p>Расписание на эту неделю ещё не опубликовано.</p>
            ) : (
                <DayLessonList
                    lessons={lessons}
                    markNow={date === today}
                    onLessonSelect={onLessonSelect}
                />
            )}
        </div>
    );
};

export default ScheduleDayView;

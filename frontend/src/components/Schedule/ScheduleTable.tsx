import React from 'react';
import {useSchedule} from '../../hooks/useSchedule';
import ScheduleDay from './ScheduleDay';
import '../../css/ScheduleTable.css';

const FullScheduleTable: React.FC = () => {
    const {days, isLoading, error, hasClass} = useSchedule();
    const lessons = days[0]?.items ?? [];
    const toMinutes = (time: string) => {
        const [hours, minutes] = time.split(':').map(Number);
        return hours * 60 + minutes;
    };

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
                <>
                    <div className="schedule-time-axis" aria-hidden="true">
                        {lessons.map((lesson, index) => {
                            const nextLesson = lessons[index + 1];
                            const breakMinutes = nextLesson
                                ? toMinutes(nextLesson.startTime) - toMinutes(lesson.endTime)
                                : 0;

                            return (
                                <span key={lesson.id}>
                                    <strong>{lesson.startTime}</strong>
                                    {breakMinutes > 0 && <small>{breakMinutes} мин</small>}
                                </span>
                            );
                        })}
                    </div>

                    {days.map((day) => (
                        <ScheduleDay
                            key={day.name}
                            day={day}
                        />
                    ))}
                </>
            )}
        </div>
    );
};

export default FullScheduleTable;
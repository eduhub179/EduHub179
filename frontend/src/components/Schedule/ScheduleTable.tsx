import React from 'react';
import { weekSchedule } from '../../local_data/ScheduleData.ts';
import ScheduleDay from './ScheduleDay';
import '../../css/ScheduleTable.css';

const FullScheduleTable: React.FC = () => {
    const lessons = weekSchedule[0]?.items ?? [];
    const toMinutes = (time: string) => {
        const [hours, minutes] = time.split(':').map(Number);
        return hours * 60 + minutes;
    };

    return (
        <div className="full-schedule-table">
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

            {weekSchedule.map((day) => (
                <ScheduleDay
                    key={day.name}
                    day={day}
                />
            ))}
        </div>
    );
};

export default FullScheduleTable;
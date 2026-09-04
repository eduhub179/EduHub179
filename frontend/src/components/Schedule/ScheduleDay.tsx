import React from 'react';
import type {DaySchedule} from '../../types/ScheduleTypes.ts';
import '../../css/ScheduleTable.css';

interface ScheduleDayProps {
    day: DaySchedule;
}

const toMinutes = (time: string) => {
    const [hours, minutes] = time.split(':').map(Number);
    return hours * 60 + minutes;
};

const ScheduleDay: React.FC<ScheduleDayProps> = ({day}) => {
    return (
        <section className="schedule-day">
            <header className="schedule-day__header">
                <span className="schedule-day__short-name">
                    {day.shortName}
                </span>

                <span className="schedule-day__name">
                    {day.name}
                </span>
            </header>
            <div className="schedule-day__lessons">
                {day.items.map((lesson, index) => {
                    const nextLesson = day.items[index + 1];
                    const breakMinutes = nextLesson
                        ? toMinutes(nextLesson.startTime) - toMinutes(lesson.endTime)
                        : 0;

                    return (
                        <div className="schedule-day__slot" key={lesson.id}>
                            <div className={`schedule-day__lesson${lesson.title ? '' : ' schedule-day__lesson--empty'}`}>
                                <span className="schedule-day__number">
                                    {lesson.lessonNumber}
                                </span>

                                <div className="schedule-day__info">
                                    <div className="schedule-day__title">
                                        {lesson.title}
                                    </div>

                                    <time className="schedule-day__time">
                                        {lesson.startTime} – {lesson.endTime}
                                    </time>
                                </div>
                            </div>

                            {breakMinutes > 0 && (
                                <div className="schedule-day__break">
                                    {breakMinutes} мин
                                </div>
                            )}
                        </div>
                    );
                })}
            </div>
        </section>
    );
};

export default ScheduleDay;
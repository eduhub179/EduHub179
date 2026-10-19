import React, {useState} from 'react';
import {useSchedule} from '../../hooks/useSchedule';
import type {ScheduleItem} from '../../types/ScheduleTypes.ts';
import '../../css/ScheduleTable.css';

interface FullScheduleTableProps {
    onLessonSelect: (lesson: ScheduleItem) => void;
    selectedClass: string;
}

const WEEKDAYS = ['Понедельник', 'Вторник', 'Среда', 'Четверг', 'Пятница', 'Суббота'];
const DEFAULT_LESSON_TIMES = [
    '09:00 – 09:45',
    '09:55 – 10:40',
    '11:00 – 11:45',
    '11:55 – 12:40',
    '13:10 – 13:55',
    '14:15 – 15:00',
    '15:10 – 15:55',
];
const LESSON_SLOT_HEIGHT = 63;

const DEFAULT_LESSON_INTERVALS = DEFAULT_LESSON_TIMES.map((lessonTime) => {
    const [startTime, endTime] = lessonTime.split(' – ');
    return {
        start: startTime.split(':').map(Number).reduce((hours, minutes) => hours * 60 + minutes),
        end: endTime.split(':').map(Number).reduce((hours, minutes) => hours * 60 + minutes),
    };
});

const parseTimeInMinutes = (time: string): number => {
    const [hours, minutes] = time.split(':').map(Number);
    return hours * 60 + minutes;
};

const getTimelinePosition = (time: string): number => {
    const lessonMinutes = parseTimeInMinutes(time);

    for (let index = 0; index < DEFAULT_LESSON_INTERVALS.length; index += 1) {
        const interval = DEFAULT_LESSON_INTERVALS[index];
        const duration = interval.end - interval.start;

        if (lessonMinutes < interval.start) {
            return (index + (lessonMinutes - interval.start) / duration) * LESSON_SLOT_HEIGHT;
        }

        if (lessonMinutes <= interval.end) {
            return (index + (lessonMinutes - interval.start) / duration) * LESSON_SLOT_HEIGHT;
        }

        const nextInterval = DEFAULT_LESSON_INTERVALS[index + 1];
        if (nextInterval && lessonMinutes < nextInterval.start) {
            return (index + 1) * LESSON_SLOT_HEIGHT;
        }
    }

    const lastIndex = DEFAULT_LESSON_INTERVALS.length - 1;
    const lastInterval = DEFAULT_LESSON_INTERVALS[lastIndex];
    return (lastIndex + (lessonMinutes - lastInterval.start) / (lastInterval.end - lastInterval.start)) *
        LESSON_SLOT_HEIGHT;
};

const FullScheduleTable: React.FC<FullScheduleTableProps> = ({onLessonSelect, selectedClass}) => {
    const {days, isLoading, error, hasClass} = useSchedule(selectedClass);
    const [hoveredDay, setHoveredDay] = useState<string | null>(null);
    const scheduleDays = WEEKDAYS.map((name) => ({
        name,
        lessons: (days.find((day) => day.name === name)?.items ?? [])
            .filter((lesson) => lesson.title)
            .slice()
            .sort((first, second) => first.startTime.localeCompare(second.startTime))
    }));

    const getLessonPositioning = (lessons: ScheduleItem[]) => {
        const slotTotals = new Map<string, number>();
        const slotSeen = new Map<string, number>();

        lessons.forEach((lesson) => {
            const slotKey = `${lesson.startTime}-${lesson.endTime}`;
            slotTotals.set(slotKey, (slotTotals.get(slotKey) ?? 0) + 1);
        });

        return lessons.map((lesson) => {
            const slotKey = `${lesson.startTime}-${lesson.endTime}`;
            const slotCount = slotTotals.get(slotKey) ?? 1;
            const slotIndex = slotSeen.get(slotKey) ?? 0;
            slotSeen.set(slotKey, slotIndex + 1);

            return {
                lesson,
                slotCount,
                slotIndex,
            };
        });
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
                            <tr>
                                <th scope="row">
                                    <div className="schedule-grid__row-heading">
                                        {DEFAULT_LESSON_TIMES.map((time, index) => (
                                            <div className="schedule-grid__row-label" key={time}>
                                                <span>Урок {index + 1}</span>
                                                <time>{time}</time>
                                            </div>
                                        ))}
                                    </div>
                                </th>
                                {scheduleDays.map((day) => (
                                    <td
                                        key={day.name}
                                        className={hoveredDay === day.name ? 'schedule-grid__day-hovered' : undefined}
                                        onMouseEnter={() => setHoveredDay(day.name)}
                                    >
                                        <div className="schedule-grid__timeline">
                                            {getLessonPositioning(day.lessons).map(({lesson, slotCount, slotIndex}) => {
                                                const top = getTimelinePosition(lesson.startTime);
                                                const bottom = getTimelinePosition(lesson.endTime);
                                                const isNonStandardTime = !DEFAULT_LESSON_INTERVALS.some(
                                                    ({start, end}) =>
                                                        start === parseTimeInMinutes(lesson.startTime) &&
                                                        end === parseTimeInMinutes(lesson.endTime),
                                                );

                                                const width = 100 / slotCount;
                                                const left = width * slotIndex;
                                                return (
                                                    <button
                                                        className={`schedule-grid__lesson${isNonStandardTime ? ' schedule-grid__lesson--non-standard' : ''}${slotCount > 1 ? ' schedule-grid__lesson--split' : ''}`}
                                                        style={{
                                                            top,
                                                            left: `${left}%`,
                                                            width: `${width}%`,
                                                            height: Math.max(0, bottom - top),
                                                        }}
                                                        type="button"
                                                        key={lesson.id}
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
                                                );
                                            })}
                                        </div>
                                    </td>
                                ))}
                            </tr>
                        </tbody>
                    </table>
                </div>
            )}
        </div>
    );
};

export default FullScheduleTable;
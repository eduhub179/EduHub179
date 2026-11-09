import React from 'react';
import {addDays, getLocalDate, mondayOf} from '../../hooks/useSchedule';

interface WeekNavProps {
    /** Any date of the shown week, `YYYY-MM-DD`. */
    weekDate: string;
    onChange: (date: string) => void;
}

const dayMonth = new Intl.DateTimeFormat('ru-RU', {day: 'numeric', month: 'long'});

/** "10–15 ноября", or "29 октября – 3 ноября" when the week spans two months. */
const formatRange = (monday: string): string => {
    const start = new Date(`${monday}T00:00:00`);
    const end = new Date(`${addDays(monday, 5)}T00:00:00`);
    return start.getMonth() === end.getMonth()
        ? `${start.getDate()}–${dayMonth.format(end)}`
        : `${dayMonth.format(start)} – ${dayMonth.format(end)}`;
};

/** "← 10–15 ноября →" with a jump back to the current week. */
const WeekNav: React.FC<WeekNavProps> = ({weekDate, onChange}) => {
    const monday = mondayOf(weekDate);
    const isCurrentWeek = monday === mondayOf(getLocalDate(new Date()));

    return (
        <div className="week-nav" role="group" aria-label="Неделя">
            <button
                className="week-nav__arrow"
                onClick={() => onChange(addDays(monday, -7))}
                type="button"
                aria-label="Предыдущая неделя"
            >
                ←
            </button>
            <span className="week-nav__label">{formatRange(monday)}</span>
            <button
                className="week-nav__arrow"
                onClick={() => onChange(addDays(monday, 7))}
                type="button"
                aria-label="Следующая неделя"
            >
                →
            </button>
            {!isCurrentWeek && (
                <button
                    className="week-nav__today"
                    onClick={() => onChange(getLocalDate(new Date()))}
                    type="button"
                >
                    Эта неделя
                </button>
            )}
        </div>
    );
};

export default WeekNav;

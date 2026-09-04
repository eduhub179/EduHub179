import React, {useState} from 'react';
import {Link} from 'react-router-dom';

import '../../css/Schedule.css';

interface ScheduleItem {
    id: number;
    startTime: string;
    endTime: string;
    lessonNumber: number;
    title: string;
}

const scheduleData_example: ScheduleItem[] = [
    {id: 1, startTime: '08:30', endTime: '09:15', lessonNumber: 1, title: 'Математика'},
    {id: 2, startTime: '09:25', endTime: '10:10', lessonNumber: 2, title: 'Русский язык'},
    {id: 3, startTime: '10:20', endTime: '11:05', lessonNumber: 3, title: 'Физика'},
    {id: 4, startTime: '11:15', endTime: '12:00', lessonNumber: 4, title: 'Химия'},
    {id: 5, startTime: '12:10', endTime: '12:55', lessonNumber: 5, title: 'История'},
    {id: 6, startTime: '13:05', endTime: '13:50', lessonNumber: 6, title: 'Английский язык'},
    {id: 7, startTime: '14:00', endTime: '14:45', lessonNumber: 7, title: 'Физкультура'},
];

const days = [
    'Monday',
    'Tuesday',
    'Wednesday',
    'Thursday',
    'Friday',
    'Saturday',
    'Sunday',
];

const Schedule: React.FC = () => {
    const [dayIndex, setDayIndex] = useState(0);
    const [scheduleData] = useState<ScheduleItem[]>(scheduleData_example);

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

    const day = days[dayIndex];

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

                        <span>{day}</span>

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

            <ul className="schedule__list">
                {scheduleData.map((item) => (
                    <li key={item.id} className="schedule__item">
                        <div className="schedule__time">
                            <span>{item.startTime}</span>
                            <span>{item.endTime}</span>
                        </div>

                        <div className="schedule__details">
                            <div className="schedule__lesson-number">
                                Урок {item.lessonNumber}
                            </div>

                            <div className="schedule__title">
                                {item.title}
                            </div>
                        </div>
                    </li>
                ))}
            </ul>
        </aside>
    );
};

export default Schedule;
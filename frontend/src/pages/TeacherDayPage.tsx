import React, {useState} from 'react';
import {Link} from 'react-router-dom';
import {hasTeacherRole} from '../api/access';
import {useAuth} from '../auth/AuthProvider';
import Header from '../components/Header';
import DayLessonList from '../components/Schedule/DayLessonList';
import LessonModal from '../components/Schedule/LessonModal';
import {addDays, getLocalDate, useSchedule} from '../hooks/useSchedule';
import type {ScheduleItem} from '../types/ScheduleTypes';
import '../css/index.css';
import '../css/DayLessonList.css';
import '../css/TeacherDayPage.css';

const dayTitle = new Intl.DateTimeFormat('ru-RU', {weekday: 'long', day: 'numeric', month: 'long'});

/** The teacher's lessons of one day; each lesson says whether homework is set and opens the lesson card. */
const TeacherDayPage: React.FC = () => {
    const {user} = useAuth();
    const isTeacher = hasTeacherRole(user);
    const today = getLocalDate(new Date());
    const [date, setDate] = useState(today);
    const [selectedLesson, setSelectedLesson] = useState<ScheduleItem | null>(null);
    const {days, isLoading, error} = useSchedule(undefined, 'teacher', date);

    const lessons = days.flatMap((day) => day.items).filter((lesson) => lesson.date === date);
    const held = lessons.filter((lesson) => !lesson.cancelled);
    const withoutHomework = held.filter((lesson) => !lesson.homework).length;
    const title = dayTitle.format(new Date(`${date}T00:00:00`));

    return (
        <>
            <Header/>
            <main className="teacher-day">
                <div className="teacher-day__heading">
                    <div>
                        <h1>Мой день</h1>
                        <p className="teacher-day__date">
                            {title.charAt(0).toLocaleUpperCase('ru-RU') + title.slice(1)}
                            {date === today && ' · сегодня'}
                        </p>
                    </div>
                    {/* Arrows first and "Сегодня" last, so the arrows never move under the cursor. */}
                    <div className="week-nav" role="group" aria-label="День">
                        <button
                            aria-label="Предыдущий день"
                            className="week-nav__arrow"
                            onClick={() => setDate(addDays(date, -1))}
                            type="button"
                        >
                            ←
                        </button>
                        <button
                            aria-label="Следующий день"
                            className="week-nav__arrow"
                            onClick={() => setDate(addDays(date, 1))}
                            type="button"
                        >
                            →
                        </button>
                        {date !== today && (
                            <button className="week-nav__today" onClick={() => setDate(today)} type="button">
                                Сегодня
                            </button>
                        )}
                    </div>
                </div>

                {!user ? (
                    <p>Войдите в аккаунт, чтобы увидеть свои уроки.</p>
                ) : !isTeacher ? (
                    <p>Эта страница для учителей. Расписание класса — на странице <Link to="/schedule">«Расписание»</Link>.</p>
                ) : isLoading ? (
                    <p>Загрузка уроков...</p>
                ) : error ? (
                    <p role="alert">{error}</p>
                ) : (
                    <>
                        {held.length > 0 && (
                            <p className="teacher-day__summary">
                                Уроков: {held.length}
                                {' · '}
                                {withoutHomework ? `без ДЗ: ${withoutHomework}` : 'ДЗ задано везде'}
                            </p>
                        )}
                        <DayLessonList
                            emptyText={days.length ? 'В этот день у вас нет уроков.' : 'Расписание на эту неделю ещё не опубликовано.'}
                            homeworkActions
                            lessons={lessons}
                            markNow={date === today}
                            onLessonSelect={setSelectedLesson}
                        />
                        <p className="teacher-day__more">
                            Вся неделя и расписание классов — на странице <Link to="/schedule">«Расписание»</Link>.
                        </p>
                    </>
                )}
            </main>
            <LessonModal
                lesson={selectedLesson}
                onClose={() => setSelectedLesson(null)}
                onLessonUpdated={setSelectedLesson}
            />
        </>
    );
};

export default TeacherDayPage;

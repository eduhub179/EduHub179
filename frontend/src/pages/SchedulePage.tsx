import React, {useEffect, useState} from 'react';
import {fetchSchoolClasses} from '../api/classApi';
import type {SchoolClassDto} from '../api/classApi';
import {hasTeacherRole} from '../api/access';
import {useAuth} from '../auth/AuthProvider';
import Header from '../components/Header';
import ScheduleClassSelector from '../components/Schedule/ScheduleClassSelector';
import FullScheduleTable from '../components/Schedule/ScheduleTable.tsx';
import ScheduleDayView from '../components/Schedule/ScheduleDayView';
import LessonModal from '../components/Schedule/LessonModal';
import WeekNav from '../components/Schedule/WeekNav';
import {getLocalDate} from '../hooks/useSchedule';
import type {ScheduleItem} from '../types/ScheduleTypes.ts';
import '../css/FullSchedule.css';
import '../css/DayLessonList.css';

const FullSchedule: React.FC = () => {
    const {user} = useAuth();
    const [selectedLesson, setSelectedLesson] = useState<ScheduleItem | null>(null);
    const [selectedClass, setSelectedClass] = useState(user?.schoolClass?.trim() ?? '');
    const [isTeacherScheduleSelected, setIsTeacherScheduleSelected] = useState(hasTeacherRole(user));
    const [classes, setClasses] = useState<SchoolClassDto[]>([]);
    const [classListError, setClassListError] = useState<string | null>(null);
    // The shown day (day view) / any day of the shown week (week view).
    const [date, setDate] = useState(() => getLocalDate(new Date()));
    const [view, setView] = useState<'week' | 'day'>('week');

    useEffect(() => {
        let isCurrent = true;
        fetchSchoolClasses()
            .then(({classes: schoolClasses}) => {
                if (isCurrent) {
                    setClasses(schoolClasses.filter((schoolClass) => schoolClass.is_active));
                }
            })
            .catch((error: unknown) => {
                if (isCurrent) {
                    setClassListError(error instanceof Error ? error.message : 'Не удалось загрузить список классов.');
                }
            });

        return () => {
            isCurrent = false;
        };
    }, []);

    useEffect(() => {
        if (user?.schoolClass) {
            setSelectedClass(user.schoolClass.trim());
        }
    }, [user?.schoolClass]);

    return (
        <>
            <Header />

            <main className="full-schedule">
                <div className="full-schedule__header">
                    <h1>Полное расписание</h1>
                    <ScheduleClassSelector
                        classes={classes}
                        selectedClassId={selectedClass}
                        onChange={(classId) => {
                            setSelectedClass(classId);
                            setIsTeacherScheduleSelected(false);
                        }}
                        showTeacherSchedule={hasTeacherRole(user)}
                        isTeacherScheduleSelected={isTeacherScheduleSelected}
                        onSelectTeacherSchedule={() => setIsTeacherScheduleSelected(true)}
                    />
                </div>

                <div className="full-schedule__controls">
                    <WeekNav weekDate={date} onChange={setDate}/>
                    <div className="view-switch" role="group" aria-label="Вид расписания">
                        <button
                            aria-pressed={view === 'week'}
                            className="view-switch__option"
                            onClick={() => setView('week')}
                            type="button"
                        >
                            Неделя
                        </button>
                        <button
                            aria-pressed={view === 'day'}
                            className="view-switch__option"
                            onClick={() => setView('day')}
                            type="button"
                        >
                            День
                        </button>
                    </div>
                </div>
                {classListError && <p className="full-schedule__error" role="alert">{classListError}</p>}
                {view === 'week' ? (
                    <FullScheduleTable
                        onLessonSelect={setSelectedLesson}
                        selectedClass={selectedClass}
                        scheduleType={isTeacherScheduleSelected ? 'teacher' : 'class'}
                        weekDate={date}
                    />
                ) : (
                    <ScheduleDayView
                        date={date}
                        onDateChange={setDate}
                        onLessonSelect={setSelectedLesson}
                        scheduleType={isTeacherScheduleSelected ? 'teacher' : 'class'}
                        selectedClass={selectedClass}
                    />
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

export default FullSchedule;
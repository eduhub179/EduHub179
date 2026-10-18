import React, {useEffect, useState} from 'react';
import {fetchSchoolClasses} from '../api/classApi';
import type {SchoolClassDto} from '../api/classApi';
import {useAuth} from '../auth/AuthProvider';
import Header from '../components/Header';
import ScheduleClassSelector from '../components/Schedule/ScheduleClassSelector';
import FullScheduleTable from '../components/Schedule/ScheduleTable.tsx';
import MainContent from '../components/MainContent.tsx';
import type {ScheduleItem} from '../types/ScheduleTypes.ts';
import '../css/FullSchedule.css';


const FullSchedule: React.FC = () => {
    const {user} = useAuth();
    const [selectedLesson, setSelectedLesson] = useState<ScheduleItem | null>(null);
    const [selectedClass, setSelectedClass] = useState(user?.schoolClass?.trim() ?? '');
    const [classes, setClasses] = useState<SchoolClassDto[]>([]);
    const [classListError, setClassListError] = useState<string | null>(null);

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

    useEffect(() => {
        if (!selectedLesson) {
            return;
        }

        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                setSelectedLesson(null);
            }
        };

        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [selectedLesson]);

    return (
        <>
            <Header />

            <main className="full-schedule">
                <div className="full-schedule__header">
                    <h1>Полное расписание</h1>
                    <ScheduleClassSelector
                        classes={classes}
                        selectedClassId={selectedClass}
                        onChange={setSelectedClass}
                    />
                </div>

                {classListError && <p className="full-schedule__error" role="alert">{classListError}</p>}
                <FullScheduleTable
                    onLessonSelect={setSelectedLesson}
                    selectedClass={selectedClass}
                />
            </main>
            {selectedLesson && (
                <div
                    className="schedule-lesson-modal"
                    onClick={() => setSelectedLesson(null)}
                >
                    <div
                        className="schedule-lesson-modal__dialog"
                        role="dialog"
                        aria-modal="true"
                        aria-label={`Урок ${selectedLesson.lessonNumber}: ${selectedLesson.title}`}
                        onClick={(event) => event.stopPropagation()}
                    >
                        <MainContent
                            selectedLesson={selectedLesson}
                            onCloseLesson={() => setSelectedLesson(null)}
                        />
                    </div>
                </div>
            )}
        </>
    );
};

export default FullSchedule;
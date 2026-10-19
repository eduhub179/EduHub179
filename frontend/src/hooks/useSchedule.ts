import {useEffect, useState} from 'react';
import {fetchClassWeekSchedule, fetchSchoolClasses, fetchWeekSchedule} from '../api';
import {useAuth} from '../auth/AuthProvider';
import type {DaySchedule, ScheduleItem} from '../types/ScheduleTypes';

interface ScheduleState {
    days: DaySchedule[];
    isLoading: boolean;
    error: string | null;
    hasClass: boolean;
}

const getLocalDate = (date: Date): string => {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
};

const mapScheduleWeek = (week: Awaited<ReturnType<typeof fetchClassWeekSchedule>>): DaySchedule[] =>
    week.days.map((day) => {
        const date = new Date(`${day.date}T00:00:00`);
        const weekday = new Intl.DateTimeFormat('ru-RU', {weekday: 'long'}).format(date);
        const shortWeekday = new Intl.DateTimeFormat('ru-RU', {weekday: 'short'}).format(date);

        return {
            name: weekday.charAt(0).toLocaleUpperCase('ru-RU') + weekday.slice(1),
            shortName: shortWeekday.charAt(0).toLocaleUpperCase('ru-RU') + shortWeekday.slice(1),
            items: day.lessons.map((lesson, index): ScheduleItem => ({
                id: lesson.id,
                startTime: lesson.start_time,
                endTime: lesson.end_time,
                lessonNumber: index + 1,
                title: lesson.subject_name,
                teachers: lesson.teachers.map((teacher) => teacher.trim().split(/\s+/)[0]).filter(Boolean),
                cabinet: lesson.cabinet,
                homework: lesson.homework
                    ? {description: lesson.homework.text, files: []}
                    : undefined,
            })),
        };
    });

export const useSchedule = (selectedClassOverride?: string): ScheduleState => {
    const {user} = useAuth();
    const schoolClass = (selectedClassOverride ?? user?.schoolClass ?? '').trim();
    const [state, setState] = useState<ScheduleState>({
        days: [],
        isLoading: false,
        error: null,
        hasClass: false,
    });

    useEffect(() => {
        let isCurrent = true;

        if (!schoolClass) {
            setState({days: [], isLoading: false, error: null, hasClass: false});
            return () => {
                isCurrent = false;
            };
        }

        setState({days: [], isLoading: true, error: null, hasClass: true});

        const loadSchedule = async () => {
            try {
                const {classes} = await fetchSchoolClasses();
                const normalizedClass = schoolClass.toLocaleLowerCase('ru-RU');
                const selectedClass = classes.find((schoolClassDto) =>
                    schoolClassDto.id === schoolClass ||
                    schoolClassDto.name.trim().toLocaleLowerCase('ru-RU') === normalizedClass,
                );

                if (!selectedClass) {
                    if (isCurrent) {
                        setState({
                            days: [],
                            isLoading: false,
                            error: 'Выбранный класс не найден. Укажите класс из списка в профиле.',
                            hasClass: true,
                        });
                    }
                    return;
                }

                const usePersonalStudentSchedule =
                    !selectedClassOverride && user?.role === 'student' && Boolean(user.id);
                const week = usePersonalStudentSchedule
                    ? await fetchWeekSchedule(getLocalDate(new Date()), user!.id!)
                    : await fetchClassWeekSchedule(getLocalDate(new Date()), selectedClass.id);
                if (isCurrent) {
                    setState({days: mapScheduleWeek(week), isLoading: false, error: null, hasClass: true});
                }
            } catch (error) {
                if (isCurrent) {
                    setState({
                        days: [],
                        isLoading: false,
                        error: error instanceof Error ? error.message : 'Не удалось загрузить расписание.',
                        hasClass: true,
                    });
                }
            }
        };

        void loadSchedule();

        return () => {
            isCurrent = false;
        };
    }, [schoolClass, user?.id, user?.role]);

    return state;
};

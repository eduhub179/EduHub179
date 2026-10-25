import {useEffect, useRef, useState} from 'react';
import {
    fetchClassWeekSchedule,
    fetchSchoolClasses,
    fetchTeacherWeekSchedule,
    fetchWeekSchedule,
    formatClassName,
    hasTeacherRole,
    hasStudentRole,
    homeworkFileUrl,
} from '../api';
import {useAuth} from '../auth/AuthProvider';
import type {DaySchedule, ScheduleItem} from '../types/ScheduleTypes';
import type {WeeklyScheduleDto} from '../api/scheduleApi';

/** Dispatch on `window` after changing lessons (e.g. attaching a file): every schedule reloads. */
export const SCHEDULE_CHANGED_EVENT = 'eduhub:schedule-changed';

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

type ApiLesson = WeeklyScheduleDto['days'][number]['lessons'][number];

/**
 * Hides a cancelled lesson when a lesson that takes place overlaps it in time
 * (something was put in its place), unless it still has homework to show.
 */
const withoutReplacedLessons = (lessons: ApiLesson[]): ApiLesson[] =>
    lessons.filter(
        (lesson) =>
            lesson.status !== 'cancelled' ||
            lesson.homework ||
            !lessons.some(
                (other) =>
                    other.status !== 'cancelled' &&
                    other.start_time < lesson.end_time &&
                    lesson.start_time < other.end_time,
            ),
    );

const mapScheduleWeek = (week: WeeklyScheduleDto, includeTargetName = false): DaySchedule[] =>
    week.days.map((day) => {
        const date = new Date(`${day.date}T00:00:00`);
        const weekday = new Intl.DateTimeFormat('ru-RU', {weekday: 'long'}).format(date);
        const shortWeekday = new Intl.DateTimeFormat('ru-RU', {weekday: 'short'}).format(date);

        return {
            name: weekday.charAt(0).toLocaleUpperCase('ru-RU') + weekday.slice(1),
            shortName: shortWeekday.charAt(0).toLocaleUpperCase('ru-RU') + shortWeekday.slice(1),
            items: withoutReplacedLessons(day.lessons).map((lesson, index): ScheduleItem => ({
                id: lesson.id,
                date: day.date,
                startTime: lesson.start_time,
                endTime: lesson.end_time,
                lessonNumber: index + 1,
                title: lesson.subject_name,
                teachers: lesson.teachers.map((teacher) => teacher.trim().split(/\s+/)[0]).filter(Boolean),
                targetName: includeTargetName ? formatClassName(lesson.target_name) : undefined,
                cabinet: lesson.cabinet,
                cancelled: lesson.status === 'cancelled',
                note: lesson.note ?? null,
                substituted: Boolean(lesson.usual_teachers?.length),
                extra: Boolean(lesson.is_extra),
                homework: lesson.homework
                    ? {
                        id: lesson.homework.id,
                        description: lesson.homework.text,
                        files: lesson.homework.files.map((file) => ({
                            id: file.id,
                            name: file.file_name,
                            mimeType: file.mime_type,
                            sizeBytes: file.size_bytes,
                            url: homeworkFileUrl(lesson.homework!.id, file.id),
                        })),
                    }
                    : undefined,
            })),
        };
    });

export const useSchedule = (
    selectedClassOverride?: string,
    source: 'class' | 'teacher' = 'class',
): ScheduleState => {
    const {user} = useAuth();
    const schoolClass = (selectedClassOverride ?? user?.schoolClass ?? '').trim();
    const [state, setState] = useState<ScheduleState>({
        days: [],
        isLoading: false,
        error: null,
        hasClass: false,
    });
    const [reloadTick, setReloadTick] = useState(0);
    // Which class/user the shown days belong to: a reload of the same schedule keeps them
    // on screen instead of flashing "loading".
    const loadedKey = useRef<string | null>(null);

    useEffect(() => {
        const reload = () => setReloadTick((tick) => tick + 1);
        window.addEventListener(SCHEDULE_CHANGED_EVENT, reload);
        return () => window.removeEventListener(SCHEDULE_CHANGED_EVENT, reload);
    }, []);

    useEffect(() => {
        let isCurrent = true;

        if (source === 'teacher' && hasTeacherRole(user)) {
            if (!user?.id) {
                loadedKey.current = null;
                setState({
                    days: [],
                    isLoading: false,
                    error: 'Войдите в аккаунт, чтобы увидеть расписание.',
                    hasClass: false,
                });
                return () => {
                    isCurrent = false;
                };
            }

            const teacherId = user.id;
            const key = `teacher|${teacherId}`;
            if (loadedKey.current !== key) {
                loadedKey.current = key;
                setState({days: [], isLoading: true, error: null, hasClass: true});
            }

            const loadTeacherSchedule = async () => {
                try {
                    const week = await fetchTeacherWeekSchedule(getLocalDate(new Date()), teacherId);
                    if (isCurrent) {
                        setState({
                            days: mapScheduleWeek(week, true),
                            isLoading: false,
                            error: null,
                            hasClass: true,
                        });
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

            void loadTeacherSchedule();
            return () => {
                isCurrent = false;
            };
        }

        if (!schoolClass) {
            loadedKey.current = null;
            setState({days: [], isLoading: false, error: null, hasClass: false});
            return () => {
                isCurrent = false;
            };
        }

        const key = `${schoolClass}|${user?.id ?? ''}|${user?.role ?? ''}`;
        if (loadedKey.current !== key) {
            loadedKey.current = key;
            setState({days: [], isLoading: true, error: null, hasClass: true});
        }

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
                    !selectedClassOverride && hasStudentRole(user) && Boolean(user?.id);
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
    }, [schoolClass, source, user, reloadTick]);

    return state;
};

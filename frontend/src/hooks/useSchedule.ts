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
    ApiError,
    errorText,
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

export const getLocalDate = (date: Date): string => {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
};

type ApiLesson = WeeklyScheduleDto['days'][number]['lessons'][number];

/** A missing week is an empty one, not an error: weeks ahead are published on Fridays. */
const scheduleError = (error: unknown): string | null => {
    if (error instanceof ApiError && error.backendText === 'Schedule week not found') return null;
    return errorText(error, 'Не удалось загрузить расписание.');
};

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

/** A lesson from the API as the schedule screens show it. */
export const toScheduleItem = (
    lesson: ApiLesson,
    date: string,
    lessonNumber: number,
    includeTargetName = false,
): ScheduleItem => ({
    id: lesson.id,
    date,
    startTime: lesson.start_time,
    endTime: lesson.end_time,
    lessonNumber,
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
});

const mapScheduleWeek = (week: WeeklyScheduleDto, includeTargetName = false): DaySchedule[] =>
    week.days.map((day) => {
        const date = new Date(`${day.date}T00:00:00`);
        const weekday = new Intl.DateTimeFormat('ru-RU', {weekday: 'long'}).format(date);
        const shortWeekday = new Intl.DateTimeFormat('ru-RU', {weekday: 'short'}).format(date);

        return {
            name: weekday.charAt(0).toLocaleUpperCase('ru-RU') + weekday.slice(1),
            shortName: shortWeekday.charAt(0).toLocaleUpperCase('ru-RU') + shortWeekday.slice(1),
            items: withoutReplacedLessons(day.lessons).map((lesson, index) =>
                toScheduleItem(lesson, day.date, index + 1, includeTargetName)),
        };
    });

/** Monday (`YYYY-MM-DD`) of the week containing `date`. */
export const mondayOf = (date: string): string => {
    const day = new Date(`${date}T00:00:00`);
    const shift = (day.getDay() + 6) % 7;
    day.setDate(day.getDate() - shift);
    return getLocalDate(day);
};

/** `date` moved by `days` (may be negative). */
export const addDays = (date: string, days: number): string => {
    const day = new Date(`${date}T00:00:00`);
    day.setDate(day.getDate() + days);
    return getLocalDate(day);
};

/**
 * Lessons of the week containing `weekDate` (default: today).
 * `source`: the class schedule (a student's own one if no class is chosen) or the
 * logged-in teacher's.
 */
export const useSchedule = (
    selectedClassOverride?: string,
    source: 'class' | 'teacher' = 'class',
    weekDate?: string,
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
    const weekStart = mondayOf(weekDate ?? getLocalDate(new Date()));
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
            const key = `teacher|${teacherId}|${weekStart}`;
            if (loadedKey.current !== key) {
                loadedKey.current = key;
                setState({days: [], isLoading: true, error: null, hasClass: true});
            }

            const loadTeacherSchedule = async () => {
                try {
                    const week = await fetchTeacherWeekSchedule(weekStart, teacherId);
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
                            error: scheduleError(error),
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

        const key = `${schoolClass}|${user?.id ?? ''}|${user?.role ?? ''}|${weekStart}`;
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
                    ? await fetchWeekSchedule(weekStart, user!.id!)
                    : await fetchClassWeekSchedule(weekStart, selectedClass.id);
                if (isCurrent) {
                    setState({days: mapScheduleWeek(week), isLoading: false, error: null, hasClass: true});
                }
            } catch (error) {
                if (isCurrent) {
                    setState({
                        days: [],
                        isLoading: false,
                        error: scheduleError(error),
                        hasClass: true,
                    });
                }
            }
        };

        void loadSchedule();

        return () => {
            isCurrent = false;
        };
    }, [schoolClass, source, user, reloadTick, weekStart]);

    return state;
};

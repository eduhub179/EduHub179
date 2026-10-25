import React, {useEffect, useState} from 'react';
import {
    fetchAdminLessons,
    fetchAdminUsers,
    fetchCabinets,
    updateAdminLesson,
    type AdminLessonDto,
    type AdminUserDto,
    type CabinetDto,
    type LessonChangeInput,
} from '../../api';
import {SCHEDULE_CHANGED_EVENT} from '../../hooks/useSchedule';
import type {ScheduleItem} from '../../types/ScheduleTypes.ts';
import {errorText, staffOnly} from '../Admin/common';

interface LessonAdminControlsProps {
    lesson: ScheduleItem;
    onLessonUpdated?: (lesson: ScheduleItem) => void;
}

interface Draft {
    substituteId: string;
    cabinetId: string;
    note: string;
}

const draftOf = (lesson: AdminLessonDto): Draft => ({
    substituteId: lesson.substitute_teacher?.id ?? '',
    cabinetId: lesson.cabinet?.id ?? '',
    note: lesson.note ?? '',
});

/** Shared by every opened card: teachers and cabinets are loaded once per page. */
let directory: Promise<[AdminUserDto[], CabinetDto[]]> | null = null;
const loadDirectory = () => {
    directory ??= Promise.all([
        fetchAdminUsers({role: 'teacher'}),
        fetchAdminUsers({role: 'admin'}),
        fetchCabinets(),
    ]).then(([teachers, admins, cabinets]): [AdminUserDto[], CabinetDto[]] => [
        staffOnly([...teachers.users, ...admins.users]),
        cabinets,
    ]);
    directory.catch(() => {
        directory = null;
    });
    return directory;
};

/** Admins: cancel the lesson, set a substitute, a cabinet or a note right from the schedule. */
const LessonAdminControls: React.FC<LessonAdminControlsProps> = ({lesson, onLessonUpdated}) => {
    const [adminLesson, setAdminLesson] = useState<AdminLessonDto | null>(null);
    const [teachers, setTeachers] = useState<AdminUserDto[]>([]);
    const [cabinets, setCabinets] = useState<CabinetDto[]>([]);
    const [draft, setDraft] = useState<Draft | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        setAdminLesson(null);
        setDraft(null);
        setError(null);
        if (!lesson.date) return;
        let current = true;
        Promise.all([fetchAdminLessons(lesson.date), loadDirectory()])
            .then(([lessons, [teacherList, cabinetList]]) => {
                if (!current) return;
                setAdminLesson(lessons.find((item) => item.id === String(lesson.id)) ?? null);
                setTeachers(teacherList);
                setCabinets(cabinetList);
            })
            .catch((loadError) => current && setError(errorText(loadError)));
        return () => {
            current = false;
        };
    }, [lesson.id, lesson.date]);

    if (!adminLesson) {
        return error ? <p className="homework-view__upload-error">{error}</p> : null;
    }

    const apply = async (change: LessonChangeInput) => {
        setBusy(true);
        setError(null);
        try {
            const updated = await updateAdminLesson(adminLesson.id, change);
            setAdminLesson(updated);
            setDraft(null);
            onLessonUpdated?.({
                ...lesson,
                cancelled: updated.status === 'cancelled',
                note: updated.note,
                cabinet: updated.cabinet ? String(updated.cabinet.number) : null,
                // Same short form as the schedule: last names.
                teachers: (updated.substitute_teacher ? [updated.substitute_teacher] : updated.teachers)
                    .map((teacher) => teacher.name.split(/\s+/)[0]),
            });
            window.dispatchEvent(new Event(SCHEDULE_CHANGED_EVENT));
        } catch (applyError) {
            setError(errorText(applyError));
        } finally {
            setBusy(false);
        }
    };

    const save = (value: Draft) => {
        const before = draftOf(adminLesson);
        const change: LessonChangeInput = {};
        if (value.substituteId !== before.substituteId) change.substitute_teacher_id = value.substituteId || null;
        if (value.cabinetId !== before.cabinetId) change.cabinet_id = value.cabinetId || null;
        if (value.note.trim() !== before.note) change.note = value.note.trim() || null;
        if (Object.keys(change).length === 0) {
            setDraft(null);
            return;
        }
        void apply(change);
    };

    const cancelled = adminLesson.status === 'cancelled';
    return (
        <section className="lesson-admin" aria-label="Управление уроком">
            <h2>Управление уроком</h2>
            <div className="lesson-admin__summary">
                {adminLesson.substitute_teacher && <span>Замена: {adminLesson.substitute_teacher.name}</span>}
                {adminLesson.cabinet && <span>Кабинет {adminLesson.cabinet.number}</span>}
            </div>
            {draft ? (
                <div className="lesson-admin__form">
                    <label>
                        Замена
                        <select value={draft.substituteId} onChange={(event) => setDraft({...draft, substituteId: event.target.value})}>
                            <option value="">нет ({adminLesson.teachers.map((t) => t.name).join(', ') || 'учитель не указан'})</option>
                            {teachers.map((teacher) => (
                                <option key={teacher.id} value={teacher.id}>
                                    {teacher.last_name} {teacher.first_name}
                                </option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Кабинет
                        <select value={draft.cabinetId} onChange={(event) => setDraft({...draft, cabinetId: event.target.value})}>
                            <option value="">как обычно</option>
                            {cabinets.map((cabinet) => (
                                <option key={cabinet.id} value={cabinet.id}>{cabinet.number}</option>
                            ))}
                        </select>
                    </label>
                    <label className="lesson-admin__wide">
                        Заметка для учеников
                        <input
                            value={draft.note}
                            maxLength={500}
                            placeholder="например, «в актовом зале»"
                            onChange={(event) => setDraft({...draft, note: event.target.value})}
                        />
                    </label>
                    <div className="homework-view__actions lesson-admin__wide">
                        <button className="homework-view__attach" type="button" disabled={busy} onClick={() => save(draft)}>
                            Сохранить
                        </button>
                        <button className="homework-view__cancel" type="button" disabled={busy} onClick={() => setDraft(null)}>
                            Отмена
                        </button>
                    </div>
                </div>
            ) : (
                <div className="homework-view__actions">
                    <button
                        className={cancelled ? 'homework-view__cancel' : 'homework-view__delete'}
                        type="button"
                        disabled={busy}
                        onClick={() => void apply({status: cancelled ? 'scheduled' : 'cancelled'})}
                    >
                        {cancelled ? 'Вернуть урок' : 'Отменить урок'}
                    </button>
                    <button className="homework-view__cancel" type="button" disabled={busy} onClick={() => setDraft(draftOf(adminLesson))}>
                        Замена, кабинет, заметка
                    </button>
                </div>
            )}
            {error && <p className="homework-view__upload-error" role="alert">{error}</p>}
        </section>
    );
};

export default LessonAdminControls;

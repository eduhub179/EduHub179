import React, {useCallback, useEffect, useMemo, useState} from 'react';
import type {FormEvent} from 'react';
import {
    createCabinet,
    fetchAdminLessons,
    fetchAdminUsers,
    fetchCabinets,
    updateAdminLesson,
    type AdminLessonDto,
    type AdminUserDto,
    type CabinetDto,
    type LessonChangeInput,
} from '../../api';
import {errorText, formatDay, isoDate} from './common';

interface Draft {
    cancelled: boolean;
    cabinetId: string;
    substituteId: string;
    note: string;
}

const draftOf = (lesson: AdminLessonDto): Draft => ({
    cancelled: lesson.status === 'cancelled',
    cabinetId: lesson.cabinet?.id ?? '',
    substituteId: lesson.substitute_teacher?.id ?? '',
    note: lesson.note ?? '',
});

/** Only the fields the admin changed, so a save never overwrites anything else. */
const changeOf = (lesson: AdminLessonDto, draft: Draft): LessonChangeInput => {
    const before = draftOf(lesson);
    const change: LessonChangeInput = {};
    if (draft.cancelled !== before.cancelled) change.status = draft.cancelled ? 'cancelled' : 'scheduled';
    if (draft.cabinetId !== before.cabinetId) change.cabinet_id = draft.cabinetId || null;
    if (draft.substituteId !== before.substituteId) change.substitute_teacher_id = draft.substituteId || null;
    if (draft.note.trim() !== before.note) change.note = draft.note.trim() || null;
    return change;
};

/** Lessons of one day by class: cancel, move to another cabinet, set a substitute or a note. */
const LessonsTab: React.FC = () => {
    const [date, setDate] = useState(() => isoDate(new Date()));
    const [lessons, setLessons] = useState<AdminLessonDto[] | null>(null);
    const [target, setTarget] = useState('');
    const [teachers, setTeachers] = useState<AdminUserDto[]>([]);
    const [cabinets, setCabinets] = useState<CabinetDto[]>([]);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [editing, setEditing] = useState<{id: string; draft: Draft} | null>(null);
    const [saveError, setSaveError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [newCabinet, setNewCabinet] = useState({number: '', description: ''});
    const [cabinetMessage, setCabinetMessage] = useState<string | null>(null);

    useEffect(() => {
        Promise.all([
            fetchAdminUsers({role: 'teacher'}),
            fetchAdminUsers({role: 'admin'}),
            fetchCabinets(),
        ])
            .then(([teacherList, adminList, cabinetList]) => {
                setTeachers([...teacherList.users, ...adminList.users]);
                setCabinets(cabinetList);
            })
            .catch((error) => setLoadError(errorText(error)));
    }, []);

    const load = useCallback(() => {
        setLessons(null);
        setLoadError(null);
        setEditing(null);
        fetchAdminLessons(date)
            .then(setLessons)
            .catch((error) => {
                setLessons([]);
                setLoadError(
                    errorText(error) === 'Schedule week not found'
                        ? 'На эту неделю расписания нет. Его можно создать во вкладке «Недели расписания».'
                        : errorText(error),
                );
            });
    }, [date]);

    useEffect(load, [load]);

    const targets = useMemo(
        () => [...new Set((lessons ?? []).map((lesson) => lesson.target_name))].sort((a, b) => a.localeCompare(b, 'ru')),
        [lessons],
    );
    const groups = useMemo(() => {
        const shown = (lessons ?? []).filter((lesson) => !target || lesson.target_name === target);
        const byTarget = new Map<string, AdminLessonDto[]>();
        for (const lesson of shown) {
            byTarget.set(lesson.target_name, [...(byTarget.get(lesson.target_name) ?? []), lesson]);
        }
        return [...byTarget.entries()].sort(([a], [b]) => a.localeCompare(b, 'ru'));
    }, [lessons, target]);

    const save = async (lesson: AdminLessonDto, draft: Draft) => {
        const change = changeOf(lesson, draft);
        if (Object.keys(change).length === 0) {
            setEditing(null);
            return;
        }
        setBusy(true);
        setSaveError(null);
        try {
            const updated = await updateAdminLesson(lesson.id, change);
            setLessons((current) => (current ?? []).map((item) => (item.id === updated.id ? updated : item)));
            setEditing(null);
        } catch (error) {
            setSaveError(errorText(error));
        } finally {
            setBusy(false);
        }
    };

    const addCabinet = async (event: FormEvent) => {
        event.preventDefault();
        setCabinetMessage(null);
        try {
            const cabinet = await createCabinet(Number(newCabinet.number), newCabinet.description.trim() || null);
            setCabinets((current) => [...current, cabinet].sort((a, b) => a.number - b.number));
            setNewCabinet({number: '', description: ''});
            setCabinetMessage(`Кабинет ${cabinet.number} добавлен.`);
        } catch (error) {
            setCabinetMessage(errorText(error));
        }
    };

    return (
        <section>
            <div className="admin__toolbar">
                <label>
                    День{' '}
                    <input type="date" value={date} onChange={(event) => setDate(event.target.value)} />
                </label>
                <span className="admin__muted">{formatDay(date)}</span>
                <label>
                    Класс{' '}
                    <select value={target} onChange={(event) => setTarget(event.target.value)}>
                        <option value="">все</option>
                        {targets.map((name) => (
                            <option key={name} value={name}>{name}</option>
                        ))}
                    </select>
                </label>
            </div>

            {loadError && <p className="admin__error">{loadError}</p>}
            {lessons === null && <p className="admin__muted">Загрузка…</p>}
            {lessons !== null && lessons.length === 0 && !loadError && (
                <p className="admin__muted">В этот день уроков нет.</p>
            )}

            {groups.map(([name, items]) => (
                <div key={name} className="admin__card">
                    <h3>{name}</h3>
                    <table className="admin__table">
                        <tbody>
                            {items.map((lesson) => {
                                const isEditing = editing?.id === lesson.id;
                                const draft = isEditing ? editing.draft : null;
                                const setDraft = (patch: Partial<Draft>) =>
                                    setEditing({id: lesson.id, draft: {...(draft ?? draftOf(lesson)), ...patch}});
                                return (
                                    <tr key={lesson.id} className={lesson.status === 'cancelled' ? 'admin__row--cancelled' : ''}>
                                        <td className="admin__nowrap">{lesson.start_time}–{lesson.end_time}</td>
                                        <td>
                                            <strong>{lesson.subject_name}</strong>
                                            {lesson.status === 'cancelled' && <span className="admin__badge admin__badge--red">отменён</span>}
                                            <div className="admin__muted">
                                                {lesson.substitute_teacher ? (
                                                    <>
                                                        <s>{lesson.teachers.map((t) => t.name).join(', ')}</s>{' '}
                                                        → <span className="admin__badge">замена: {lesson.substitute_teacher.name}</span>
                                                    </>
                                                ) : (
                                                    lesson.teachers.map((t) => t.name).join(', ') || 'учитель не указан'
                                                )}
                                                {lesson.cabinet && <> · каб. {lesson.cabinet.number}</>}
                                            </div>
                                            {lesson.note && <div className="admin__note">{lesson.note}</div>}
                                            {draft && (
                                                <div className="admin__editor">
                                                    <label>
                                                        <input
                                                            type="checkbox"
                                                            checked={draft.cancelled}
                                                            onChange={(event) => setDraft({cancelled: event.target.checked})}
                                                        />{' '}
                                                        Урок отменён
                                                    </label>
                                                    <label>
                                                        Кабинет{' '}
                                                        <select value={draft.cabinetId} onChange={(event) => setDraft({cabinetId: event.target.value})}>
                                                            <option value="">как в шаблоне</option>
                                                            {cabinets.map((cabinet) => (
                                                                <option key={cabinet.id} value={cabinet.id}>
                                                                    {cabinet.number}{cabinet.description ? ` — ${cabinet.description}` : ''}
                                                                </option>
                                                            ))}
                                                        </select>
                                                    </label>
                                                    <label>
                                                        Замена{' '}
                                                        <select value={draft.substituteId} onChange={(event) => setDraft({substituteId: event.target.value})}>
                                                            <option value="">нет</option>
                                                            {teachers.map((teacher) => (
                                                                <option key={teacher.id} value={teacher.id}>
                                                                    {teacher.last_name} {teacher.first_name}
                                                                </option>
                                                            ))}
                                                        </select>
                                                    </label>
                                                    <label className="admin__grow">
                                                        Заметка для учеников{' '}
                                                        <input
                                                            value={draft.note}
                                                            maxLength={500}
                                                            placeholder="например, «в актовом зале»"
                                                            onChange={(event) => setDraft({note: event.target.value})}
                                                        />
                                                    </label>
                                                    {saveError && <p className="admin__error">{saveError}</p>}
                                                    <div className="admin__actions">
                                                        <button type="button" disabled={busy} onClick={() => void save(lesson, draft)}>
                                                            Сохранить
                                                        </button>
                                                        <button type="button" className="admin__secondary" onClick={() => setEditing(null)}>
                                                            Отмена
                                                        </button>
                                                    </div>
                                                </div>
                                            )}
                                        </td>
                                        <td className="admin__nowrap">
                                            {!isEditing && (
                                                <button
                                                    type="button"
                                                    className="admin__secondary"
                                                    onClick={() => {
                                                        setSaveError(null);
                                                        setEditing({id: lesson.id, draft: draftOf(lesson)});
                                                    }}
                                                >
                                                    Изменить
                                                </button>
                                            )}
                                        </td>
                                    </tr>
                                );
                            })}
                        </tbody>
                    </table>
                </div>
            ))}

            <form className="admin__card admin__inline-form" onSubmit={(event) => void addCabinet(event)}>
                <span>Нет нужного кабинета?</span>
                <input
                    type="number"
                    min={100}
                    max={999}
                    required
                    placeholder="номер"
                    value={newCabinet.number}
                    onChange={(event) => setNewCabinet({...newCabinet, number: event.target.value})}
                />
                <input
                    placeholder="описание (необязательно)"
                    value={newCabinet.description}
                    onChange={(event) => setNewCabinet({...newCabinet, description: event.target.value})}
                />
                <button type="submit">Добавить кабинет</button>
                {cabinetMessage && <span className="admin__muted">{cabinetMessage}</span>}
            </form>
        </section>
    );
};

export default LessonsTab;

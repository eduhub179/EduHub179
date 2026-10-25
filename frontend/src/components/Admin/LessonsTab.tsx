import React, {useCallback, useEffect, useMemo, useState} from 'react';
import type {FormEvent} from 'react';
import {
    addOneOffLesson,
    createCabinet,
    fetchAdminLessons,
    fetchAdminUsers,
    fetchCabinets,
    fetchClassTimetable,
    fetchSchoolClasses,
    formatClassName,
    getSchoolClassLabel,
    moveLesson,
    updateAdminLesson,
    type AdminLessonDto,
    type AdminUserDto,
    type CabinetDto,
    type TimetableCourseDto,
    type LessonChangeInput,
    type SchoolClassDto,
} from '../../api';
import {errorText, formatDay, isoDate, staffOnly} from './common';

interface Place {
    date: string;
    start: string;
    end: string;
    cabinetId: string;
}

interface PlaceFieldsProps {
    place: Place;
    cabinets: CabinetDto[];
    onChange: (place: Place) => void;
}

/** Date, time and cabinet of a lesson being added or moved. */
const PlaceFields: React.FC<PlaceFieldsProps> = ({place, cabinets, onChange}) => (
    <>
        <label>
            Дата{' '}
            <input type="date" required value={place.date} onChange={(event) => onChange({...place, date: event.target.value})} />
        </label>
        <label>
            С{' '}
            <input type="time" required value={place.start} onChange={(event) => onChange({...place, start: event.target.value})} />
        </label>
        <label>
            до{' '}
            <input type="time" required value={place.end} onChange={(event) => onChange({...place, end: event.target.value})} />
        </label>
        <label>
            Кабинет{' '}
            <select value={place.cabinetId} onChange={(event) => onChange({...place, cabinetId: event.target.value})}>
                <option value="">как обычно</option>
                {cabinets.map((cabinet) => (
                    <option key={cabinet.id} value={cabinet.id}>{cabinet.number}</option>
                ))}
            </select>
        </label>
    </>
);

const placeInput = (place: Place) => ({
    date: place.date,
    start_time: place.start,
    end_time: place.end,
    cabinet_id: place.cabinetId || null,
});

interface AddLessonFormProps {
    date: string;
    cabinets: CabinetDto[];
    onAdded: (lesson: AdminLessonDto) => void;
}

/** Adds a lesson to one date only (the regular timetable does not change). */
const AddLessonForm: React.FC<AddLessonFormProps> = ({date, cabinets, onAdded}) => {
    const [open, setOpen] = useState(false);
    const [classes, setClasses] = useState<SchoolClassDto[]>([]);
    const [classId, setClassId] = useState('');
    const [courses, setCourses] = useState<TimetableCourseDto[]>([]);
    const [courseId, setCourseId] = useState('');
    const [place, setPlace] = useState<Place>({date, start: '09:00', end: '09:45', cabinetId: ''});
    const [error, setError] = useState<string | null>(null);

    useEffect(() => setPlace((current) => ({...current, date})), [date]);

    useEffect(() => {
        if (!open || classes.length > 0) return;
        fetchSchoolClasses()
            .then(({classes: list}) => {
                const active = list.filter((item) => item.is_active);
                setClasses(active);
                setClassId(active[0]?.id ?? '');
            })
            .catch((loadError) => setError(errorText(loadError)));
    }, [open, classes.length]);

    useEffect(() => {
        if (!classId) return;
        fetchClassTimetable(classId)
            .then(({courses: list}) => {
                setCourses(list);
                setCourseId(list[0]?.id ?? '');
            })
            .catch((loadError) => setError(errorText(loadError)));
    }, [classId]);

    const submit = async (event: FormEvent) => {
        event.preventDefault();
        setError(null);
        try {
            onAdded(await addOneOffLesson(courseId, placeInput(place)));
            setOpen(false);
        } catch (addError) {
            setError(errorText(addError));
        }
    };

    if (!open) {
        return (
            <div className="admin__actions admin__spaced">
                <button type="button" className="admin__secondary" onClick={() => setOpen(true)}>
                    + Добавить урок на один день
                </button>
            </div>
        );
    }
    return (
        <form className="admin__card admin__editor" onSubmit={(event) => void submit(event)}>
            <strong className="admin__wide">Разовый урок (постоянное расписание не меняется)</strong>
            <label>
                Класс{' '}
                <select value={classId} onChange={(event) => setClassId(event.target.value)}>
                    {classes.map((item) => (
                        <option key={item.id} value={item.id}>{getSchoolClassLabel(item)}</option>
                    ))}
                </select>
            </label>
            <label>
                Предмет{' '}
                <select required value={courseId} onChange={(event) => setCourseId(event.target.value)}>
                    {courses.map((course) => (
                        <option key={course.id} value={course.id}>
                            {course.subject.name}{course.group_id ? ` (${formatClassName(course.target_name)})` : ''}
                        </option>
                    ))}
                </select>
            </label>
            <PlaceFields place={place} cabinets={cabinets} onChange={setPlace} />
            {courses.length === 0 && classId && (
                <p className="admin__muted">У класса нет предметов — добавьте их во вкладке «Расписание».</p>
            )}
            {error && <p className="admin__error">{error}</p>}
            <div className="admin__actions">
                <button type="submit" disabled={!courseId}>Добавить</button>
                <button type="button" className="admin__secondary" onClick={() => setOpen(false)}>Отмена</button>
            </div>
        </form>
    );
};

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
    const [moving, setMoving] = useState<{id: string; place: Place} | null>(null);
    const [message, setMessage] = useState<string | null>(null);

    useEffect(() => {
        Promise.all([
            fetchAdminUsers({role: 'teacher'}),
            fetchAdminUsers({role: 'admin'}),
            fetchCabinets(),
        ])
            .then(([teacherList, adminList, cabinetList]) => {
                setTeachers(staffOnly([...teacherList.users, ...adminList.users]));
                setCabinets(cabinetList);
            })
            .catch((error) => setLoadError(errorText(error)));
    }, []);

    const load = useCallback(() => {
        setLessons(null);
        setLoadError(null);
        setEditing(null);
        setMoving(null);
        fetchAdminLessons(date)
            .then(setLessons)
            .catch((error) => {
                setLessons([]);
                setLoadError(errorText(error));
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

    /** One click: cancel a lesson or bring it back, nothing else changes. */
    const toggleCancelled = async (lesson: AdminLessonDto) => {
        setBusy(true);
        setSaveError(null);
        try {
            const updated = await updateAdminLesson(lesson.id, {
                status: lesson.status === 'cancelled' ? 'scheduled' : 'cancelled',
            });
            setLessons((current) => (current ?? []).map((item) => (item.id === updated.id ? updated : item)));
        } catch (error) {
            setMessage(null);
            setLoadError(`${lesson.subject_name}: ${errorText(error)}`);
        } finally {
            setBusy(false);
        }
    };

    const move = async (lesson: AdminLessonDto, place: Place) => {
        setBusy(true);
        setSaveError(null);
        try {
            const moved = await moveLesson(lesson.id, placeInput(place));
            setMessage(
                `«${moved.subject_name}» (${formatClassName(moved.target_name)}) теперь ${formatDay(moved.date)}, ${moved.start_time}` +
                    (lesson.has_homework ? '. Домашка перенесена вместе с уроком.' : '.'),
            );
            load();
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
                            <option key={name} value={name}>{formatClassName(name)}</option>
                        ))}
                    </select>
                </label>
            </div>

            {message && <p className="admin__success">{message}</p>}
            <AddLessonForm
                date={date}
                cabinets={cabinets}
                onAdded={(lesson) => {
                    setMessage(`Добавлен урок «${lesson.subject_name}» (${formatClassName(lesson.target_name)}) ${formatDay(lesson.date)}, ${lesson.start_time}.`);
                    load();
                }}
            />
            {loadError && <p className="admin__error">{loadError}</p>}
            {lessons === null && <p className="admin__muted">Загрузка…</p>}
            {lessons !== null && lessons.length === 0 && !loadError && (
                <p className="admin__muted">В этот день уроков нет.</p>
            )}

            {groups.map(([name, items]) => (
                <div key={name} className="admin__card">
                    <h3>{formatClassName(name)}</h3>
                    <table className="admin__table">
                        <tbody>
                            {items.map((lesson) => {
                                const isEditing = editing?.id === lesson.id;
                                const draft = isEditing ? editing.draft : null;
                                const setDraft = (patch: Partial<Draft>) =>
                                    setEditing({id: lesson.id, draft: {...(draft ?? draftOf(lesson)), ...patch}});
                                return (
                                    <tr
                                        key={lesson.id}
                                        className={
                                            lesson.status === 'cancelled'
                                                ? 'admin__row--cancelled'
                                                : lesson.is_extra || lesson.substitute_teacher
                                                    ? 'admin__row--changed'
                                                    : ''
                                        }
                                    >
                                        <td className="admin__nowrap">{lesson.start_time}–{lesson.end_time}</td>
                                        <td>
                                            <strong>{lesson.subject_name}</strong>
                                            {lesson.status === 'cancelled' && <span className="admin__badge admin__badge--red">отменён</span>}
                                            {lesson.is_extra && <span className="admin__badge admin__badge--blue">новый урок</span>}
                                            <div className="admin__muted">
                                                {lesson.substitute_teacher ? (
                                                    <>
                                                        <s>{lesson.teachers.map((t) => t.name).join(', ')}</s>{' '}
                                                        → <span className="admin__badge admin__badge--blue">замена: {lesson.substitute_teacher.name}</span>
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
                                            {moving?.id === lesson.id && (
                                                <form
                                                    className="admin__editor"
                                                    onSubmit={(event) => {
                                                        event.preventDefault();
                                                        void move(lesson, moving.place);
                                                    }}
                                                >
                                                    <strong className="admin__wide">Перенести урок</strong>
                                                    <PlaceFields
                                                        place={moving.place}
                                                        cabinets={cabinets}
                                                        onChange={(place) => setMoving({id: lesson.id, place})}
                                                    />
                                                    <p className="admin__muted admin__wide">
                                                        Старый урок станет «отменён», в новое время встанет такой же урок
                                                        {lesson.has_homework ? ', домашка переедет вместе с ним' : ''}.
                                                    </p>
                                                    {saveError && <p className="admin__error">{saveError}</p>}
                                                    <div className="admin__actions">
                                                        <button type="submit" disabled={busy}>Перенести</button>
                                                        <button type="button" className="admin__secondary" onClick={() => setMoving(null)}>
                                                            Отмена
                                                        </button>
                                                    </div>
                                                </form>
                                            )}
                                        </td>
                                        <td className="admin__nowrap">
                                            {!isEditing && moving?.id !== lesson.id && (
                                                <button
                                                    type="button"
                                                    className={`admin__below ${lesson.status === 'cancelled' ? 'admin__secondary' : 'admin__danger-outline'}`}
                                                    disabled={busy}
                                                    onClick={() => void toggleCancelled(lesson)}
                                                >
                                                    {lesson.status === 'cancelled' ? 'Вернуть урок' : 'Отменить'}
                                                </button>
                                            )}
                                            {!isEditing && moving?.id !== lesson.id && (
                                                <button
                                                    type="button"
                                                    className="admin__secondary"
                                                    onClick={() => {
                                                        setSaveError(null);
                                                        setMoving(null);
                                                        setEditing({id: lesson.id, draft: draftOf(lesson)});
                                                    }}
                                                >
                                                    Изменить
                                                </button>
                                            )}
                                            {!isEditing && moving?.id !== lesson.id && lesson.status !== 'cancelled' && (
                                                <button
                                                    type="button"
                                                    className="admin__secondary admin__below"
                                                    onClick={() => {
                                                        setSaveError(null);
                                                        setEditing(null);
                                                        setMoving({
                                                            id: lesson.id,
                                                            place: {
                                                                date: lesson.date,
                                                                start: lesson.start_time,
                                                                end: lesson.end_time,
                                                                cabinetId: lesson.cabinet?.id ?? '',
                                                            },
                                                        });
                                                    }}
                                                >
                                                    Перенести
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

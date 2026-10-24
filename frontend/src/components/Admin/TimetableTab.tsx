import React, {useCallback, useEffect, useMemo, useState} from 'react';
import type {FormEvent} from 'react';
import {
    addSlot,
    createCourse,
    createSubject,
    fetchAdminUsers,
    fetchCabinets,
    fetchClassTimetable,
    fetchGroups,
    fetchSchoolClasses,
    fetchSubjects,
    formatClassName,
    getSchoolClassLabel,
    removeSlot,
    setCourseTeachers,
    updateSlot,
    type AdminUserDto,
    type CabinetDto,
    type ClassTimetableDto,
    type TimetableCourseDto,
    type DayCode,
    type NamedDto,
    type Parity,
    type SchoolClassDto,
    type SlotDto,
    type SlotInput,
    type TimetableChangeDto,
} from '../../api';
import {errorText, formatDay, isoDate, staffOnly} from './common';

const days: {code: DayCode; title: string}[] = [
    {code: 'mon', title: 'Понедельник'},
    {code: 'tue', title: 'Вторник'},
    {code: 'wed', title: 'Среда'},
    {code: 'thu', title: 'Четверг'},
    {code: 'fri', title: 'Пятница'},
    {code: 'sat', title: 'Суббота'},
];

const parityTitles: Record<Parity, string> = {
    every: 'каждую неделю',
    odd: 'нечётные недели',
    even: 'чётные недели',
};

interface SlotDraft {
    courseId: string;
    day: DayCode;
    start: string;
    end: string;
    parity: Parity;
    cabinetId: string;
}

const emptySlot = (courseId = ''): SlotDraft => ({
    courseId,
    day: 'mon',
    start: '09:00',
    end: '09:45',
    parity: 'every',
    cabinetId: '',
});

const draftOf = (slot: SlotDto): SlotDraft => ({
    courseId: slot.course_id,
    day: slot.day,
    start: slot.start_time,
    end: slot.end_time,
    parity: slot.parity,
    cabinetId: slot.cabinet_id ?? '',
});

const courseTitle = (course: TimetableCourseDto, classTarget: string): string =>
    course.target_name === classTarget
        ? course.subject.name
        : `${course.subject.name} (${formatClassName(course.target_name)})`;

const teacherNames = (teachers: {name: string}[]): string =>
    teachers.map((teacher) => teacher.name).join(', ') || 'учитель не назначен';

/** What happened to already created weeks, in words. */
const changeText = (change: TimetableChangeDto, applyFrom: string | null): string => {
    if (!applyFrom) return 'Сохранено. Уже созданные недели не менялись: изменение попадёт в новые недели «из шаблонов».';
    const parts = [`Сохранено. В уже созданных неделях с ${formatDay(applyFrom)} изменено уроков: ${change.changed}.`];
    if (change.kept > 0) {
        parts.push(`Оставлено как было (есть домашка): ${change.kept}.`);
    }
    return parts.join(' ');
};

interface SlotFormProps {
    draft: SlotDraft;
    courses: TimetableCourseDto[];
    cabinets: CabinetDto[];
    classTarget: string;
    onChange: (draft: SlotDraft) => void;
}

const SlotForm: React.FC<SlotFormProps> = ({draft, courses, cabinets, classTarget, onChange}) => (
    <>
        <label>
            Предмет
            <select required value={draft.courseId} onChange={(event) => onChange({...draft, courseId: event.target.value})}>
                <option value="" disabled>выберите</option>
                {courses.map((course) => (
                    <option key={course.id} value={course.id}>{courseTitle(course, classTarget)}</option>
                ))}
            </select>
        </label>
        <label>
            День
            <select value={draft.day} onChange={(event) => onChange({...draft, day: event.target.value as DayCode})}>
                {days.map((day) => (
                    <option key={day.code} value={day.code}>{day.title}</option>
                ))}
            </select>
        </label>
        <label>
            Начало
            <input type="time" required value={draft.start} onChange={(event) => onChange({...draft, start: event.target.value})} />
        </label>
        <label>
            Конец
            <input type="time" required value={draft.end} onChange={(event) => onChange({...draft, end: event.target.value})} />
        </label>
        <label>
            Недели
            <select value={draft.parity} onChange={(event) => onChange({...draft, parity: event.target.value as Parity})}>
                {Object.entries(parityTitles).map(([code, title]) => (
                    <option key={code} value={code}>{title}</option>
                ))}
            </select>
        </label>
        <label>
            Кабинет
            <select value={draft.cabinetId} onChange={(event) => onChange({...draft, cabinetId: event.target.value})}>
                <option value="">не указан</option>
                {cabinets.map((cabinet) => (
                    <option key={cabinet.id} value={cabinet.id}>{cabinet.number}</option>
                ))}
            </select>
        </label>
    </>
);

interface CourseRowProps {
    course: TimetableCourseDto;
    classTarget: string;
    teachers: AdminUserDto[];
    onSaved: (course: TimetableCourseDto) => void;
}

/** A course with its teachers; teachers are added from a list and removed with ×. */
const CourseRow: React.FC<CourseRowProps> = ({course, classTarget, teachers, onSaved}) => {
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);

    const save = async (teacherIds: string[]) => {
        setBusy(true);
        setError(null);
        try {
            onSaved(await setCourseTeachers(course.id, teacherIds));
        } catch (saveError) {
            setError(errorText(saveError));
        } finally {
            setBusy(false);
        }
    };

    const current = course.teachers.map((teacher) => teacher.id);
    return (
        <tr>
            <td>
                <strong>{courseTitle(course, classTarget)}</strong>
            </td>
            <td>
                <div className="admin__chips">
                    {course.teachers.map((teacher) => (
                        <span key={teacher.id} className="admin__chip">
                            {teacher.name}
                            <button
                                type="button"
                                aria-label={`Убрать ${teacher.name}`}
                                disabled={busy}
                                onClick={() => void save(current.filter((id) => id !== teacher.id))}
                            >
                                ×
                            </button>
                        </span>
                    ))}
                    <select
                        value=""
                        disabled={busy}
                        onChange={(event) => event.target.value && void save([...current, event.target.value])}
                    >
                        <option value="">+ учитель</option>
                        {teachers
                            .filter((teacher) => !current.includes(teacher.id))
                            .map((teacher) => (
                                <option key={teacher.id} value={teacher.id}>
                                    {teacher.last_name} {teacher.first_name}
                                </option>
                            ))}
                    </select>
                </div>
                {error && <p className="admin__error">{error}</p>}
            </td>
        </tr>
    );
};

/**
 * The regular timetable of a class (slots by day), its courses and their teachers.
 * Changes go to future weeks; with «менять уже созданные недели» they also go to
 * created weeks from the chosen date (lessons with homework keep their subject).
 */
const TimetableTab: React.FC = () => {
    const [classes, setClasses] = useState<SchoolClassDto[]>([]);
    const [classId, setClassId] = useState('');
    const [timetable, setTimetable] = useState<ClassTimetableDto | null>(null);
    const [subjects, setSubjects] = useState<NamedDto[]>([]);
    const [groups, setGroups] = useState<NamedDto[]>([]);
    const [teachers, setTeachers] = useState<AdminUserDto[]>([]);
    const [cabinets, setCabinets] = useState<CabinetDto[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [message, setMessage] = useState<string | null>(null);
    const [applyToCreated, setApplyToCreated] = useState(true);
    const [applyFrom, setApplyFrom] = useState(() => isoDate(new Date()));
    const [editing, setEditing] = useState<{id: string; draft: SlotDraft} | null>(null);
    const [newSlot, setNewSlot] = useState<SlotDraft | null>(null);
    const [newCourse, setNewCourse] = useState({target: 'class', groupId: '', subjectId: '', teacherId: ''});
    const [newSubject, setNewSubject] = useState('');
    const [busy, setBusy] = useState(false);

    useEffect(() => {
        Promise.all([
            fetchSchoolClasses(),
            fetchSubjects(),
            fetchGroups(),
            fetchAdminUsers({role: 'teacher'}),
            fetchAdminUsers({role: 'admin'}),
            fetchCabinets(),
        ])
            .then(([classList, subjectList, groupList, teacherList, adminList, cabinetList]) => {
                const active = classList.classes.filter((item) => item.is_active);
                setClasses(active);
                setClassId((current) => current || active[0]?.id || '');
                setSubjects(subjectList);
                setGroups(groupList);
                setTeachers(staffOnly([...teacherList.users, ...adminList.users]));
                setCabinets(cabinetList);
            })
            .catch((loadError) => setError(errorText(loadError)));
    }, []);

    const load = useCallback(() => {
        if (!classId) return;
        setTimetable(null);
        setEditing(null);
        fetchClassTimetable(classId)
            .then(setTimetable)
            .catch((loadError) => setError(errorText(loadError)));
    }, [classId]);

    useEffect(load, [load]);

    const classTarget = classes.find((item) => item.id === classId)?.name ?? '';
    const courses = timetable?.courses ?? [];
    const byDay = useMemo(() => {
        const map = new Map<DayCode, SlotDto[]>(days.map((day) => [day.code, []]));
        for (const slot of timetable?.slots ?? []) {
            map.get(slot.day)?.push(slot);
        }
        return map;
    }, [timetable]);

    const effectiveApplyFrom = applyToCreated ? applyFrom : null;

    const inputOf = (draft: SlotDraft): SlotInput => ({
        course_id: draft.courseId,
        day: draft.day,
        start_time: draft.start,
        end_time: draft.end,
        parity: draft.parity,
        cabinet_id: draft.cabinetId || null,
        apply_from: effectiveApplyFrom,
    });

    const run = async (action: () => Promise<TimetableChangeDto>) => {
        setBusy(true);
        setError(null);
        setMessage(null);
        try {
            const change = await action();
            setMessage(changeText(change, effectiveApplyFrom));
            setEditing(null);
            setNewSlot(null);
            load();
        } catch (actionError) {
            setError(errorText(actionError));
        } finally {
            setBusy(false);
        }
    };

    const remove = (slot: SlotDto) => {
        const where = effectiveApplyFrom
            ? `Уроки в уже созданных неделях с ${formatDay(effectiveApplyFrom)} удалятся (с домашкой — станут «отменён»).`
            : 'Уже созданные недели не изменятся.';
        if (window.confirm(`Убрать «${slot.subject_name}» (${slot.start_time}) из расписания? ${where}`)) {
            void run(() => removeSlot(slot.id, effectiveApplyFrom));
        }
    };

    const addCourse = async (event: FormEvent) => {
        event.preventDefault();
        setError(null);
        setMessage(null);
        try {
            const course = await createCourse({
                class_id: newCourse.target === 'class' ? classId : null,
                group_id: newCourse.target === 'group' ? newCourse.groupId : null,
                subject_id: newCourse.subjectId,
                teacher_ids: newCourse.teacherId ? [newCourse.teacherId] : [],
            });
            setMessage(`Предмет «${courseTitle(course, classTarget)}» добавлен. Теперь его можно поставить в расписание.`);
            setNewCourse({target: 'class', groupId: '', subjectId: '', teacherId: ''});
            load();
        } catch (courseError) {
            setError(errorText(courseError));
        }
    };

    const addSubject = async () => {
        setError(null);
        try {
            const subject = await createSubject(newSubject);
            setSubjects((current) => [...current, subject].sort((a, b) => a.name.localeCompare(b.name, 'ru')));
            setNewCourse((current) => ({...current, subjectId: subject.id}));
            setNewSubject('');
        } catch (subjectError) {
            setError(errorText(subjectError));
        }
    };

    const updateCourse = (updated: TimetableCourseDto) =>
        setTimetable((current) =>
            current && {
                ...current,
                courses: current.courses.map((course) => (course.id === updated.id ? updated : course)),
                slots: current.slots.map((slot) =>
                    slot.course_id === updated.id ? {...slot, teachers: updated.teachers} : slot,
                ),
            },
        );

    return (
        <section>
            <div className="admin__toolbar">
                <label>
                    Класс{' '}
                    <select value={classId} onChange={(event) => setClassId(event.target.value)}>
                        {classes.map((item) => (
                            <option key={item.id} value={item.id}>{getSchoolClassLabel(item)}</option>
                        ))}
                    </select>
                </label>
                <label>
                    <input type="checkbox" checked={applyToCreated} onChange={(event) => setApplyToCreated(event.target.checked)} />{' '}
                    менять и уже созданные недели, начиная с
                </label>
                <input
                    type="date"
                    disabled={!applyToCreated}
                    value={applyFrom}
                    onChange={(event) => setApplyFrom(event.target.value)}
                />
            </div>
            <p className="admin__muted">
                Это постоянное расписание: из него создаются новые недели. Разовые изменения (замена, отмена, перенос
                одного урока) — во вкладке «Замены и отмены».
            </p>

            {error && <p className="admin__error">{error}</p>}
            {message && <p className="admin__success">{message}</p>}
            {classes.length === 0 && !error && <p className="admin__muted">Загрузка…</p>}
            {classId && timetable === null && !error && <p className="admin__muted">Загрузка…</p>}

            {timetable && (
                <>
                    <div className="admin__week">
                        {days.map((day) => (
                            <div key={day.code} className="admin__card admin__day">
                                <h3>{day.title}</h3>
                                {(byDay.get(day.code) ?? []).length === 0 && <p className="admin__muted">уроков нет</p>}
                                {(byDay.get(day.code) ?? []).map((slot) =>
                                    editing?.id === slot.id ? (
                                        <form
                                            key={slot.id}
                                            className="admin__slot-editor"
                                            onSubmit={(event) => {
                                                event.preventDefault();
                                                void run(() => updateSlot(slot.id, inputOf(editing.draft)));
                                            }}
                                        >
                                            <SlotForm
                                                draft={editing.draft}
                                                courses={courses}
                                                cabinets={cabinets}
                                                classTarget={classTarget}
                                                onChange={(draft) => setEditing({id: slot.id, draft})}
                                            />
                                            <div className="admin__actions">
                                                <button type="submit" disabled={busy}>Сохранить</button>
                                                <button type="button" className="admin__secondary" onClick={() => setEditing(null)}>
                                                    Отмена
                                                </button>
                                            </div>
                                        </form>
                                    ) : (
                                        <div key={slot.id} className="admin__slot">
                                            <div className="admin__slot-time">{slot.start_time}–{slot.end_time}</div>
                                            <div className="admin__grow">
                                                <strong>
                                                    {slot.target_name === classTarget
                                                        ? slot.subject_name
                                                        : `${slot.subject_name} (${formatClassName(slot.target_name)})`}
                                                </strong>
                                                {slot.parity !== 'every' && (
                                                    <span className="admin__badge">{parityTitles[slot.parity]}</span>
                                                )}
                                                <div className="admin__muted">
                                                    {teacherNames(slot.teachers)}
                                                    {slot.cabinet_number !== null && <> · каб. {slot.cabinet_number}</>}
                                                </div>
                                            </div>
                                            <div className="admin__slot-actions">
                                                <button
                                                    type="button"
                                                    className="admin__secondary"
                                                    onClick={() => {
                                                        setNewSlot(null);
                                                        setEditing({id: slot.id, draft: draftOf(slot)});
                                                    }}
                                                >
                                                    Изменить
                                                </button>
                                                <button type="button" className="admin__secondary" disabled={busy} onClick={() => remove(slot)}>
                                                    Убрать
                                                </button>
                                            </div>
                                        </div>
                                    ),
                                )}
                            </div>
                        ))}
                    </div>

                    {newSlot ? (
                        <form
                            className="admin__card admin__form"
                            onSubmit={(event) => {
                                event.preventDefault();
                                void run(() => addSlot(inputOf(newSlot)));
                            }}
                        >
                            <h3>Новый урок в расписании</h3>
                            <SlotForm
                                draft={newSlot}
                                courses={courses}
                                cabinets={cabinets}
                                classTarget={classTarget}
                                onChange={setNewSlot}
                            />
                            <div className="admin__actions">
                                <button type="submit" disabled={busy || !newSlot.courseId}>Добавить</button>
                                <button type="button" className="admin__secondary" onClick={() => setNewSlot(null)}>
                                    Отмена
                                </button>
                            </div>
                        </form>
                    ) : (
                        <div className="admin__actions admin__spaced">
                            <button
                                type="button"
                                disabled={courses.length === 0}
                                onClick={() => {
                                    setEditing(null);
                                    setNewSlot(emptySlot(courses[0]?.id));
                                }}
                            >
                                + Урок в расписание
                            </button>
                            {courses.length === 0 && (
                                <span className="admin__muted">Сначала добавьте классу предмет (ниже).</span>
                            )}
                        </div>
                    )}

                    <div className="admin__card">
                        <h3>Предметы и учителя класса {classTarget}</h3>
                        <p className="admin__muted">
                            Учитель предмета — постоянный, для всех уроков. Если учитель меняется на один день, это замена
                            во вкладке «Замены и отмены».
                        </p>
                        {courses.length === 0 ? (
                            <p className="admin__muted">Предметов пока нет.</p>
                        ) : (
                            <table className="admin__table">
                                <tbody>
                                    {courses.map((course) => (
                                        <CourseRow
                                            key={course.id}
                                            course={course}
                                            classTarget={classTarget}
                                            teachers={teachers}
                                            onSaved={updateCourse}
                                        />
                                    ))}
                                </tbody>
                            </table>
                        )}
                    </div>

                    <form className="admin__card admin__form" onSubmit={(event) => void addCourse(event)}>
                        <h3>Добавить предмет</h3>
                        <label>
                            Для кого
                            <select value={newCourse.target} onChange={(event) => setNewCourse({...newCourse, target: event.target.value})}>
                                <option value="class">весь класс {classTarget}</option>
                                <option value="group">группа</option>
                            </select>
                        </label>
                        {newCourse.target === 'group' && (
                            <label>
                                Группа
                                <select required value={newCourse.groupId} onChange={(event) => setNewCourse({...newCourse, groupId: event.target.value})}>
                                    <option value="" disabled>выберите</option>
                                    {groups.map((group) => (
                                        <option key={group.id} value={group.id}>{group.name}</option>
                                    ))}
                                </select>
                            </label>
                        )}
                        <label>
                            Предмет
                            <select required value={newCourse.subjectId} onChange={(event) => setNewCourse({...newCourse, subjectId: event.target.value})}>
                                <option value="" disabled>выберите</option>
                                {subjects.map((subject) => (
                                    <option key={subject.id} value={subject.id}>{subject.name}</option>
                                ))}
                            </select>
                        </label>
                        <label>
                            Учитель
                            <select value={newCourse.teacherId} onChange={(event) => setNewCourse({...newCourse, teacherId: event.target.value})}>
                                <option value="">потом</option>
                                {teachers.map((teacher) => (
                                    <option key={teacher.id} value={teacher.id}>
                                        {teacher.last_name} {teacher.first_name}
                                    </option>
                                ))}
                            </select>
                        </label>
                        <div className="admin__actions">
                            <button type="submit">Добавить предмет</button>
                        </div>
                        <div className="admin__wide admin__inline-form">
                            <span className="admin__muted">Нет предмета в списке?</span>
                            <input placeholder="название" value={newSubject} onChange={(event) => setNewSubject(event.target.value)} />
                            <button type="button" className="admin__secondary" disabled={!newSubject.trim()} onClick={() => void addSubject()}>
                                Создать предмет
                            </button>
                        </div>
                    </form>
                </>
            )}
        </section>
    );
};

export default TimetableTab;

import React, {useEffect, useState} from 'react';
import type {SyntheticEvent} from 'react';
import {Link} from 'react-router-dom';
import {
    addPlusnikTask,
    createPlusnikSheet,
    deleteMaterial,
    fetchPlusnikLessons,
    formatClassName,
    getSchoolClassLabel,
    materialFileUrl,
    updatePlusnikSheet,
    uploadMaterialFile,
    type PlusnikSheetStatus,
    type PlusnikLessonDto,
    type PlusnikSheetDto,
    type PlusnikTaskDto,
    type SchoolClassDto,
} from '../../api';
import '../../css/MathTopicForm.css';

interface TopicDetails {
    id: string;
    name: string;
    issueDate: string;
    deadline: string | null;
    status: PlusnikSheetStatus;
    taskCount: number | null;
    tasks: PlusnikTaskDto[];
}

interface MathTopicFormProps {
    mode: 'create' | 'edit';
    classes: SchoolClassDto[];
    defaultClassId: string;
    topic?: TopicDetails;
    onClose?: () => void;
    onCreated?: () => void;
    onSaved?: (
        topicId: string,
        updates: {
            name: string;
            issueDate: string;
            deadline: string | null;
            status?: PlusnikSheetStatus;
            taskCount?: number;
            tasks?: PlusnikTaskDto[];
        },
    ) => void;
}

const isMathSubject = (subject: string) => /математ|алгебр|геометр/i.test(subject);
const cleanTopicName = (name: string) => name.replace(/^тема\s*:?\s*/i, '');
const taskNumberParts = (taskNumber: string) => {
    const match = taskNumber.match(/^(\d+)(.*)$/);
    return match ? {base: match[1], suffix: match[2]} : null;
};
const groupTaskPoints = (tasks: PlusnikTaskDto[]) => {
    const groups = new Map<string, string[]>();
    for (const {number} of tasks) {
        const parts = taskNumberParts(number);
        const base = parts?.base ?? number;
        const suffix = parts?.suffix ?? '';
        const points = groups.get(base) ?? [];
        if (suffix && !points.includes(suffix)) {
            points.push(suffix);
        }
        groups.set(base, points);
    }

    return [...groups.entries()].sort(([first], [second]) =>
        Number.isNaN(Number(first)) || Number.isNaN(Number(second))
            ? first.localeCompare(second, 'ru-RU')
            : Number(first) - Number(second),
    );
};
const existingPointGroups = (tasks: PlusnikTaskDto[]) =>
    groupTaskPoints(tasks).filter(([, points]) => points.length > 0);
const pointsDraftFromTasks = (tasks: PlusnikTaskDto[]) =>
    Object.fromEntries(existingPointGroups(tasks).map(([base, points]) => [base, points.join(', ')]));
const taskNumberBases = (tasks: PlusnikTaskDto[], fallbackCount: number | null) => {
    const bases = [...new Set(tasks.flatMap(({number}) => {
        const parts = taskNumberParts(number);
        return parts ? [parts.base] : [];
    }))];
    if (bases.length) {
        return bases.sort((first, second) => Number(first) - Number(second));
    }

    return Array.from({length: Math.max(fallbackCount ?? 1, 1)}, (_, index) => String(index + 1));
};
const localDate = () => {
    const date = new Date();
    const offset = date.getTimezoneOffset() * 60_000;
    return new Date(date.getTime() - offset).toISOString().slice(0, 10);
};
const toLocalDateTime = (dateTime: string | null) => {
    if (!dateTime) return '';
    const date = new Date(dateTime);
    if (Number.isNaN(date.getTime())) return '';
    date.setMinutes(date.getMinutes() - date.getTimezoneOffset());
    return date.toISOString().slice(0, 16);
};
const errorMessage = (error: unknown) =>
    error instanceof Error ? error.message : 'Неизвестная ошибка.';

const MathTopicForm: React.FC<MathTopicFormProps> = ({
    mode,
    classes,
    defaultClassId,
    topic,
    onClose,
    onCreated,
    onSaved,
}) => {
    const [lessons, setLessons] = useState<PlusnikLessonDto[]>([]);
    const [areLessonsLoading, setAreLessonsLoading] = useState(false);
    const [selectedClassId, setSelectedClassId] = useState(defaultClassId);
    const [selectedLessonId, setSelectedLessonId] = useState('');
    const [name, setName] = useState(topic?.name ?? '');
    const [issueDate, setIssueDate] = useState(topic?.issueDate ?? localDate());
    const [deadline, setDeadline] = useState(toLocalDateTime(topic?.deadline ?? null));
    const [status, setStatus] = useState<PlusnikSheetStatus>(topic?.status ?? 'published');
    const [taskNumbers, setTaskNumbers] = useState(['1']);
    const [editTaskCount, setEditTaskCount] = useState(topic?.taskCount ?? 1);
    const [existingPointsDraft, setExistingPointsDraft] = useState(() =>
        pointsDraftFromTasks(topic?.tasks ?? []),
    );
    const [isAddPointsOpen, setIsAddPointsOpen] = useState(false);
    const [selectedTaskNumber, setSelectedTaskNumber] = useState(
        taskNumberBases(topic?.tasks ?? [], topic?.taskCount ?? 1)[0] ?? '1',
    );
    const [pointsInput, setPointsInput] = useState('');
    const [pendingTaskNumbers, setPendingTaskNumbers] = useState<string[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [createdTopic, setCreatedTopic] = useState<{sheetId: string; materialId: string | null} | null>(null);
    const [isSaving, setIsSaving] = useState(false);

    useEffect(() => {
        if (mode !== 'create' || !selectedClassId) {
            setLessons([]);
            setSelectedLessonId('');
            setAreLessonsLoading(false);
            return;
        }

        let isCurrent = true;
        setAreLessonsLoading(true);
        setError(null);
        setLessons([]);
        setSelectedLessonId('');
        fetchPlusnikLessons(selectedClassId)
            .then((availableLessons) => {
                if (!isCurrent) return;
                const mathLessons = availableLessons.filter((lesson) => isMathSubject(lesson.subject_name));
                setLessons(mathLessons);
                setSelectedLessonId(mathLessons[0]?.lesson_id ?? '');
            })
            .catch((loadError: unknown) => {
                if (isCurrent) {
                    setError(loadError instanceof Error
                        ? loadError.message
                        : 'Не удалось загрузить курсы выбранного класса.');
                }
            })
            .finally(() => {
                if (isCurrent) setAreLessonsLoading(false);
            });

        return () => {
            isCurrent = false;
        };
    }, [mode, selectedClassId]);

    useEffect(() => {
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape' && !isSaving) onClose?.();
        };
        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [isSaving, onClose]);

    const resizeTasks = (count: number) => {
        const safeCount = Math.min(100, Math.max(1, count || 1));
        setTaskNumbers((current) => Array.from(
            {length: safeCount},
            (_, index) => current[index] ?? String(index + 1),
        ));
        setPendingTaskNumbers((current) => current.filter((number) => {
            const parts = taskNumberParts(number);
            return !parts || Number(parts.base) <= safeCount;
        }));
        if (Number(selectedTaskNumber) > safeCount) {
            setSelectedTaskNumber(String(safeCount));
        }
    };

    const submit = async (event: SyntheticEvent<HTMLFormElement>) => {
        event.preventDefault();
        const formElement = event.currentTarget;
        if (!name.trim()) {
            setError('Укажите название темы.');
            return;
        }
        if (mode === 'edit') {
            for (const [base, draft] of Object.entries(existingPointsDraft)) {
                const points = draft
                    .split(/[\s,;]+/)
                    .map((point) => point.trim().toLocaleLowerCase('ru-RU'))
                    .filter(Boolean);
                if (points.some((point) => !/^[а-яё]+$/i.test(point))) {
                    setError(`Пункты задачи ${base} должны состоять только из русских букв.`);
                    return;
                }
                if (new Set(points).size !== points.length) {
                    setError(`В пунктах задачи ${base} есть повторы.`);
                    return;
                }
            }

            const hasPointEdits = existingPointGroups(topic?.tasks ?? []).some(([base, points]) => {
                const original = points.join(', ').toLocaleLowerCase('ru-RU');
                const edited = (existingPointsDraft[base] ?? '')
                    .split(/[\s,;]+/)
                    .map((point) => point.trim().toLocaleLowerCase('ru-RU'))
                    .filter(Boolean)
                    .join(', ');
                return original !== edited;
            });
            if (hasPointEdits) {
                setError(
                    'Подпункты изменены в форме, но для их сохранения нужен API-метод изменения пункта. '
                    + 'Остальные изменения не сохранены.',
                );
                return;
            }
        }

        setIsSaving(true);
        setError(null);
        if (mode === 'edit') {
            if (!topic) {
                setError('Не удалось определить тему для редактирования.');
                setIsSaving(false);
                return;
            }
            try {
                for (const taskNumber of pendingTaskNumbers) {
                    await addPlusnikTask(topic.id, taskNumber);
                    setPendingTaskNumbers((current) => current.filter((pending) => pending !== taskNumber));
                }
                const updatedSheet = await updatePlusnikSheet(topic.id, {
                    name: name.trim(),
                    issue_date: issueDate,
                    deadline: deadline ? new Date(deadline).toISOString() : null,
                    status,
                });
                onSaved?.(topic.id, {
                    name: cleanTopicName(updatedSheet.name),
                    issueDate: updatedSheet.issue_date,
                    deadline: updatedSheet.deadline,
                    status: updatedSheet.status,
                    tasks: updatedSheet.tasks,
                    taskCount: updatedSheet.tasks.length,
                });
            } catch (saveError: unknown) {
                setError(errorMessage(saveError));
            } finally {
                setIsSaving(false);
            }
            return;
        }

        const lesson = lessons.find(({lesson_id}) => lesson_id === selectedLessonId);
        if (!selectedClassId) {
            setError('Выберите класс.');
            setIsSaving(false);
            return;
        }
        if (!lesson) {
            setError('Выберите математический курс.');
            setIsSaving(false);
            return;
        }

        const fileInput = formElement.elements.namedItem('topic-file');
        const file = fileInput instanceof HTMLInputElement ? fileInput.files?.[0] : undefined;
        const createTaskNumbers = taskNumbers.flatMap((base) => {
            const points = pendingTaskNumbers.filter((number) => taskNumberParts(number)?.base === base);
            return points.length ? points : [base];
        });
        setCreatedTopic(null);
        let materialId: string | null = null;
        try {
            if (file) {
                const material = await uploadMaterialFile(lesson.lesson_id, file, {
                    title: `Задание: ${name.trim()}`,
                    description: `Файл к теме «${name.trim()}».`,
                });
                materialId = material.id;
            }
            try {
                const sheet: PlusnikSheetDto = await createPlusnikSheet(lesson.lesson_id, {
                    name: name.trim(),
                    issue_date: issueDate || localDate(),
                    deadline: deadline ? new Date(deadline).toISOString() : null,
                    status: 'published',
                    tasks: createTaskNumbers,
                });
                setCreatedTopic({sheetId: sheet.id, materialId});
                setName('');
                setDeadline('');
                setIssueDate(localDate());
                setTaskNumbers(['1']);
                setPendingTaskNumbers([]);
                setEditTaskCount(1);
                formElement.reset();
                onCreated?.();
            } catch (sheetError: unknown) {
                if (materialId) {
                    try {
                        await deleteMaterial(materialId);
                    } catch (cleanupError: unknown) {
                        setError(
                            `Не удалось создать тему: ${errorMessage(sheetError)} Файл не удалось удалить из материалов курса: ${errorMessage(cleanupError)}`,
                        );
                        return;
                    }
                }
                setError(
                    `Не удалось создать тему: ${errorMessage(sheetError)}${materialId ? ' Загруженный файл удален.' : ''}`,
                );
            }
        } catch (submitError: unknown) {
            setError(submitError instanceof Error ? submitError.message : 'Не удалось создать тему.');
        } finally {
            setIsSaving(false);
        }
    };

    const addTaskPointsToDraft = () => {
        const points = pointsInput
            .split(/[\s,;]+/)
            .map((point) => point.trim().toLocaleLowerCase('ru-RU'))
            .filter(Boolean);
        if (points.length === 0) {
            setError('Введите буквы пунктов через пробел, запятую или точку с запятой.');
            return;
        }
        if (points.some((point) => !/^[а-яё]+$/i.test(point))) {
            setError('Пункты должны состоять только из русских букв.');
            return;
        }
        if (new Set(points).size !== points.length) {
            setError('В списке есть повторяющиеся пункты.');
            return;
        }

        const nextNumbers = points.map((point) => `${selectedTaskNumber}${point}`);
        const existingNumbers = new Set((topic?.tasks ?? []).map(({number}) =>
            number.toLocaleLowerCase('ru-RU'),
        ));
        const stagedNumbers = new Set(pendingTaskNumbers.map((number) =>
            number.toLocaleLowerCase('ru-RU'),
        ));
        const duplicates = nextNumbers.filter((number) =>
            existingNumbers.has(number.toLocaleLowerCase('ru-RU'))
            || stagedNumbers.has(number.toLocaleLowerCase('ru-RU')),
        );
        if (duplicates.length) {
            setError(`Эти пункты уже есть: ${duplicates.join(', ')}.`);
            return;
        }

        setPendingTaskNumbers((current) => [...current, ...nextNumbers]);
        setPointsInput('');
        setIsAddPointsOpen(false);
        setError(null);
    };

    const form = (
        <form className="math-topic-form" onSubmit={submit}>
            <label>
                Название темы
                <input
                    autoFocus={mode === 'edit'}
                    maxLength={255}
                    required
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                />
            </label>
            {mode === 'create' && (
                <>
                    <label>
                        Класс
                        <select
                            required
                            value={selectedClassId}
                            onChange={(event) => {
                                setSelectedClassId(event.target.value);
                                setError(null);
                            }}
                        >
                            <option value="">Выберите класс</option>
                            {classes.map((schoolClass) => (
                                <option key={schoolClass.id} value={schoolClass.id}>
                                    {getSchoolClassLabel(schoolClass)}
                                </option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Математический курс
                        <select
                            value={selectedLessonId}
                            onChange={(event) => setSelectedLessonId(event.target.value)}
                            disabled={!selectedClassId || lessons.length === 0}
                        >
                            <option value="">Выберите курс</option>
                            {lessons.map((lesson) => (
                                <option key={lesson.lesson_id} value={lesson.lesson_id}>
                                    {lesson.subject_name} — {formatClassName(lesson.target_name)}
                                </option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Файл темы
                        <input name="topic-file" type="file"/>
                    </label>
                </>
            )}
            <label>
                Дата выдачи
                <input
                    required
                    type="date"
                    value={issueDate}
                    onChange={(event) => setIssueDate(event.target.value)}
                />
            </label>
            <label>
                Дедлайн
                <input
                    type="datetime-local"
                    value={deadline}
                    onChange={(event) => setDeadline(event.target.value)}
                />
            </label>
            {mode === 'create' && (
                <label>
                    Количество задач
                    <input
                        required
                        type="number"
                        min={1}
                        max={100}
                        value={taskNumbers.length}
                        onChange={(event) => resizeTasks(Number(event.target.value))}
                    />
                </label>
            )}
            {mode === 'edit' && (
                <>
                    <label>
                        Количество задач
                        <input
                            type="number"
                            min={1}
                            max={100}
                            value={editTaskCount}
                            onChange={(event) => {
                                const count = Number(event.target.value);
                                setEditTaskCount(Math.min(100, Math.max(1, count || 1)));
                            }}
                        />
                    </label>
                    <p className="math-topic-form__placeholder">
                        Изменение количества задач пока не подключено и не будет сохранено.
                    </p>
                    {existingPointGroups(topic?.tasks ?? []).length > 0 && (
                        <div className="math-topic-form__existing-points" aria-label="Существующие пункты задач">
                            <strong>Существующие пункты:</strong>
                            {existingPointGroups(topic?.tasks ?? []).map(([base]) => (
                                <label key={base}>
                                    Задача {base}
                                    <input
                                        aria-label={`Пункты задачи ${base}`}
                                        value={existingPointsDraft[base] ?? ''}
                                        onChange={(event) => {
                                            setExistingPointsDraft((current) => ({
                                                ...current,
                                                [base]: event.target.value,
                                            }));
                                            setError(null);
                                        }}
                                    />
                                </label>
                            ))}
                            <span>
                                Изменения пунктов пока нельзя сохранить: отсутствует API-метод изменения пункта.
                            </span>
                        </div>
                    )}
                </>
            )}
            <div className="math-topic-form__points">
                <button
                    aria-expanded={isAddPointsOpen}
                    className="math-topic-form__add-points"
                    onClick={() => {
                        setIsAddPointsOpen((open) => !open);
                        setError(null);
                    }}
                    type="button"
                >
                    +
                </button>
                <span>Добавить пункты к задаче</span>
            </div>
            {isAddPointsOpen && (
                <div className="math-topic-form__points-editor">
                    <label>
                        Номер задачи
                        <select
                            value={selectedTaskNumber}
                            onChange={(event) => setSelectedTaskNumber(event.target.value)}
                        >
                            {(mode === 'create'
                                ? taskNumbers
                                : taskNumberBases(topic?.tasks ?? [], topic?.taskCount ?? 1)
                            ).map((taskNumber) => (
                                <option key={taskNumber} value={taskNumber}>{taskNumber}</option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Буквы пунктов
                        <input
                            placeholder="а, б; в"
                            value={pointsInput}
                            onChange={(event) => setPointsInput(event.target.value)}
                        />
                    </label>
                    <p>Разделяйте буквы пробелом, запятой или точкой с запятой.</p>
                    <button onClick={addTaskPointsToDraft} type="button">
                        Добавить
                    </button>
                </div>
            )}
            {pendingTaskNumbers.length > 0 && (
                <p className="math-topic-form__pending-points" role="status">
                    Будут добавлены при сохранении: {pendingTaskNumbers.join(', ')}
                </p>
            )}
            {error && <p className="math-topics__error" role="alert">{error}</p>}
            {mode === 'create' && createdTopic && (
                <p className="math-topic-form__success" role="status">
                    Тема добавлена{createdTopic.materialId ? ', файл загружен в материалы курса.' : '.'}
                    {' '}<Link to={`/plusnik/${createdTopic.sheetId}`}>Открыть тему</Link>
                    {createdTopic.materialId && (
                        <> · <a href={materialFileUrl(createdTopic.materialId)}>Открыть файл</a></>
                    )}
                </p>
            )}
            {mode === 'create' && selectedClassId && !areLessonsLoading && lessons.length === 0 && (
                <p className="math-topics__error" role="alert">
                    Для выбранного класса нет доступных математических курсов.
                </p>
            )}
            {mode === 'edit' && (
                <div aria-label="Видимость темы для учеников" className="math-topic-modal__visibility">
                    <button
                        aria-pressed={status === 'published'}
                        className={status === 'published' ? 'is-selected' : ''}
                        onClick={() => setStatus('published')}
                        type="button"
                    >
                        Показана
                    </button>
                    <button
                        aria-pressed={status !== 'published'}
                        className={status !== 'published' ? 'is-selected' : ''}
                        onClick={() => setStatus('draft')}
                        type="button"
                    >
                        Скрыта
                    </button>
                </div>
            )}
            <div className="math-topic-modal__actions">
                <button className="math-topic-modal__cancel" disabled={isSaving} onClick={onClose} type="button">
                    Отмена
                </button>
                <button
                    className="math-topics__create-button"
                    disabled={isSaving || (mode === 'create'
                        && (!selectedClassId || areLessonsLoading || lessons.length === 0))}
                    type="submit"
                >
                    {isSaving
                        ? mode === 'edit' ? 'Сохранение…' : 'Создание…'
                        : mode === 'edit' ? 'Сохранить' : 'Создать тему'}
                </button>
            </div>
        </form>
    );

    return (
        <div
            className="math-topic-modal__backdrop"
            onClick={(event) => {
                if (event.target === event.currentTarget && !isSaving) onClose?.();
            }}
        >
            <section aria-labelledby="math-topic-form-title" aria-modal="true" className="math-topic-modal" role="dialog">
                <div className="math-topic-modal__heading">
                    <h2 id="math-topic-form-title">
                        {mode === 'edit' ? 'Редактирование темы' : 'Добавление темы'}
                    </h2>
                    <button
                        aria-label="Закрыть окно"
                        className="math-topic-modal__close"
                        disabled={isSaving}
                        onClick={onClose}
                        type="button"
                    >
                        ×
                    </button>
                </div>
                {form}
            </section>
        </div>
    );
};

export default MathTopicForm;

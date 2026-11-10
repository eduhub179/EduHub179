import React, {useEffect, useState} from 'react';
import type {SyntheticEvent} from 'react';
import {Link} from 'react-router-dom';
import {
    addPlusnikTasks,
    createPlusnikSheet,
    deletePlusnikTasks,
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
import {errorText} from '../../api/errors';

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
const taskNumbersFromTasks = (tasks: PlusnikTaskDto[], fallbackCount: number | null) =>
    tasks.length
        ? tasks.map(({number}) => number)
        : Array.from({length: Math.max(fallbackCount ?? 1, 1)}, (_, index) => String(index + 1));
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
    errorText(error, 'Неизвестная ошибка.');

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
    const [status] = useState<PlusnikSheetStatus>(topic?.status ?? 'published');
    const [taskNumbers, setTaskNumbers] = useState(() =>
        taskNumbersFromTasks(topic?.tasks ?? [], mode === 'edit' ? topic?.taskCount ?? 1 : 1),
    );
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
                    setError(errorText(loadError, 'Не удалось загрузить курсы выбранного класса.'));
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

    const submit = async (event: SyntheticEvent<HTMLFormElement>) => {
        event.preventDefault();
        const formElement = event.currentTarget;
        if (!name.trim()) {
            setError('Укажите название темы.');
            return;
        }
        const desiredNumbers = taskNumbers;
        if (new Set(desiredNumbers).size !== desiredNumbers.length) {
            setError('Номера должны быть уникальными.');
            return;
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
                const currentTasks = topic.tasks;
                const desiredSet = new Set(desiredNumbers);
                const deleteIds = currentTasks
                    .filter(({number}) => !desiredSet.has(number))
                    .map(({id}) => id);
                const addNumbers = desiredNumbers.filter((number) =>
                    !currentTasks.some((task) => task.number === number),
                );
                let updatedTasks = currentTasks;
                if (deleteIds.length) {
                    updatedTasks = await deletePlusnikTasks(topic.id, deleteIds);
                }
                if (addNumbers.length) {
                    updatedTasks = await addPlusnikTasks(topic.id, addNumbers);
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
                    tasks: updatedSheet.tasks.length ? updatedSheet.tasks : updatedTasks,
                    taskCount: updatedSheet.tasks.length || updatedTasks.length,
                });
                onClose?.();
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
                    tasks: desiredNumbers,
                });
                setCreatedTopic({sheetId: sheet.id, materialId});
                setName('');
                setDeadline('');
                setIssueDate(localDate());
                setTaskNumbers(['1']);
                formElement.reset();
                onCreated?.();
                onClose?.();
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
            setError(errorText(submitError, 'Не удалось создать тему.'));
        } finally {
            setIsSaving(false);
        }
    };

    const resizeTaskRows = (count: number) => {
        const safeCount = Math.min(100, Math.max(1, count || 1));
        setTaskNumbers((current) => {
            if (safeCount <= current.length) {
                return current.slice(0, safeCount);
            }

            const numbers = [...current];
            let nextNumber = numbers.reduce((max, number) => Math.max(max, Number.parseInt(number, 10) || 0), 0) + 1;
            while (numbers.length < safeCount) {
                while (numbers.includes(String(nextNumber))) {
                    nextNumber += 1;
                }
                numbers.push(String(nextNumber));
                nextNumber += 1;
            }
            return numbers;
        });
        setError(null);
    };

    const form = (
        <form className="math-topic-form math-topic-form--modal" onSubmit={submit}>
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
            <div className="math-topic-form__dates">
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
            </div>
            <label>
                Количество
                <input
                    required
                    type="number"
                    min={1}
                    max={100}
                    value={taskNumbers.length}
                    onChange={(event) => resizeTaskRows(Number(event.target.value))}
                />
            </label>
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

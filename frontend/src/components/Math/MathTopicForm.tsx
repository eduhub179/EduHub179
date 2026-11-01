import React, {useEffect, useState} from 'react';
import type {SyntheticEvent} from 'react';
import {Link} from 'react-router-dom';
import {
    createPlusnikSheet,
    deleteMaterial,
    fetchPlusnikLessons,
    formatClassName,
    getSchoolClassLabel,
    materialFileUrl,
    updatePlusnikSheet,
    uploadMaterialFile,
    type PlusnikLessonDto,
    type PlusnikSheetDto,
    type SchoolClassDto,
} from '../../api';
import '../../css/MathTopicForm.css';

interface TopicDetails {
    id: string;
    name: string;
    issueDate: string;
    deadline: string | null;
}

interface MathTopicFormProps {
    mode: 'create' | 'edit';
    classes: SchoolClassDto[];
    defaultClassId: string;
    topic?: TopicDetails;
    onClose?: () => void;
    onCreated?: () => void;
    onSaved?: (topicId: string, updates: {name: string; issueDate: string; deadline: string | null}) => void;
    onRemoved?: (topicId: string) => void;
}

const isMathSubject = (subject: string) => /математ|алгебр|геометр/i.test(subject);
const cleanTopicName = (name: string) => name.replace(/^тема\s*:?\s*/i, '');
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
    onRemoved,
}) => {
    const [lessons, setLessons] = useState<PlusnikLessonDto[]>([]);
    const [areLessonsLoading, setAreLessonsLoading] = useState(false);
    const [selectedClassId, setSelectedClassId] = useState(defaultClassId);
    const [selectedLessonId, setSelectedLessonId] = useState('');
    const [name, setName] = useState(topic?.name ?? '');
    const [issueDate, setIssueDate] = useState(topic?.issueDate ?? localDate());
    const [deadline, setDeadline] = useState(toLocalDateTime(topic?.deadline ?? null));
    const [taskNumbers, setTaskNumbers] = useState(['1']);
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
    };

    const submit = async (event: SyntheticEvent<HTMLFormElement>) => {
        event.preventDefault();
        const formElement = event.currentTarget;
        if (!name.trim()) {
            setError('Укажите название темы.');
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
                const updatedSheet = await updatePlusnikSheet(topic.id, {
                    name: name.trim(),
                    issue_date: issueDate,
                    deadline: deadline ? new Date(deadline).toISOString() : null,
                });
                onSaved?.(topic.id, {
                    name: cleanTopicName(updatedSheet.name),
                    issueDate: updatedSheet.issue_date,
                    deadline: updatedSheet.deadline,
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
                    tasks: taskNumbers.map((number, index) => number.trim() || String(index + 1)),
                });
                setCreatedTopic({sheetId: sheet.id, materialId});
                setName('');
                setDeadline('');
                setIssueDate(localDate());
                setTaskNumbers(['1']);
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

    const removeFromPage = () => {
        if (!topic || !window.confirm(`Убрать тему «${topic.name}» с этой страницы? В базе данных она останется.`)) {
            return;
        }
        onRemoved?.(topic.id);
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
                <>
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
                    <fieldset className="math-topic-form__tasks">
                        <legend>Пункты задач</legend>
                        {taskNumbers.map((number, index) => (
                            <label key={index}>
                                Задача {index + 1}
                                <input
                                    maxLength={20}
                                    value={number}
                                    onChange={(event) => setTaskNumbers((current) =>
                                        current.map((task, taskIndex) =>
                                            taskIndex === index ? event.target.value : task,
                                        ),
                                    )}
                                />
                            </label>
                        ))}
                    </fieldset>
                </>
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
                <button
                    className="math-topic-modal__remove"
                    disabled={isSaving}
                    onClick={removeFromPage}
                    type="button"
                >
                    Убрать тему со страницы
                </button>
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

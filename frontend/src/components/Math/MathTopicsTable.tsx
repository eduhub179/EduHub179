import React, {useEffect, useState} from 'react';
import type {SyntheticEvent} from 'react';
import {Link} from 'react-router-dom';
import {
    createPlusnikSheet,
    deleteMaterial,
    fetchMyPlusnikProgress,
    fetchPlusnikLessons,
    fetchPlusnikSheets,
    formatClassName,
    getSchoolClassLabel,
    hasStudentRole,
    hasTeacherOrAdminRole,
    materialFileUrl,
    uploadMaterialFile,
    type PlusnikLessonDto,
    type PlusnikSheetDto,
    type SchoolClassDto,
} from '../../api';
import type {User} from '../../auth/authService';

interface MathTopicsTableProps {
    schoolClass: SchoolClassDto;
    classes: SchoolClassDto[];
    user: User | null;
}

interface MathTopic {
    id: string;
    name: string;
    subject: string;
    deadline: string | null;
    taskCount: number | null;
}

const isMathSubject = (subject: string) => /математ|алгебр|геометр/i.test(subject);
const cleanTopicName = (name: string) => name.replace(/^тема\s*:?\s*/i, '');
const formatDeadline = (deadline: string | null) => deadline
    ? new Date(deadline).toLocaleString('ru-RU')
    : '—';
const localDate = () => {
    const date = new Date();
    const offset = date.getTimezoneOffset() * 60_000;
    return new Date(date.getTime() - offset).toISOString().slice(0, 10);
};

const MathTopicsTable: React.FC<MathTopicsTableProps> = ({schoolClass, classes, user}) => {
    const [topics, setTopics] = useState<MathTopic[]>([]);
    const [formLessons, setFormLessons] = useState<PlusnikLessonDto[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [reloadKey, setReloadKey] = useState(0);
    const [isCreateFormOpen, setIsCreateFormOpen] = useState(false);
    const [isCreating, setIsCreating] = useState(false);
    const [createError, setCreateError] = useState<string | null>(null);
    const [createdTopic, setCreatedTopic] = useState<{sheetId: string; materialId: string | null} | null>(null);
    const [selectedClassId, setSelectedClassId] = useState(schoolClass.id);
    const [selectedLessonId, setSelectedLessonId] = useState('');
    const [topicName, setTopicName] = useState('');
    const [issueDate, setIssueDate] = useState(localDate);
    const [deadline, setDeadline] = useState('');
    const [taskNumbers, setTaskNumbers] = useState(['1']);

    useEffect(() => {
        setSelectedClassId(schoolClass.id);
    }, [schoolClass.id]);

    useEffect(() => {
        let isCurrent = true;
        setFormLessons([]);
        setSelectedLessonId('');
        if (!selectedClassId || !user?.id || !hasTeacherOrAdminRole(user)) {
            return () => {
                isCurrent = false;
            };
        }

        fetchPlusnikLessons(selectedClassId)
            .then((lessons) => {
                if (!isCurrent) return;
                const availableLessons = lessons.filter((lesson) => isMathSubject(lesson.subject_name));
                setFormLessons(availableLessons);
                setSelectedLessonId(availableLessons[0]?.lesson_id ?? '');
            })
            .catch((loadError: unknown) => {
                if (isCurrent) {
                    setCreateError(loadError instanceof Error
                        ? loadError.message
                        : 'Не удалось загрузить курсы выбранного класса.');
                }
            });

        return () => {
            isCurrent = false;
        };
    }, [selectedClassId, user]);

    useEffect(() => {
        let isCurrent = true;
        setIsLoading(true);
        setError(null);

        const loadTopics = async (): Promise<MathTopic[]> => {
            if (!user?.id) {
                return [];
            }

            if (hasStudentRole(user)) {
                if (user.classId !== schoolClass.id) {
                    return [];
                }

                const progress = await fetchMyPlusnikProgress();
                return progress
                    .filter(({subject_name}) => isMathSubject(subject_name))
                    .map(({sheet, subject_name, total_tasks}) => ({
                        id: sheet.id,
                        name: cleanTopicName(sheet.name),
                        subject: subject_name,
                        deadline: sheet.deadline,
                        taskCount: total_tasks,
                    }));
            }

            if (!hasTeacherOrAdminRole(user)) {
                return [];
            }

            // The backend picks the class's courses, including group courses (profile groups)
            // attended by its students; here only math subjects are kept.
            const lessons = await fetchPlusnikLessons(schoolClass.id);
            const mathLessons = lessons.filter((lesson) => isMathSubject(lesson.subject_name));
            const sheetsByLesson = await Promise.all(mathLessons.map(async (lesson) => {
                const sheets = await fetchPlusnikSheets(lesson.lesson_id);
                return sheets
                    .filter((sheet) => sheet.status !== 'archived')
                    .map((sheet) => ({
                        id: sheet.id,
                        name: cleanTopicName(sheet.name),
                        subject: lesson.subject_name,
                        deadline: sheet.deadline,
                        taskCount: sheet.tasks.length,
                    }));
            }));

            return sheetsByLesson.flat();
        };

        void loadTopics()
            .then((loadedTopics) => {
                if (isCurrent) {
                    setTopics(loadedTopics);
                }
            })
            .catch((loadError: unknown) => {
                if (isCurrent) {
                    setError(loadError instanceof Error
                        ? loadError.message
                        : 'Не удалось загрузить темы по математике.');
                }
            })
            .finally(() => {
                if (isCurrent) {
                    setIsLoading(false);
                }
            });

        return () => {
            isCurrent = false;
        };
    }, [schoolClass, user, reloadKey]);

    const resizeTasks = (count: number) => {
        const safeCount = Math.min(100, Math.max(1, count || 1));
        setTaskNumbers((current) => Array.from(
            {length: safeCount},
            (_, index) => current[index] ?? String(index + 1),
        ));
    };

    const createTopic = async (event: SyntheticEvent<HTMLFormElement>) => {
        event.preventDefault();
        const form = event.currentTarget;
        const fileInput = form.elements.namedItem('topic-file');
        const file = fileInput instanceof HTMLInputElement ? fileInput.files?.[0] : undefined;
        const lesson = formLessons.find(({lesson_id}) => lesson_id === selectedLessonId);
        if (!selectedClassId) {
            setCreateError('Выберите класс.');
            return;
        }
        if (!lesson) {
            setCreateError('Выберите математический курс.');
            return;
        }

        setIsCreating(true);
        setCreateError(null);
        setCreatedTopic(null);
        let materialId: string | null = null;
        try {
            if (file) {
                const material = await uploadMaterialFile(lesson.lesson_id, file, {
                    title: `Задание: ${topicName.trim()}`,
                    description: `Файл к теме «${topicName.trim()}».`,
                });
                materialId = material.id;
            }
            try {
                const sheet: PlusnikSheetDto = await createPlusnikSheet(lesson.lesson_id, {
                    name: topicName.trim(),
                    issue_date: issueDate || localDate(),
                    deadline: deadline ? new Date(deadline).toISOString() : null,
                    status: 'published',
                    tasks: taskNumbers.map((number, index) => number.trim() || String(index + 1)),
                });
                setCreatedTopic({sheetId: sheet.id, materialId});
                setTopicName('');
                setDeadline('');
                setIssueDate(localDate());
                setTaskNumbers(['1']);
                form.reset();
            } catch (sheetError) {
                if (materialId) {
                    try {
                        await deleteMaterial(materialId);
                    } catch (cleanupError) {
                        setCreateError(
                            `Не удалось создать тему: ${errorMessage(sheetError)} Файл не удалось удалить из материалов курса: ${errorMessage(cleanupError)}`,
                        );
                        return;
                    }
                }
                setCreateError(
                    `Не удалось создать тему: ${errorMessage(sheetError)}${materialId ? ' Загруженный файл удален.' : ''}`,
                );
                return;
            }
            setReloadKey((current) => current + 1);
        } catch (submitError) {
            setCreateError(submitError instanceof Error
                ? submitError.message
                : 'Не удалось создать тему.');
        } finally {
            setIsCreating(false);
        }
    };

    return (
        <section className="math-topics" aria-label={`Темы по математике для класса ${getSchoolClassLabel(schoolClass)}`}>
            <div className="math-topics__heading">
                <h2>Темы класса {getSchoolClassLabel(schoolClass)}</h2>
                {hasTeacherOrAdminRole(user) && (
                    <button
                        className="math-topics__create-button"
                        type="button"
                        onClick={() => {
                            setIsCreateFormOpen((open) => !open);
                            setCreateError(null);
                            setCreatedTopic(null);
                        }}
                    >
                        {isCreateFormOpen ? 'Закрыть' : 'Добавить тему'}
                    </button>
                )}
            </div>
            {isCreateFormOpen && hasTeacherOrAdminRole(user) && (
                <form className="math-topic-form" onSubmit={createTopic}>
                    <label>
                        Название темы
                        <input
                            required
                            maxLength={255}
                            value={topicName}
                            onChange={(event) => setTopicName(event.target.value)}
                        />
                    </label>
                    <label>
                        Класс
                        <select
                            required
                            value={selectedClassId}
                            onChange={(event) => {
                                setSelectedClassId(event.target.value);
                                setCreateError(null);
                            }}
                        >
                            <option value="">Выберите класс</option>
                            {classes.map((schoolClassOption) => (
                                <option key={schoolClassOption.id} value={schoolClassOption.id}>
                                    {getSchoolClassLabel(schoolClassOption)}
                                </option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Математический курс
                        <select
                            value={selectedLessonId}
                            onChange={(event) => setSelectedLessonId(event.target.value)}
                            disabled={!selectedClassId || formLessons.length === 0}
                        >
                            <option value="">Выберите курс</option>
                            {formLessons.map((lesson) => (
                                <option key={lesson.lesson_id} value={lesson.lesson_id}>
                                    {lesson.subject_name} — {formatClassName(lesson.target_name)}
                                </option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Файл темы
                        <input name="topic-file" type="file" />
                    </label>
                    <label>
                        Дата выдачи
                        <input
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
                    {createError && <p className="math-topics__error" role="alert">{createError}</p>}
                    {createdTopic && (
                        <p className="math-topic-form__success" role="status">
                            Тема добавлена{createdTopic.materialId ? ', файл загружен в материалы курса.' : '.'}
                            {' '}<Link to={`/plusnik/${createdTopic.sheetId}`}>Открыть тему</Link>
                            {createdTopic.materialId && (
                                <> · <a href={materialFileUrl(createdTopic.materialId)}>Открыть файл</a></>
                            )}
                        </p>
                    )}
                    {selectedClassId && formLessons.length === 0 && !isLoading && (
                        <p className="math-topics__error" role="alert">
                            Для выбранного класса нет доступных математических курсов.
                        </p>
                    )}
                    <button
                        className="math-topics__create-button"
                        type="submit"
                        disabled={isCreating || !selectedClassId || formLessons.length === 0}
                    >
                        {isCreating ? 'Создание…' : 'Создать тему'}
                    </button>
                </form>
            )}
            {isLoading ? (
                <p className="math-content__description">Загрузка тем...</p>
            ) : error ? (
                <p className="math-topics__error" role="alert">{error}</p>
            ) : !user?.id ? (
                <p className="math-admin-table__empty">Войдите в аккаунт, чтобы увидеть темы.</p>
            ) : hasStudentRole(user) && user.classId !== schoolClass.id ? (
                <p className="math-admin-table__empty">Темы доступны только для вашего класса.</p>
            ) : topics.length ? (
                <div className="math-admin-table-wrap">
                    <table className="math-admin-table">
                        <thead>
                        <tr>
                            <th scope="col">Тема</th>
                            <th className="math-admin-table__subject" scope="col" title="Предмет">Пр.</th>
                            <th scope="col">Дедлайн</th>
                            <th scope="col">Задач</th>
                            <th scope="col">Плюсник</th>
                        </tr>
                        </thead>
                        <tbody>
                        {topics.map((topic) => (
                            <tr key={topic.id}>
                                <th scope="row">{topic.name}</th>
                                <td
                                    className="math-admin-table__subject"
                                    aria-label={topic.subject}
                                    title={topic.subject}
                                >
                                    {topic.subject.charAt(0).toLocaleUpperCase('ru-RU')}
                                </td>
                                <td>{formatDeadline(topic.deadline)}</td>
                                <td>{topic.taskCount ?? '—'}</td>
                                <td>
                                    <Link
                                        className="math-admin-table__plusnik-link"
                                        to={`/plusnik/${topic.id}`}
                                    >
                                        Открыть
                                    </Link>
                                </td>
                            </tr>
                        ))}
                        </tbody>
                    </table>
                </div>
            ) : (
                <p className="math-admin-table__empty">Темы для этого класса пока не добавлены.</p>
            )}
        </section>
    );
};

const errorMessage = (error: unknown): string =>
    error instanceof Error ? error.message : 'Неизвестная ошибка.';

export default MathTopicsTable;

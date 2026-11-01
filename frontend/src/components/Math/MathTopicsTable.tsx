import React, {useEffect, useState} from 'react';
import {Link} from 'react-router-dom';
import {
    fetchMyPlusnikProgress,
    fetchPlusnikLessons,
    fetchPlusnikSheets,
    getSchoolClassLabel,
    hasStudentRole,
    hasTeacherOrAdminRole,
    type SchoolClassDto,
} from '../../api';
import type {User} from '../../auth/authService';
import MathTopicForm from './MathTopicForm';

interface MathTopicsTableProps {
    schoolClass: SchoolClassDto;
    classes: SchoolClassDto[];
    user: User | null;
}

interface MathTopic {
    id: string;
    name: string;
    subject: string;
    issueDate: string;
    deadline: string | null;
    taskCount: number | null;
}

const isMathSubject = (subject: string) => /математ|алгебр|геометр/i.test(subject);
const cleanTopicName = (name: string) => name.replace(/^тема\s*:?\s*/i, '');
const formatDeadline = (deadline: string | null) => deadline
    ? new Date(deadline).toLocaleDateString('ru-RU')
    : '—';
const MathTopicsTable: React.FC<MathTopicsTableProps> = ({schoolClass, classes, user}) => {
    const [topics, setTopics] = useState<MathTopic[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [reloadKey, setReloadKey] = useState(0);
    const [isCreateFormOpen, setIsCreateFormOpen] = useState(false);
    const [editingTopic, setEditingTopic] = useState<MathTopic | null>(null);

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
                        issueDate: sheet.issue_date,
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
                        issueDate: sheet.issue_date,
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

    const openTopicEditor = (topic: MathTopic) => {
        setEditingTopic(topic);
    };

    const removeTopicFromPage = (topicId: string) => {
        setTopics((currentTopics) => currentTopics.filter(({id}) => id !== topicId));
        setEditingTopic(null);
    };

    const updateTopicInList = (
        topicId: string,
        updates: {name: string; issueDate: string; deadline: string | null},
    ) => {
        setTopics((currentTopics) => currentTopics.map((topic) =>
            topic.id === topicId ? {...topic, ...updates} : topic,
        ));
        setEditingTopic(null);
    };

    return (
        <section className="math-topics" aria-label={`Темы по математике для класса ${getSchoolClassLabel(schoolClass)}`}>
            <div className="math-topics__heading">
                <h2>Темы класса {getSchoolClassLabel(schoolClass)}</h2>
                {hasTeacherOrAdminRole(user) && (
                    <button
                        className="math-topics__create-button"
                        type="button"
                        onClick={() => setIsCreateFormOpen((open) => !open)}
                    >
                        {isCreateFormOpen ? 'Закрыть' : 'Добавить тему'}
                    </button>
                )}
            </div>
            {isCreateFormOpen && hasTeacherOrAdminRole(user) && (
                <MathTopicForm
                    classes={classes}
                    defaultClassId={schoolClass.id}
                    key={`create-topic-${schoolClass.id}`}
                    mode="create"
                    onCreated={() => setReloadKey((current) => current + 1)}
                    onClose={() => setIsCreateFormOpen(false)}
                />
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
                                <th scope="row">
                                    <div className="math-topic__name-cell">
                                        {hasTeacherOrAdminRole(user) && (
                                            <button
                                                aria-label={`Редактировать тему «${topic.name}»`}
                                                className="math-topic__edit-button"
                                                onClick={() => openTopicEditor(topic)}
                                                title="Редактировать тему"
                                                type="button"
                                            >
                                                <svg aria-hidden="true" viewBox="0 0 24 24">
                                                    <path d="M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4Z"/>
                                                </svg>
                                            </button>
                                        )}
                                        <span>{topic.name}</span>
                                    </div>
                                </th>
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
            {editingTopic && (
                <MathTopicForm
                    classes={classes}
                    defaultClassId={schoolClass.id}
                    key={editingTopic.id}
                    mode="edit"
                    onClose={() => setEditingTopic(null)}
                    onRemoved={removeTopicFromPage}
                    onSaved={updateTopicInList}
                    topic={editingTopic}
                />
            )}
        </section>
    );
};

export default MathTopicsTable;

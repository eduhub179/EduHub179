import React, {useEffect, useState} from 'react';
import {Link} from 'react-router-dom';
import {fetchMyPlusnikProgress, fetchPlusnikLessons, fetchPlusnikSheets} from '../../api/plusnikApi';
import type {SchoolClassDto} from '../../api/classApi';
import type {User} from '../../auth/authService';

interface MathTopicsTableProps {
    schoolClass: SchoolClassDto;
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
const normalize = (value: string) => value.trim().toLocaleLowerCase('ru-RU');
const cleanTopicName = (name: string) => name.replace(/^тема\s*:?\s*/i, '');
const formatDeadline = (deadline: string | null) => deadline
    ? new Date(deadline).toLocaleString('ru-RU')
    : '—';
const getClassTargetName = (schoolClass: SchoolClassDto) =>
    `${schoolClass.graduation_year}${schoolClass.letter}`.toLocaleLowerCase('ru-RU');

const MathTopicsTable: React.FC<MathTopicsTableProps> = ({schoolClass, user}) => {
    const [topics, setTopics] = useState<MathTopic[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let isCurrent = true;
        setIsLoading(true);
        setError(null);

        const loadTopics = async (): Promise<MathTopic[]> => {
            if (!user?.id) {
                return [];
            }

            if (user.role === 'student') {
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

            if (user.role !== 'teacher' && user.role !== 'admin') {
                return [];
            }

            const classTargetName = getClassTargetName(schoolClass);
            const lessons = await fetchPlusnikLessons();
            const mathLessons = lessons.filter((lesson) =>
                normalize(lesson.target_name) === classTargetName && isMathSubject(lesson.subject_name),
            );
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
    }, [schoolClass, user]);

    return (
        <section className="math-topics" aria-label={`Темы по математике для класса ${schoolClass.name}`}>
            <h2>Темы класса {schoolClass.name}</h2>
            {isLoading ? (
                <p className="math-content__description">Загрузка тем...</p>
            ) : error ? (
                <p className="math-topics__error" role="alert">{error}</p>
            ) : !user?.id ? (
                <p className="math-admin-table__empty">Войдите в аккаунт, чтобы увидеть темы.</p>
            ) : user.role === 'student' && user.classId !== schoolClass.id ? (
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

export default MathTopicsTable;

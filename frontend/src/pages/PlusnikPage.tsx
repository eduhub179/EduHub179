import React, {useEffect, useState} from 'react';
import {Link, useParams} from 'react-router-dom';
import {
    fetchMyPlusnikProgress,
    fetchPlusnikMatrix,
    type PlusnikMatrixDto,
    type PlusnikSheetProgressDto,
} from '../api';
import {useAuth} from '../auth/AuthProvider';
import Header from '../components/Header';
import '../css/PlusnikPage.css';

const formatDate = (value: string | null, dateOnly = false) => {
    if (!value) {
        return '—';
    }

    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
        return value;
    }

    return date.toLocaleString('ru-RU', dateOnly
        ? {dateStyle: 'short', timeZone: 'UTC'}
        : {dateStyle: 'short', timeStyle: 'short'});
};

const cleanSheetName = (name: string) => name.replace(/^тема\s*:?\s*/i, '');

const PlusnikPage: React.FC = () => {
    const {topic: sheetId} = useParams<{topic: string}>();
    const {user} = useAuth();
    const [matrix, setMatrix] = useState<PlusnikMatrixDto | null>(null);
    const [progress, setProgress] = useState<PlusnikSheetProgressDto | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let isCurrent = true;
        setIsLoading(true);
        setError(null);
        setMatrix(null);
        setProgress(null);

        const loadSheet = async () => {
            if (!user?.id || !sheetId) {
                return;
            }

            if (user.role === 'student') {
                const sheets = await fetchMyPlusnikProgress();
                const studentSheet = sheets.find(({sheet}) => sheet.id === sheetId);
                if (isCurrent && studentSheet) {
                    setProgress(studentSheet);
                } else if (isCurrent) {
                    setError('Этот плюсник недоступен вашему аккаунту.');
                }
                return;
            }

            if (user.role === 'teacher' || user.role === 'admin') {
                const loadedMatrix = await fetchPlusnikMatrix(sheetId);
                if (isCurrent) {
                    setMatrix(loadedMatrix);
                }
                return;
            }

            setError('У вашей роли нет доступа к этому плюснику.');
        };

        void loadSheet()
            .catch((loadError: unknown) => {
                if (isCurrent) {
                    setError(loadError instanceof Error
                        ? loadError.message
                        : 'Не удалось загрузить плюсник.');
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
    }, [sheetId, user]);

    const sheet = matrix?.sheet ?? progress?.sheet;
    const isAuthorizedRole = user?.role === 'student' || user?.role === 'teacher' || user?.role === 'admin';

    return (
        <>
            <Header/>
            <main className="plusnik-page">
                <div className="plusnik-page__heading">
                    <div>
                        <h1>{sheet ? cleanSheetName(sheet.name) : 'Плюсник'}</h1>
                        {sheet && (
                            <p>
                                Выдано: {formatDate(sheet.issue_date, true)}
                                {' · '}Дедлайн: {formatDate(sheet.deadline)}
                                {' · '}Статус: {sheet.status}
                            </p>
                        )}
                    </div>
                    <Link to="/math">Назад к математике</Link>
                </div>

                {!user?.id ? (
                    <p className="plusnik-page__message">Войдите в аккаунт, чтобы открыть плюсник.</p>
                ) : !isAuthorizedRole ? (
                    <p className="plusnik-page__message">У вашей роли нет доступа к этому плюснику.</p>
                ) : isLoading ? (
                    <p className="plusnik-page__message">Загрузка плюсника...</p>
                ) : error ? (
                    <p className="plusnik-page__error" role="alert">{error}</p>
                ) : progress ? (
                    <section className="plusnik-page__progress" aria-label="Прогресс по плюснику">
                        <p>Предмет: {progress.subject_name}</p>
                        <p>
                            Решено заданий: {progress.solved_tasks} из {progress.total_tasks}
                        </p>
                        <progress
                            value={progress.solved_tasks}
                            max={Math.max(progress.total_tasks, 1)}
                            aria-label={`${progress.solved_tasks} из ${progress.total_tasks} заданий решено`}
                        />
                    </section>
                ) : matrix ? (
                    <div className="plusnik-page__table-wrap">
                        <table className="plusnik-page__table">
                            <thead>
                                <tr>
                                    <th scope="col">Ученик</th>
                                    {[...matrix.sheet.tasks]
                                        .sort((first, second) => first.sort_order - second.sort_order)
                                        .map((task) => (
                                            <th scope="col" key={task.id}>Задача {task.number}</th>
                                        ))}
                                </tr>
                            </thead>
                            <tbody>
                                {matrix.students.map((student) => (
                                    <tr key={student.id}>
                                        <th scope="row">{student.name}</th>
                                        {[...matrix.sheet.tasks]
                                            .sort((first, second) => first.sort_order - second.sort_order)
                                            .map((task) => {
                                                const hasPlus = matrix.pluses.some((plus) =>
                                                    plus.task_id === task.id && plus.student_id === student.id,
                                                );
                                                return (
                                                    <td key={task.id} className="plusnik-page__cell">
                                                        <span
                                                            className={`plusnik-page__solved${hasPlus ? ' is-solved' : ''}`}
                                                            aria-label={hasPlus ? 'Решена' : 'Не решена'}
                                                        >
                                                            {hasPlus ? '✓' : '—'}
                                                        </span>
                                                    </td>
                                                );
                                            })}
                                    </tr>
                                ))}
                                {matrix.students.length === 0 && (
                                    <tr>
                                        <td className="plusnik-page__empty" colSpan={matrix.sheet.tasks.length + 1}>
                                            Для этого урока пока нет учеников.
                                        </td>
                                    </tr>
                                )}
                            </tbody>
                        </table>
                    </div>
                ) : (
                    <p className="plusnik-page__message">Информация о плюснике не найдена.</p>
                )}
            </main>
        </>
    );
};

export default PlusnikPage;

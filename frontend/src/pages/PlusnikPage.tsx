import React, {useEffect, useState} from 'react';
import {Link, useParams} from 'react-router-dom';
import {
    fetchPlusnikMatrix,
    hasStudentRole,
    hasTeacherOrAdminRole,
    type PlusnikMatrixDto,
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
const formatPlusDate = (value: string) => {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
        ? '—'
        : date.toLocaleDateString('ru-RU', {day: '2-digit', month: '2-digit'});
};

const PlusnikPage: React.FC = () => {
    const {topic: sheetId} = useParams<{ topic: string }>();
    const {user} = useAuth();
    const [matrix, setMatrix] = useState<PlusnikMatrixDto | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [hoveredTaskColumn, setHoveredTaskColumn] = useState<number | null>(null);

    useEffect(() => {
        let isCurrent = true;
        setIsLoading(true);
        setError(null);
        setMatrix(null);

        const loadSheet = async () => {
            if (!user?.id || !sheetId) {
                return;
            }

            if (hasStudentRole(user) || hasTeacherOrAdminRole(user)) {
                const loadedMatrix = await fetchPlusnikMatrix(sheetId);
                if (isCurrent) setMatrix(loadedMatrix);
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

    const sheet = matrix?.sheet;
    const isAuthorizedRole = hasStudentRole(user) || hasTeacherOrAdminRole(user);
    const students = matrix?.students ?? [];
    const orderedStudents = hasStudentRole(user)
        ? [
            ...students.filter((student) => student.id === user?.id),
            ...students.filter((student) => student.id !== user?.id),
        ]
        : students;

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
                ) : matrix ? (
                    <div className="plusnik-page__table-wrap">
                        <table className="plusnik-page__table">
                            <thead>
                            <tr>
                                <th scope="col">Ученик</th>
                                <th scope="col" className="plusnik-page__count-cell" aria-label="Сдано задач">Σ</th>
                                <th scope="col" className="plusnik-page__grade-cell">Оценка</th>
                                {[...matrix.sheet.tasks]
                                    .sort((first, second) => first.sort_order - second.sort_order)
                                    .map((task, taskIndex) => (
                                        <th
                                            scope="col"
                                            key={task.id}
                                            className={hoveredTaskColumn === taskIndex ? 'is-hovered-column' : undefined}
                                            onMouseEnter={() => setHoveredTaskColumn(taskIndex)}
                                            onMouseLeave={() => setHoveredTaskColumn(null)}
                                        >
                                            {task.number}
                                        </th>
                                    ))}
                            </tr>
                            </thead>
                            <tbody>
                            {orderedStudents.map((student) => (
                                <tr
                                    key={student.id}
                                    className={student.id === user?.id ? 'is-current-user' : undefined}
                                >
                                    <th scope="row">{student.name}</th>
                                    <td className="plusnik-page__count-cell">
                                        {matrix.pluses.filter((plus) => plus.student_id === student.id).length}
                                    </td>
                                    <td className="plusnik-page__grade-cell">—</td>
                                    {[...matrix.sheet.tasks]
                                        .sort((first, second) => first.sort_order - second.sort_order)
                                        .map((task, taskIndex) => {
                                            const plus = matrix.pluses.find((record) =>
                                                record.task_id === task.id && record.student_id === student.id,
                                            );
                                            const hasPlus = Boolean(plus);
                                            return (
                                                <td
                                                    key={task.id}
                                                    className={[
                                                        'plusnik-page__cell',
                                                        hasPlus && 'is-solved',
                                                        hoveredTaskColumn === taskIndex && 'is-hovered-column',
                                                    ].filter(Boolean).join(' ')}
                                                    aria-label={hasPlus ? 'Сдано' : 'Не сдано'}
                                                    onMouseEnter={() => setHoveredTaskColumn(taskIndex)}
                                                    onMouseLeave={() => setHoveredTaskColumn(null)}
                                                >
                                                    {plus ? formatPlusDate(plus.granted_at) : ''}
                                                </td>
                                            );
                                        })}
                                </tr>
                            ))}
                            {orderedStudents.length === 0 && (
                                <tr>
                                    <td className="plusnik-page__empty" colSpan={matrix.sheet.tasks.length + 3}>
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

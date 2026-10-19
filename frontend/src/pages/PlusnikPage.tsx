import React from 'react';
import {Link, useParams} from 'react-router-dom';
import Header from '../components/Header';
import '../css/PlusnikPage.css';

const TOPIC_TITLES: Record<string, string> = {
    algebra: 'Алгебра',
    geometry: 'Геометрия',
    other: 'Другие материалы',
};

const PlusnikPage: React.FC = () => {
    const {topic} = useParams<{topic: string}>();
    const topicTitle = topic ? TOPIC_TITLES[topic] : undefined;

    return (
        <>
            <Header/>
            <main className="plusnik-page">
                <div className="plusnik-page__heading">
                    <div>
                        <h1>Плюсник{topicTitle ? ` — ${topicTitle}` : ''}</h1>
                        <p>Заглушка: реальные данные учеников и задач пока не подключены.</p>
                    </div>
                    <Link to="/math">Назад к математике</Link>
                </div>
                <div className="plusnik-page__table-wrap">
                    <table className="plusnik-page__table">
                        <thead>
                            <tr>
                                <th scope="col">Ученик</th>
                                <th scope="col">Задача 1</th>
                                <th scope="col">Задача 2</th>
                                <th scope="col">Задача 3</th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td className="plusnik-page__empty" colSpan={4}>
                                    Данные появятся здесь
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </main>
        </>
    );
};

export default PlusnikPage;

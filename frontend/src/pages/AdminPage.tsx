import React from 'react';
import {Navigate, NavLink, Route, Routes} from 'react-router-dom';
import {hasAdminRole} from '../api';
import {useAuth} from '../auth/AuthProvider';
import Header from '../components/Header';
import AnnouncementsTab from '../components/Admin/AnnouncementsTab';
import LessonsTab from '../components/Admin/LessonsTab';
import UsersTab from '../components/Admin/UsersTab';
import WeeksTab from '../components/Admin/WeeksTab';
import '../css/AdminPage.css';

const tabs = [
    {path: 'lessons', title: 'Замены и отмены'},
    {path: 'users', title: 'Пользователи'},
    {path: 'announcements', title: 'Объявления'},
    {path: 'weeks', title: 'Недели расписания'},
];

/** Admin panel: only for users with role `admin`; the backend checks the role again. */
const AdminPage: React.FC = () => {
    const {user} = useAuth();

    if (!user) {
        return <Navigate to="/login" replace />;
    }

    return (
        <>
            <Header />
            <main className="admin">
                <h1 className="admin__title">Админ-панель</h1>
                {!hasAdminRole(user) ? (
                    <p className="admin__error">Этот раздел доступен только администраторам.</p>
                ) : (
                    <>
                        <nav className="admin__tabs">
                            {tabs.map((tab) => (
                                <NavLink
                                    key={tab.path}
                                    to={`/admin/${tab.path}`}
                                    className={({isActive}) => `admin__tab${isActive ? ' admin__tab--active' : ''}`}
                                >
                                    {tab.title}
                                </NavLink>
                            ))}
                        </nav>
                        <Routes>
                            <Route path="lessons" element={<LessonsTab />} />
                            <Route path="users" element={<UsersTab currentUserId={user.id} />} />
                            <Route path="announcements" element={<AnnouncementsTab />} />
                            <Route path="weeks" element={<WeeksTab />} />
                            <Route path="*" element={<Navigate to="lessons" replace />} />
                        </Routes>
                    </>
                )}
            </main>
        </>
    );
};

export default AdminPage;

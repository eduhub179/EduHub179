import React, {useCallback, useEffect, useState} from 'react';
import type {FormEvent} from 'react';
import {
    createAdminUser,
    fetchAdminUsers,
    fetchSchoolClasses,
    setAdminUserPassword,
    updateAdminUser,
    type AdminUserDto,
    type SchoolClassDto,
    type UserChangeInput,
    type UserFilter,
    type UserRole,
} from '../../api';
import {errorText, roleTitles} from './common';

interface UsersTabProps {
    /** The admin themselves: their role and active flag can't be changed here. */
    currentUserId: string | null;
}

const emptyNewUser = {
    login: '',
    last_name: '',
    first_name: '',
    middle_name: '',
    role: 'student' as UserRole,
    class_id: '',
    password: '',
};

/** Shortest password the backend accepts. */
const MIN_PASSWORD = 8;

/** Search users, change role / class / active flag / password, add a user. */
const UsersTab: React.FC<UsersTabProps> = ({currentUserId}) => {
    const [filter, setFilter] = useState<UserFilter>({q: '', role: '', classId: '', includeInactive: false});
    const [users, setUsers] = useState<AdminUserDto[] | null>(null);
    const [truncated, setTruncated] = useState(false);
    const [classes, setClasses] = useState<SchoolClassDto[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [newUser, setNewUser] = useState(emptyNewUser);
    const [showNewUser, setShowNewUser] = useState(false);
    const [message, setMessage] = useState<string | null>(null);
    /** The user whose password is being typed. */
    const [password, setPassword] = useState<{userId: string; value: string} | null>(null);

    useEffect(() => {
        fetchSchoolClasses()
            .then(({classes: list}) => setClasses(list))
            .catch(() => setClasses([]));
    }, []);

    const load = useCallback(() => {
        setError(null);
        fetchAdminUsers(filter)
            .then((response) => {
                setUsers(response.users);
                setTruncated(response.truncated);
            })
            .catch((loadError) => setError(errorText(loadError)));
    }, [filter]);

    // Search as you type, but not on every key press.
    useEffect(() => {
        const timer = window.setTimeout(load, 300);
        return () => window.clearTimeout(timer);
    }, [load]);

    const change = async (user: AdminUserDto, patch: UserChangeInput) => {
        setError(null);
        try {
            await updateAdminUser(user.id, patch);
            load();
        } catch (changeError) {
            setError(`${user.last_name} ${user.first_name}: ${errorText(changeError)}`);
        }
    };

    const savePassword = async (user: AdminUserDto, value: string) => {
        setError(null);
        setMessage(null);
        try {
            await setAdminUserPassword(user.id, value);
            setPassword(null);
            setMessage(`Пароль для ${user.login} задан.`);
        } catch (passwordError) {
            setError(`${user.last_name} ${user.first_name}: ${errorText(passwordError)}`);
        }
    };

    const create = async (event: FormEvent) => {
        event.preventDefault();
        setMessage(null);
        try {
            const created = await createAdminUser({
                login: newUser.login.trim(),
                role: newUser.role,
                last_name: newUser.last_name.trim(),
                first_name: newUser.first_name.trim(),
                middle_name: newUser.middle_name.trim() || null,
                class_id: newUser.role === 'student' ? newUser.class_id || null : null,
            });
            const login = newUser.login.trim().toLowerCase();
            if (newUser.password) {
                try {
                    await setAdminUserPassword(created.id, newUser.password);
                    setMessage(`Пользователь ${login} добавлен, войти на сайт он может с этим паролем.`);
                } catch (passwordError) {
                    setMessage(`Пользователь ${login} добавлен, но пароль не задан: ${errorText(passwordError)}`);
                }
            } else {
                setMessage(`Пользователь ${login} добавлен. Чтобы он мог войти на сайт, задайте ему пароль кнопкой «Пароль».`);
            }
            setNewUser(emptyNewUser);
            load();
        } catch (createError) {
            setMessage(errorText(createError));
        }
    };

    return (
        <section>
            <div className="admin__toolbar">
                <input
                    className="admin__grow"
                    placeholder="Поиск по ФИО или логину"
                    value={filter.q}
                    onChange={(event) => setFilter({...filter, q: event.target.value})}
                />
                <select value={filter.role} onChange={(event) => setFilter({...filter, role: event.target.value as UserRole | ''})}>
                    <option value="">все роли</option>
                    <option value="student">ученики</option>
                    <option value="teacher">учителя</option>
                    <option value="admin">админы</option>
                </select>
                <select value={filter.classId} onChange={(event) => setFilter({...filter, classId: event.target.value})}>
                    <option value="">все классы</option>
                    {classes.map((schoolClass) => (
                        <option key={schoolClass.id} value={schoolClass.id}>{schoolClass.name}</option>
                    ))}
                </select>
                <label>
                    <input
                        type="checkbox"
                        checked={filter.includeInactive}
                        onChange={(event) => setFilter({...filter, includeInactive: event.target.checked})}
                    />{' '}
                    отключённые
                </label>
                <button type="button" onClick={() => setShowNewUser(!showNewUser)}>
                    {showNewUser ? 'Скрыть' : 'Добавить пользователя'}
                </button>
            </div>

            {showNewUser && (
                <form className="admin__card admin__form" onSubmit={(event) => void create(event)}>
                    <label>
                        Логин (часть почты до @)
                        <input required value={newUser.login} onChange={(event) => setNewUser({...newUser, login: event.target.value})} />
                    </label>
                    <label>
                        Фамилия
                        <input required value={newUser.last_name} onChange={(event) => setNewUser({...newUser, last_name: event.target.value})} />
                    </label>
                    <label>
                        Имя
                        <input required value={newUser.first_name} onChange={(event) => setNewUser({...newUser, first_name: event.target.value})} />
                    </label>
                    <label>
                        Отчество
                        <input value={newUser.middle_name} onChange={(event) => setNewUser({...newUser, middle_name: event.target.value})} />
                    </label>
                    <label>
                        Роль
                        <select value={newUser.role} onChange={(event) => setNewUser({...newUser, role: event.target.value as UserRole})}>
                            <option value="student">Ученик</option>
                            <option value="teacher">Учитель</option>
                            <option value="admin">Админ</option>
                        </select>
                    </label>
                    {newUser.role === 'student' && (
                        <label>
                            Класс
                            <select value={newUser.class_id} onChange={(event) => setNewUser({...newUser, class_id: event.target.value})}>
                                <option value="">без класса</option>
                                {classes.map((schoolClass) => (
                                    <option key={schoolClass.id} value={schoolClass.id}>{schoolClass.name}</option>
                                ))}
                            </select>
                        </label>
                    )}
                    <label>
                        Пароль для входа на сайт
                        <input
                            type="password"
                            autoComplete="new-password"
                            minLength={MIN_PASSWORD}
                            placeholder="можно задать позже"
                            value={newUser.password}
                            onChange={(event) => setNewUser({...newUser, password: event.target.value})}
                        />
                    </label>
                    <div className="admin__actions">
                        <button type="submit">Добавить</button>
                    </div>
                </form>
            )}
            {message && <p className="admin__muted">{message}</p>}
            {error && <p className="admin__error">{error}</p>}
            {users === null && !error && <p className="admin__muted">Загрузка…</p>}
            {truncated && <p className="admin__muted">Показаны первые 500. Уточните поиск.</p>}

            {users && (
                <div className="admin__card">
                    <table className="admin__table">
                        <thead>
                            <tr>
                                <th>ФИО</th>
                                <th>Логин</th>
                                <th>Роль</th>
                                <th>Класс</th>
                                <th>Telegram</th>
                                <th />
                            </tr>
                        </thead>
                        <tbody>
                            {users.length === 0 && (
                                <tr><td colSpan={6} className="admin__muted">Никого не нашли.</td></tr>
                            )}
                            {users.map((user) => {
                                const isSelf = user.id === currentUserId;
                                const typing = password?.userId === user.id ? password.value : null;
                                return (
                                    <React.Fragment key={user.id}>
                                    <tr className={user.is_active ? '' : 'admin__row--cancelled'}>
                                        <td>{[user.last_name, user.first_name, user.middle_name].filter(Boolean).join(' ')}</td>
                                        <td className="admin__muted">{user.login}</td>
                                        <td>
                                            <select
                                                value={user.role}
                                                disabled={isSelf}
                                                title={isSelf ? 'Свою роль поменять нельзя' : undefined}
                                                onChange={(event) => void change(user, {role: event.target.value as UserRole})}
                                            >
                                                {Object.entries(roleTitles).map(([role, title]) => (
                                                    <option key={role} value={role}>{title}</option>
                                                ))}
                                            </select>
                                        </td>
                                        <td>
                                            {user.role === 'student' ? (
                                                <select
                                                    value={user.class_id ?? ''}
                                                    onChange={(event) => void change(user, {class_id: event.target.value || null})}
                                                >
                                                    <option value="">—</option>
                                                    {classes.map((schoolClass) => (
                                                        <option key={schoolClass.id} value={schoolClass.id}>{schoolClass.name}</option>
                                                    ))}
                                                </select>
                                            ) : (
                                                <span className="admin__muted">—</span>
                                            )}
                                        </td>
                                        <td>{user.telegram_bound ? '✓' : ''}</td>
                                        <td className="admin__nowrap">
                                            <button
                                                type="button"
                                                className="admin__secondary"
                                                onClick={() => setPassword(typing === null ? {userId: user.id, value: ''} : null)}
                                            >
                                                Пароль
                                            </button>{' '}
                                            {!isSelf && (
                                                <button
                                                    type="button"
                                                    className="admin__secondary"
                                                    onClick={() => void change(user, {is_active: !user.is_active})}
                                                >
                                                    {user.is_active ? 'Отключить' : 'Включить'}
                                                </button>
                                            )}
                                        </td>
                                    </tr>
                                    {typing !== null && (
                                        <tr>
                                            <td colSpan={6}>
                                                <form
                                                    className="admin__editor"
                                                    onSubmit={(event) => {
                                                        event.preventDefault();
                                                        void savePassword(user, typing);
                                                    }}
                                                >
                                                    <label>
                                                        Новый пароль для {user.login}
                                                        <input
                                                            type="password"
                                                            autoComplete="new-password"
                                                            autoFocus
                                                            minLength={MIN_PASSWORD}
                                                            required
                                                            value={typing}
                                                            onChange={(event) => setPassword({userId: user.id, value: event.target.value})}
                                                        />
                                                    </label>
                                                    <button type="submit">Сохранить</button>
                                                    <button type="button" className="admin__secondary" onClick={() => setPassword(null)}>
                                                        Отмена
                                                    </button>
                                                    <span className="admin__muted">Старый пароль перестанет работать. Сообщите новый лично.</span>
                                                </form>
                                            </td>
                                        </tr>
                                    )}
                                    </React.Fragment>
                                );
                            })}
                        </tbody>
                    </table>
                </div>
            )}
        </section>
    );
};

export default UsersTab;

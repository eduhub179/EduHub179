import React, {useCallback, useEffect, useState} from 'react';
import type {FormEvent} from 'react';
import {createWeek, fetchWeeks, setWeekStatus, type WeekDto, type WeekSource} from '../../api';
import {errorText, formatDay, mondayOf} from './common';

const nextMonday = (): string => {
    const date = new Date();
    date.setDate(date.getDate() + 7);
    return mondayOf(date);
};

/** Schedule weeks: create a draft (empty, from templates or a copy), publish, hide. */
const WeeksTab: React.FC = () => {
    const [weeks, setWeeks] = useState<WeekDto[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [form, setForm] = useState<{monday: string; source: WeekSource; copyFrom: string}>({
        monday: nextMonday(),
        source: 'templates',
        copyFrom: '',
    });
    const [message, setMessage] = useState<string | null>(null);

    const load = useCallback(() => {
        fetchWeeks()
            .then((list) => {
                setWeeks(list);
                setForm((current) => ({...current, copyFrom: current.copyFrom || list[0]?.week_start_date || ''}));
            })
            .catch((loadError) => setError(errorText(loadError)));
    }, []);

    useEffect(load, [load]);

    const toggle = async (week: WeekDto) => {
        setError(null);
        try {
            await setWeekStatus(week.week_start_date, week.status === 'published' ? 'draft' : 'published');
            load();
        } catch (toggleError) {
            setError(errorText(toggleError));
        }
    };

    const create = async (event: FormEvent) => {
        event.preventDefault();
        setMessage(null);
        // The date picker allows any day: take the Monday of the chosen week.
        const monday = mondayOf(new Date(`${form.monday}T00:00:00`));
        try {
            await createWeek(monday, form.source, form.source === 'copy' ? form.copyFrom : undefined);
            setMessage(`Неделя с ${formatDay(monday)} создана как черновик. Проверьте её и опубликуйте.`);
            load();
        } catch (createError) {
            setMessage(errorText(createError));
        }
    };

    return (
        <section>
            <form className="admin__card admin__form" onSubmit={(event) => void create(event)}>
                <h3>Новая неделя</h3>
                <label>
                    Неделя (любой день, возьмётся понедельник)
                    <input type="date" required value={form.monday} onChange={(event) => setForm({...form, monday: event.target.value})} />
                </label>
                <label>
                    Заполнить
                    <select value={form.source} onChange={(event) => setForm({...form, source: event.target.value as WeekSource})}>
                        <option value="templates">по постоянному расписанию</option>
                        <option value="copy">копией другой недели</option>
                        <option value="empty">пустая</option>
                    </select>
                </label>
                {form.source === 'copy' && (
                    <label>
                        Какую неделю скопировать
                        <select value={form.copyFrom} onChange={(event) => setForm({...form, copyFrom: event.target.value})}>
                            {(weeks ?? []).map((week) => (
                                <option key={week.week_start_date} value={week.week_start_date}>
                                    с {formatDay(week.week_start_date)}
                                </option>
                            ))}
                        </select>
                    </label>
                )}
                <p className="admin__muted">
                    Постоянное расписание редактируется во вкладке «Расписание». Копия недели берёт только обычные
                    уроки: отмены, замены и разовые уроки не копируются.
                </p>
                <div className="admin__actions">
                    <button type="submit">Создать черновик</button>
                </div>
                {message && <p className="admin__muted">{message}</p>}
            </form>

            {error && <p className="admin__error">{error}</p>}
            {weeks === null && !error && <p className="admin__muted">Загрузка…</p>}
            {weeks && (
                <div className="admin__card">
                    <table className="admin__table">
                        <thead>
                            <tr>
                                <th>Неделя</th>
                                <th>Статус</th>
                                <th>Копия недели</th>
                                <th />
                            </tr>
                        </thead>
                        <tbody>
                            {weeks.map((week) => (
                                <tr key={week.week_start_date}>
                                    <td>с {formatDay(week.week_start_date)}</td>
                                    <td>
                                        {week.status === 'published' ? (
                                            <span className="admin__badge admin__badge--green">видна ученикам</span>
                                        ) : (
                                            <span className="admin__badge">черновик</span>
                                        )}
                                    </td>
                                    <td className="admin__muted">{week.copied_from ? `с ${formatDay(week.copied_from)}` : '—'}</td>
                                    <td className="admin__nowrap">
                                        <button type="button" className="admin__secondary" onClick={() => void toggle(week)}>
                                            {week.status === 'published' ? 'Скрыть' : 'Опубликовать'}
                                        </button>
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            )}
        </section>
    );
};

export default WeeksTab;

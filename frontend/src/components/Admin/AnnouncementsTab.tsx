import React, {useCallback, useEffect, useState} from 'react';
import type {FormEvent} from 'react';
import {
    createAnnouncement,
    deleteAnnouncement,
    fetchAdminAnnouncements,
    fetchSchoolClasses,
    getSchoolClassLabel,
    updateAnnouncement,
    type AdminAnnouncementDto,
    type AnnouncementInput,
    type AnnouncementPriority,
    type AnnouncementStatus,
    type SchoolClassDto,
} from '../../api';
import {errorText} from './common';

const statusTitles: Record<AnnouncementStatus, string> = {
    draft: 'черновик',
    published: 'опубликовано',
    archived: 'в архиве',
};

const priorityTitles: Record<AnnouncementPriority, string> = {
    low: 'низкий',
    normal: 'обычный',
    high: 'высокий',
    urgent: 'срочно',
};

interface FormState {
    id: string | null;
    title: string;
    content: string;
    status: AnnouncementStatus;
    priority: AnnouncementPriority;
    /** `datetime-local` value or ''. */
    expiresAt: string;
    classIds: string[];
    /** Kept as they are: the panel edits only class targets. */
    groupIds: string[];
    userIds: string[];
}

const emptyForm: FormState = {
    id: null,
    title: '',
    content: '',
    status: 'published',
    priority: 'normal',
    expiresAt: '',
    classIds: [],
    groupIds: [],
    userIds: [],
};

/** RFC 3339 → `YYYY-MM-DDTHH:mm` in local time for `<input type="datetime-local">`. */
const toLocalInput = (iso: string | null): string => {
    if (!iso) return '';
    const date = new Date(iso);
    const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
    return local.toISOString().slice(0, 16);
};

/** Write, target (whole school or classes), publish and archive announcements. */
const AnnouncementsTab: React.FC = () => {
    const [items, setItems] = useState<AdminAnnouncementDto[] | null>(null);
    const [classes, setClasses] = useState<SchoolClassDto[]>([]);
    const [form, setForm] = useState<FormState | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

    const load = useCallback(() => {
        fetchAdminAnnouncements()
            .then(setItems)
            .catch((loadError) => setError(errorText(loadError)));
    }, []);

    useEffect(() => {
        load();
        fetchSchoolClasses()
            .then(({classes: list}) => setClasses(list))
            .catch(() => setClasses([]));
    }, [load]);

    const className = (id: string) => classes.find((schoolClass) => schoolClass.id === id)?.name ?? 'класс';

    const audience = (item: AdminAnnouncementDto): string => {
        if (item.class_ids.length + item.group_ids.length + item.user_ids.length === 0) return 'вся школа';
        const parts = item.class_ids.map(className);
        if (item.group_ids.length) parts.push(`групп: ${item.group_ids.length}`);
        if (item.user_ids.length) parts.push(`людей: ${item.user_ids.length}`);
        return parts.join(', ');
    };

    const submit = async (event: FormEvent) => {
        event.preventDefault();
        if (!form) return;
        setError(null);
        const input: AnnouncementInput = {
            title: form.title.trim() || null,
            content: form.content.trim() || null,
            status: form.status,
            priority: form.priority,
            expires_at: form.expiresAt ? new Date(form.expiresAt).toISOString() : null,
            class_ids: form.classIds,
            group_ids: form.groupIds,
            user_ids: form.userIds,
        };
        try {
            if (form.id) {
                await updateAnnouncement(form.id, input);
            } else {
                await createAnnouncement(input);
            }
            setForm(null);
            load();
        } catch (submitError) {
            setError(errorText(submitError));
        }
    };

    const remove = async (id: string) => {
        setError(null);
        try {
            await deleteAnnouncement(id);
            setConfirmDelete(null);
            load();
        } catch (removeError) {
            setError(errorText(removeError));
        }
    };

    const toggleClass = (id: string) => {
        if (!form) return;
        const classIds = form.classIds.includes(id)
            ? form.classIds.filter((classId) => classId !== id)
            : [...form.classIds, id];
        setForm({...form, classIds});
    };

    return (
        <section>
            <div className="admin__toolbar">
                <button type="button" onClick={() => setForm(form ? null : emptyForm)}>
                    {form ? 'Закрыть' : 'Новое объявление'}
                </button>
            </div>

            {form && (
                <form className="admin__card admin__form" onSubmit={(event) => void submit(event)}>
                    <h3>{form.id ? 'Изменить объявление' : 'Новое объявление'}</h3>
                    <label className="admin__wide">
                        Заголовок
                        <input maxLength={255} value={form.title} onChange={(event) => setForm({...form, title: event.target.value})} />
                    </label>
                    <label className="admin__wide">
                        Текст (поддерживается Markdown, как в Telegram)
                        <textarea rows={5} value={form.content} onChange={(event) => setForm({...form, content: event.target.value})} />
                    </label>
                    <fieldset className="admin__wide admin__classes">
                        <legend>Кому</legend>
                        <label>
                            <input type="checkbox" checked={form.classIds.length + form.groupIds.length + form.userIds.length === 0}
                                onChange={() => setForm({...form, classIds: [], groupIds: [], userIds: []})} />{' '}
                            вся школа
                        </label>
                        {classes.map((schoolClass) => (
                            <label key={schoolClass.id}>
                                <input
                                    type="checkbox"
                                    checked={form.classIds.includes(schoolClass.id)}
                                    onChange={() => toggleClass(schoolClass.id)}
                                />{' '}
                                {getSchoolClassLabel(schoolClass)}
                            </label>
                        ))}
                    </fieldset>
                    <label>
                        Статус
                        <select value={form.status} onChange={(event) => setForm({...form, status: event.target.value as AnnouncementStatus})}>
                            {Object.entries(statusTitles).map(([status, title]) => (
                                <option key={status} value={status}>{title}</option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Важность
                        <select value={form.priority} onChange={(event) => setForm({...form, priority: event.target.value as AnnouncementPriority})}>
                            {Object.entries(priorityTitles).map(([priority, title]) => (
                                <option key={priority} value={priority}>{title}</option>
                            ))}
                        </select>
                    </label>
                    <label>
                        Скрыть после (необязательно)
                        <input type="datetime-local" value={form.expiresAt} onChange={(event) => setForm({...form, expiresAt: event.target.value})} />
                    </label>
                    <div className="admin__actions admin__wide">
                        <button type="submit">{form.id ? 'Сохранить' : 'Создать'}</button>
                    </div>
                </form>
            )}

            {error && <p className="admin__error">{error}</p>}
            {items === null && !error && <p className="admin__muted">Загрузка…</p>}
            {items?.length === 0 && <p className="admin__muted">Объявлений пока нет.</p>}
            {items?.map((item) => (
                <article key={item.id} className="admin__card">
                    <div className="admin__card-head">
                        <h3>{item.title ?? 'Без заголовка'}</h3>
                        <span className={`admin__badge${item.status === 'published' ? ' admin__badge--green' : ''}`}>
                            {statusTitles[item.status]}
                        </span>
                        {(item.priority === 'high' || item.priority === 'urgent') && (
                            <span className="admin__badge admin__badge--red">{priorityTitles[item.priority]}</span>
                        )}
                    </div>
                    {item.content && <p className="admin__content">{item.content}</p>}
                    <p className="admin__muted">
                        Кому: {audience(item)} · {item.author_name} ·{' '}
                        {new Date(item.created_at).toLocaleString('ru-RU', {dateStyle: 'short', timeStyle: 'short'})}
                        {item.expires_at && <> · до {new Date(item.expires_at).toLocaleString('ru-RU', {dateStyle: 'short', timeStyle: 'short'})}</>}
                    </p>
                    <div className="admin__actions">
                        <button
                            type="button"
                            className="admin__secondary"
                            onClick={() =>
                                setForm({
                                    id: item.id,
                                    title: item.title ?? '',
                                    content: item.content ?? '',
                                    status: item.status,
                                    priority: item.priority,
                                    expiresAt: toLocalInput(item.expires_at),
                                    classIds: item.class_ids,
                                    groupIds: item.group_ids,
                                    userIds: item.user_ids,
                                })
                            }
                        >
                            Изменить
                        </button>
                        {confirmDelete === item.id ? (
                            <>
                                <button type="button" className="admin__danger" onClick={() => void remove(item.id)}>
                                    Точно удалить
                                </button>
                                <button type="button" className="admin__secondary" onClick={() => setConfirmDelete(null)}>
                                    Нет
                                </button>
                            </>
                        ) : (
                            <button type="button" className="admin__secondary" onClick={() => setConfirmDelete(item.id)}>
                                Удалить
                            </button>
                        )}
                    </div>
                </article>
            ))}
        </section>
    );
};

export default AnnouncementsTab;

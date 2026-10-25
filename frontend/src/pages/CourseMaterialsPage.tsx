import React, {useCallback, useEffect, useState} from 'react';
import type {FormEvent} from 'react';
import {Link, useParams} from 'react-router-dom';
import {formatClassName} from '../api/classApi';
import {
    addMaterialLink,
    createMaterialTopic,
    deleteMaterial,
    deleteMaterialTopic,
    fetchCourse,
    fetchMaterialTopics,
    fetchMaterials,
    materialFileUrl,
    updateMaterial,
    updateMaterialTopic,
    uploadMaterialFile,
    type CourseDto,
    type MaterialDto,
    type MaterialTopicSelector,
    type MaterialTopicsDto,
} from '../api';
import {useAuth} from '../auth/AuthProvider';
import FileDropDialog from '../components/FileDropDialog';
import Header from '../components/Header';
import '../css/CourseMaterialsPage.css';

/** Sidebar entry: all materials, those without a topic, or a topic id. */
type TopicKey = 'all' | 'none' | string;

const MAX_FILE_SIZE_BYTES = 100 * 1024 * 1024;

const selectorOf = (key: TopicKey): MaterialTopicSelector =>
    key === 'all' || key === 'none' ? key : {topicId: key};

const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} Б`;
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} КБ`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
};

const fileKind = (mimeType: string, fileName: string): string => {
    if (mimeType === 'application/pdf') return 'PDF';
    if (mimeType.startsWith('image/')) return 'Изображение';
    if (mimeType.startsWith('video/')) return 'Видео';
    const extension = fileName.split('.').pop();
    return extension && extension !== fileName ? extension.toUpperCase() : 'Файл';
};

const hostOf = (url: string): string => {
    try {
        return new URL(url).host;
    } catch {
        return url;
    }
};

const errorText = (error: unknown, fallback: string) =>
    error instanceof Error ? error.message : fallback;

const CourseMaterialsPage: React.FC = () => {
    const {lessonId = ''} = useParams<{lessonId: string}>();
    const {user} = useAuth();
    const [course, setCourse] = useState<CourseDto | null>(null);
    const [overview, setOverview] = useState<MaterialTopicsDto | null>(null);
    const [selected, setSelected] = useState<TopicKey>('all');
    // Materials per sidebar entry: switching back to a loaded topic makes no request.
    const [cache, setCache] = useState<Record<TopicKey, MaterialDto[]>>({});
    const [isLoadingMaterials, setIsLoadingMaterials] = useState(false);
    const [pageError, setPageError] = useState<string | null>(null);
    const [actionError, setActionError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [isFileDialogOpen, setIsFileDialogOpen] = useState(false);

    const [newTopic, setNewTopic] = useState('');
    const [editingTopic, setEditingTopic] = useState<{id: string; name: string} | null>(null);
    const [confirmingTopic, setConfirmingTopic] = useState<string | null>(null);
    const [confirmingMaterial, setConfirmingMaterial] = useState<string | null>(null);
    const [linkForm, setLinkForm] = useState<{title: string; url: string; description: string} | null>(null);

    const canManage = course?.can_manage === true;
    const materials = cache[selected];

    const loadTopics = useCallback(async () => {
        setOverview(await fetchMaterialTopics(lessonId));
    }, [lessonId]);

    // Course header and topic sidebar: two requests, then materials of the selected topic.
    useEffect(() => {
        if (!user?.id || !lessonId) {
            return;
        }
        let isCurrent = true;
        setPageError(null);
        setCache({});
        Promise.all([fetchCourse(lessonId), fetchMaterialTopics(lessonId)])
            .then(([loadedCourse, loadedOverview]) => {
                if (isCurrent) {
                    setCourse(loadedCourse);
                    setOverview(loadedOverview);
                }
            })
            .catch((error: unknown) => {
                if (isCurrent) setPageError(errorText(error, 'Не удалось загрузить курс.'));
            });
        return () => {
            isCurrent = false;
        };
    }, [lessonId, user?.id]);

    useEffect(() => {
        if (!course || cache[selected]) {
            return;
        }
        let isCurrent = true;
        setIsLoadingMaterials(true);
        fetchMaterials(lessonId, selectorOf(selected))
            .then((loaded) => {
                if (isCurrent) setCache((current) => ({...current, [selected]: loaded}));
            })
            .catch((error: unknown) => {
                if (isCurrent) setActionError(errorText(error, 'Не удалось загрузить материалы.'));
            })
            .finally(() => {
                if (isCurrent) setIsLoadingMaterials(false);
            });
        return () => {
            isCurrent = false;
        };
    }, [cache, course, lessonId, selected]);

    /** Runs a change, then refreshes counts and drops cached lists (they may be stale). */
    const mutate = async (action: () => Promise<unknown>, fallback: string) => {
        setBusy(true);
        setActionError(null);
        try {
            await action();
            setCache({});
            await loadTopics();
        } catch (error) {
            setActionError(errorText(error, fallback));
        } finally {
            setBusy(false);
        }
    };

    const currentTopicId = selected === 'all' || selected === 'none' ? null : selected;

    const handleCreateTopic = (event: FormEvent) => {
        event.preventDefault();
        const name = newTopic.trim();
        if (!name) return;
        void mutate(async () => {
            const topic = await createMaterialTopic(lessonId, name);
            setNewTopic('');
            setSelected(topic.id);
        }, 'Не удалось создать тему.');
    };

    const handleRenameTopic = (event: FormEvent) => {
        event.preventDefault();
        if (!editingTopic?.name.trim()) return;
        const {id, name} = editingTopic;
        void mutate(async () => {
            await updateMaterialTopic(id, {name: name.trim()});
            setEditingTopic(null);
        }, 'Не удалось переименовать тему.');
    };

    const handleDeleteTopic = (topicId: string) => {
        void mutate(async () => {
            await deleteMaterialTopic(topicId);
            setConfirmingTopic(null);
            if (selected === topicId) setSelected('none');
        }, 'Не удалось удалить тему.');
    };

    const handleFiles = (chosen: File[]) => {
        if (!chosen.length) return;
        const tooLarge = chosen.find((file) => file.size > MAX_FILE_SIZE_BYTES);
        if (tooLarge) {
            setActionError(`Файл «${tooLarge.name}» больше 100 МБ.`);
            return;
        }
        void mutate(async () => {
            // One by one: each file lands at the end of the current topic in order.
            for (const file of chosen) {
                await uploadMaterialFile(lessonId, file, {topicId: currentTopicId});
            }
        }, 'Не удалось загрузить файл.');
    };

    const handleAddLink = (event: FormEvent) => {
        event.preventDefault();
        if (!linkForm) return;
        void mutate(async () => {
            await addMaterialLink(lessonId, {
                title: linkForm.title,
                url: linkForm.url,
                description: linkForm.description || null,
                topic_id: currentTopicId,
            });
            setLinkForm(null);
        }, 'Не удалось добавить ссылку.');
    };

    const handleMove = (materialId: string, topicId: string) => {
        void mutate(
            () => updateMaterial(materialId, {topic_id: topicId || null}),
            'Не удалось перенести материал.',
        );
    };

    const handleDeleteMaterial = (materialId: string) => {
        void mutate(async () => {
            await deleteMaterial(materialId);
            setConfirmingMaterial(null);
        }, 'Не удалось удалить материал.');
    };

    const topicTitle = selected === 'all'
        ? 'Все материалы'
        : selected === 'none'
            ? 'Без темы'
            : overview?.topics.find((topic) => topic.id === selected)?.name ?? 'Тема';

    const sidebarItem = (key: TopicKey, label: string, count: number) => (
        <button
            type="button"
            className={`course-materials__topic${selected === key ? ' is-active' : ''}`}
            onClick={() => setSelected(key)}
            aria-pressed={selected === key}
        >
            <span className="course-materials__topic-name">{label}</span>
            <span className="course-materials__count">{count}</span>
        </button>
    );

    if (!user?.id) {
        return (
            <>
                <Header/>
                <main className="course-materials"><p>Войдите в аккаунт, чтобы увидеть материалы.</p></main>
            </>
        );
    }

    return (
        <>
            <Header/>
            <main className="course-materials">
                <div className="course-materials__heading">
                    <div>
                        <span className="course-materials__eyebrow">Материалы курса</span>
                        <h1>{course ? `${course.subject_name} — ${formatClassName(course.target_name)}` : 'Материалы'}</h1>
                    </div>
                    <Link to="/math">Назад к математике</Link>
                </div>

                {pageError ? (
                    <p className="course-materials__error" role="alert">{pageError}</p>
                ) : !course || !overview ? (
                    <p className="course-materials__muted">Загрузка...</p>
                ) : (
                    <div className="course-materials__layout">
                        <aside className="course-materials__sidebar" aria-label="Темы">
                            {sidebarItem('all', 'Все материалы', overview.total_count)}
                            {overview.topics.map((topic) => (
                                <div className="course-materials__topic-row" key={topic.id}>
                                    {editingTopic?.id === topic.id ? (
                                        <form className="course-materials__inline-form" onSubmit={handleRenameTopic}>
                                            <input
                                                autoFocus
                                                value={editingTopic.name}
                                                maxLength={255}
                                                onChange={(event) => setEditingTopic({id: topic.id, name: event.target.value})}
                                                onKeyDown={(event) => event.key === 'Escape' && setEditingTopic(null)}
                                                aria-label="Новое название темы"
                                            />
                                            <button type="submit" disabled={busy}>OK</button>
                                        </form>
                                    ) : (
                                        sidebarItem(topic.id, topic.name, topic.material_count)
                                    )}
                                    {canManage && editingTopic?.id !== topic.id && (
                                        confirmingTopic === topic.id ? (
                                            <span className="course-materials__confirm">
                                                <button type="button" className="is-danger" disabled={busy}
                                                        onClick={() => handleDeleteTopic(topic.id)}>Удалить</button>
                                                <button type="button" onClick={() => setConfirmingTopic(null)}>Отмена</button>
                                            </span>
                                        ) : (
                                            <span className="course-materials__row-actions">
                                                <button type="button" title="Переименовать" aria-label={`Переименовать «${topic.name}»`}
                                                        onClick={() => setEditingTopic({id: topic.id, name: topic.name})}>✎</button>
                                                <button type="button" title="Удалить тему (материалы останутся без темы)"
                                                        aria-label={`Удалить тему «${topic.name}»`}
                                                        onClick={() => setConfirmingTopic(topic.id)}>×</button>
                                            </span>
                                        )
                                    )}
                                </div>
                            ))}
                            {overview.without_topic_count > 0 && sidebarItem('none', 'Без темы', overview.without_topic_count)}
                            {canManage && (
                                <form className="course-materials__inline-form" onSubmit={handleCreateTopic}>
                                    <input
                                        value={newTopic}
                                        maxLength={255}
                                        placeholder="Новая тема"
                                        onChange={(event) => setNewTopic(event.target.value)}
                                        aria-label="Название новой темы"
                                    />
                                    <button type="submit" disabled={busy || !newTopic.trim()}>+</button>
                                </form>
                            )}
                        </aside>

                        <section className="course-materials__content" aria-live="polite">
                            <div className="course-materials__content-head">
                                <h2>{topicTitle}</h2>
                                {canManage && (
                                    <div className="course-materials__toolbar">
                                        <FileDropDialog
                                            open={isFileDialogOpen}
                                            title={`Загрузить в «${topicTitle}»`}
                                            multiple
                                            hint={currentTopicId
                                                ? 'Файлы попадут в эту тему. Каждый до 100 МБ.'
                                                : 'Файлы попадут в «Без темы». Каждый до 100 МБ.'}
                                            onClose={() => setIsFileDialogOpen(false)}
                                            onFiles={handleFiles}
                                        />
                                        <button type="button" className="is-primary" disabled={busy}
                                                onClick={() => setIsFileDialogOpen(true)}>
                                            {busy ? 'Сохранение…' : 'Загрузить файлы'}
                                        </button>
                                        <button type="button" disabled={busy}
                                                onClick={() => setLinkForm(linkForm ? null : {title: '', url: '', description: ''})}>
                                            Добавить ссылку
                                        </button>
                                    </div>
                                )}
                            </div>
                            {canManage && selected === 'all' && (
                                <p className="course-materials__muted">Новые материалы отсюда попадают в «Без темы».</p>
                            )}

                            {linkForm && (
                                <form className="course-materials__link-form" onSubmit={handleAddLink}>
                                    <input required maxLength={255} placeholder="Название" value={linkForm.title}
                                           onChange={(event) => setLinkForm({...linkForm, title: event.target.value})}/>
                                    <input required type="url" maxLength={2000} placeholder="https://..." value={linkForm.url}
                                           onChange={(event) => setLinkForm({...linkForm, url: event.target.value})}/>
                                    <input maxLength={5000} placeholder="Описание (необязательно)" value={linkForm.description}
                                           onChange={(event) => setLinkForm({...linkForm, description: event.target.value})}/>
                                    <button type="submit" className="is-primary" disabled={busy}>Добавить</button>
                                </form>
                            )}

                            {actionError && <p className="course-materials__error" role="alert">{actionError}</p>}

                            {!materials || isLoadingMaterials ? (
                                <p className="course-materials__muted">Загрузка материалов...</p>
                            ) : materials.length === 0 ? (
                                <p className="course-materials__muted">Материалов пока нет.</p>
                            ) : (
                                <ul className="course-materials__list">
                                    {materials.map((material) => {
                                        const href = material.kind === 'file' ? materialFileUrl(material.id) : material.url ?? '#';
                                        return (
                                            <li className="course-materials__item" key={material.id}>
                                                <span className="course-materials__icon" aria-hidden="true">
                                                    {material.kind === 'file' ? '📄' : '🔗'}
                                                </span>
                                                <div className="course-materials__body">
                                                    <a href={href} target="_blank" rel="noopener noreferrer">{material.title}</a>
                                                    <span className="course-materials__meta">
                                                        {material.file
                                                            ? `${fileKind(material.file.mime_type, material.file.file_name)} · ${formatSize(material.file.size_bytes)}`
                                                            : hostOf(material.url ?? '')}
                                                    </span>
                                                    {material.description && (
                                                        <p className="course-materials__description">{material.description}</p>
                                                    )}
                                                </div>
                                                {canManage && (
                                                    <div className="course-materials__item-actions">
                                                        <select
                                                            value={material.topic_id ?? ''}
                                                            disabled={busy}
                                                            onChange={(event) => handleMove(material.id, event.target.value)}
                                                            aria-label="Перенести в тему"
                                                        >
                                                            <option value="">Без темы</option>
                                                            {overview.topics.map((topic) => (
                                                                <option key={topic.id} value={topic.id}>{topic.name}</option>
                                                            ))}
                                                        </select>
                                                        {confirmingMaterial === material.id ? (
                                                            <span className="course-materials__confirm">
                                                                <button type="button" className="is-danger" disabled={busy}
                                                                        onClick={() => handleDeleteMaterial(material.id)}>Удалить</button>
                                                                <button type="button" onClick={() => setConfirmingMaterial(null)}>Отмена</button>
                                                            </span>
                                                        ) : (
                                                            <button type="button" className="course-materials__remove"
                                                                    aria-label={`Удалить «${material.title}»`}
                                                                    onClick={() => setConfirmingMaterial(material.id)}>×</button>
                                                        )}
                                                    </div>
                                                )}
                                            </li>
                                        );
                                    })}
                                </ul>
                            )}
                        </section>
                    </div>
                )}
            </main>
        </>
    );
};

export default CourseMaterialsPage;

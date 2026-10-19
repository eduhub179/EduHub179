import React, { useCallback, useEffect, useRef, useState } from 'react';
import {Link} from 'react-router-dom';

interface TopicFile {
    id: string;
    name: string;
    url: string;
}

const topics = [
    { id: 'algebra', title: 'Алгебра' },
    { id: 'geometry', title: 'Геометрия' },
    { id: 'other', title: 'Другие материалы' },
];

interface MathTopicsTableProps {
    isAdmin: boolean;
}

const MathTopicsTable: React.FC<MathTopicsTableProps> = ({ isAdmin }) => {
    const [filesByTopic, setFilesByTopic] = useState<Record<string, TopicFile[]>>({});
    const [uploadingTopic, setUploadingTopic] = useState<string | null>(null);
    const [error, setError] = useState('');
    const fileInputs = useRef<Record<string, HTMLInputElement | null>>({});

    const loadTopicFiles = useCallback(async (topicId: string): Promise<TopicFile[]> => {
        const response = await fetch(`/api/math/files?topic=${encodeURIComponent(topicId)}`);
        if (!response.ok) {
            throw new Error('Не удалось загрузить список файлов.');
        }
        return response.json() as Promise<TopicFile[]>;
    }, []);

    useEffect(() => {
        let isCurrent = true;
        Promise.all(topics.map(async (topic) => [topic.id, await loadTopicFiles(topic.id)] as const))
            .then((entries) => {
                if (isCurrent) {
                    setFilesByTopic(Object.fromEntries(entries));
                    setError('');
                }
            })
            .catch((loadError: unknown) => {
                if (isCurrent) {
                    setError(loadError instanceof Error
                        ? loadError.message
                        : 'Не удалось загрузить список файлов.');
                }
            });

        return () => {
            isCurrent = false;
        };
    }, [loadTopicFiles]);

    const uploadFiles = async (topicId: string, selectedFiles: FileList | null) => {
        if (!selectedFiles?.length) {
            return;
        }

        setUploadingTopic(topicId);
        setError('');
        try {
            for (const file of Array.from(selectedFiles)) {
                const params = new URLSearchParams({ filename: file.name });
                const response = await fetch(
                    `/api/math/files/${encodeURIComponent(topicId)}?${params.toString()}`,
                    {
                        method: 'POST',
                        headers: { 'Content-Type': file.type || 'application/octet-stream' },
                        body: file,
                    },
                );
                if (!response.ok) {
                    const result = await response.json() as { error?: string };
                    throw new Error(result.error || `Не удалось загрузить файл «${file.name}».`);
                }
            }
            const files = await loadTopicFiles(topicId);
            setFilesByTopic((current) => ({ ...current, [topicId]: files }));
        } catch (uploadError) {
            setError(uploadError instanceof Error
                ? uploadError.message
                : 'Не удалось загрузить файл.');
        } finally {
            setUploadingTopic(null);
        }
    };

    return (
        <section className="math-topics" aria-label="Темы и загруженные файлы">
            {error && <p className="math-topics__error" role="alert">{error}</p>}
            <div className="math-admin-table-wrap">
                <table className="math-admin-table">
                    <thead>
                        <tr>
                            <th scope="col">Тема</th>
                            <th scope="col">Загруженные файлы</th>
                            {isAdmin && (
                                <th scope="col">
                                    <span className="visually-hidden">Добавить</span>+
                                </th>
                            )}
                            <th scope="col">Плюсник</th>
                        </tr>
                    </thead>
                    <tbody>
                        {topics.map((topic) => (
                            <tr key={topic.id}>
                                <th scope="row">{topic.title}</th>
                                <td>
                                    <div className="math-admin-table__file-cell">
                                        {filesByTopic[topic.id]?.length ? (
                                            <ul className="math-admin-table__files">
                                                {filesByTopic[topic.id].map((file) => (
                                                    <li key={file.id}>
                                                        <a href={file.url} download>{file.name}</a>
                                                    </li>
                                                ))}
                                            </ul>
                                        ) : (
                                            <span className="math-admin-table__empty">
                                                Файлы пока не загружены
                                            </span>
                                        )}
                                        {isAdmin && (
                                            <>
                                                <button
                                                    className="math-admin-table__upload"
                                                    disabled={uploadingTopic === topic.id}
                                                    onClick={() => fileInputs.current[topic.id]?.click()}
                                                    type="button"
                                                >
                                                    {uploadingTopic === topic.id
                                                        ? 'Загрузка...'
                                                        : 'Загрузить файл'}
                                                </button>
                                                <input
                                                    hidden
                                                    multiple
                                                    onChange={(event) => {
                                                        void uploadFiles(topic.id, event.currentTarget.files);
                                                        event.currentTarget.value = '';
                                                    }}
                                                    ref={(element) => {
                                                        fileInputs.current[topic.id] = element;
                                                    }}
                                                    type="file"
                                                />
                                            </>
                                        )}
                                    </div>
                                </td>
                                {isAdmin && (
                                    <td>
                                        <a
                                            aria-label={`Добавить файл: ${topic.title}`}
                                            className="math-admin-table__add"
                                            href="#"
                                            onClick={(event) => event.preventDefault()}
                                        >
                                            +
                                        </a>
                                    </td>
                                )}
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
        </section>
    );
};

export default MathTopicsTable;

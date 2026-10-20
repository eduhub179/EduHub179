import React, {useEffect, useRef, useState} from 'react';
import {useLocation} from 'react-router-dom';
import {attachLessonFile, deleteHomework, deleteHomeworkFile, homeworkFileUrl} from '../api';
import {useAuth} from '../auth/AuthProvider';
import {SCHEDULE_CHANGED_EVENT} from '../hooks/useSchedule';
import type {HomeworkFile, ScheduleItem} from '../types/ScheduleTypes.ts';
import '../css/MainContent.css';

interface MainContentProps {
    selectedLesson: ScheduleItem | null;
    onCloseLesson: () => void;
    /** Called with the lesson after its homework changed (e.g. a file was attached). */
    onLessonUpdated?: (lesson: ScheduleItem) => void;
}

/** Same limit as the backend (`MAX_FILE_SIZE`): reject early instead of uploading 100+ MB for nothing. */
const MAX_FILE_SIZE_BYTES = 100 * 1024 * 1024;

const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} Б`;
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} КБ`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
};

const FilePreview: React.FC<{file: HomeworkFile}> = ({file}) => {
    if (file.mimeType === 'application/pdf') {
        return <iframe className="homework-view__file-frame" src={file.url} title={file.name}/>;
    }
    if (file.mimeType.startsWith('image/') && file.mimeType !== 'image/svg+xml') {
        return <img className="homework-view__file-image" src={file.url} alt={file.name}/>;
    }
    return (
        <p>
            Предпросмотр недоступен.{' '}
            <a href={file.url} target="_blank" rel="noreferrer">Скачать «{file.name}»</a>
        </p>
    );
};

const MainContent: React.FC<MainContentProps> = ({selectedLesson, onCloseLesson, onLessonUpdated}) => {
    const location = useLocation();
    let path = location.pathname;

    const [pdfExists, setPdfExists] = useState<boolean | null>(null);
    const [selectedFileIndex, setSelectedFileIndex] = useState(0);
    const [isUploading, setIsUploading] = useState(false);
    const [uploadError, setUploadError] = useState<string | null>(null);
    const [isConfirmingDelete, setIsConfirmingDelete] = useState(false);
    const [isDeleting, setIsDeleting] = useState(false);
    const [deletingFileId, setDeletingFileId] = useState<string | null>(null);
    const fileInputRef = useRef<HTMLInputElement>(null);
    const {user} = useAuth();
    const canAttachFiles = user?.role === 'teacher';

    let fileName = path.split('/').pop();
    if (path == "/main")
        fileName = "";
    const pdfUrl = path === '/' ? '' : `/local_docks/${fileName}`;

    useEffect(() => {
        if (fileName === '') {
            return;
        }

        setPdfExists(null);

        fetch(pdfUrl)
            .then(response => {
                const contentType = response.headers.get('content-type');

                if (
                    response.ok &&
                    contentType?.includes('application/pdf')
                ) {
                    setPdfExists(true);
                } else {
                    setPdfExists(false);
                }
            })
            .catch(() => {
                setPdfExists(false);
            });
    }, [path, pdfUrl]);

    // Only when another lesson is opened: after an upload the new file stays selected.
    useEffect(() => {
        setSelectedFileIndex(0);
        setUploadError(null);
        setIsConfirmingDelete(false);
    }, [selectedLesson?.id]);

    const handleDeleteFile = async (fileId: string) => {
        const homework = selectedLesson?.homework;
        if (!selectedLesson || !homework) {
            return;
        }
        setDeletingFileId(fileId);
        setUploadError(null);
        try {
            const {homework_deleted} = await deleteHomeworkFile(fileId);
            const files = homework.files.filter((file) => file.id !== fileId);
            onLessonUpdated?.({
                ...selectedLesson,
                homework: homework_deleted ? undefined : {...homework, files},
            });
            setSelectedFileIndex((index) => Math.min(index, Math.max(files.length - 1, 0)));
            window.dispatchEvent(new Event(SCHEDULE_CHANGED_EVENT));
        } catch (error) {
            setUploadError(error instanceof Error ? error.message : 'Не удалось удалить файл.');
        } finally {
            setDeletingFileId(null);
        }
    };

    const handleDeleteHomework = async () => {
        const homeworkId = selectedLesson?.homework?.id;
        if (!selectedLesson || !homeworkId) {
            return;
        }
        setIsDeleting(true);
        setUploadError(null);
        try {
            await deleteHomework(homeworkId);
            onLessonUpdated?.({...selectedLesson, homework: undefined});
            setIsConfirmingDelete(false);
            setSelectedFileIndex(0);
            window.dispatchEvent(new Event(SCHEDULE_CHANGED_EVENT));
        } catch (error) {
            setUploadError(error instanceof Error ? error.message : 'Не удалось удалить ДЗ.');
        } finally {
            setIsDeleting(false);
        }
    };

    const handleFilesChosen = async (fileList: FileList | null) => {
        if (!selectedLesson || !fileList?.length) {
            return;
        }
        const files = Array.from(fileList);
        const tooLarge = files.find((file) => file.size > MAX_FILE_SIZE_BYTES);
        if (tooLarge) {
            setUploadError(`Файл «${tooLarge.name}» больше 100 МБ.`);
            return;
        }

        setIsUploading(true);
        setUploadError(null);
        let lesson = selectedLesson;
        try {
            // One by one: files of one homework get consecutive positions in the list.
            for (const file of files) {
                const attached = await attachLessonFile(String(lesson.id), file);
                const homework = lesson.homework ?? {id: attached.homework_id, description: '', files: []};
                lesson = {
                    ...lesson,
                    homework: {
                        ...homework,
                        id: attached.homework_id,
                        files: [...homework.files, {
                            id: attached.file.id,
                            name: attached.file.file_name,
                            mimeType: attached.file.mime_type,
                            sizeBytes: attached.file.size_bytes,
                            url: homeworkFileUrl(attached.homework_id, attached.file.id),
                        }],
                    },
                };
                onLessonUpdated?.(lesson);
            }
            setSelectedFileIndex((lesson.homework?.files.length ?? 1) - 1);
        } catch (error) {
            setUploadError(error instanceof Error ? error.message : 'Не удалось загрузить файл.');
        } finally {
            setIsUploading(false);
            if (fileInputRef.current) {
                fileInputRef.current.value = '';
            }
            // Schedule lists reload to show the homework badge.
            window.dispatchEvent(new Event(SCHEDULE_CHANGED_EVENT));
        }
    };

    if (selectedLesson) {
        const {homework} = selectedLesson;
        const selectedFile = homework?.files[selectedFileIndex];

        return (
            <main className="main-content main-content--homework">
                <div className="homework-view">
                    <button
                        className="homework-view__close"
                        type="button"
                        onClick={onCloseLesson}
                        aria-label="Закрыть сводку урока"
                    >
                        ×
                    </button>
                    <span className="homework-view__subject">Урок {selectedLesson.lessonNumber}</span>
                    <h1 className="homework-view__title">{selectedLesson.title || 'Урок без названия'}</h1>
                    <p className="homework-view__description">
                        Время: {selectedLesson.startTime} – {selectedLesson.endTime}
                    </p>
                    <p className="homework-view__description">
                        ДЗ: {homework?.description || (homework ? 'см. файлы.' : 'Домашнего задания нет.')}
                    </p>

                    {canAttachFiles && (
                        <div className="homework-view__actions">
                            <input
                                ref={fileInputRef}
                                type="file"
                                multiple
                                hidden
                                onChange={(event) => void handleFilesChosen(event.currentTarget.files)}
                            />
                            <button
                                className="homework-view__attach"
                                type="button"
                                disabled={isUploading || isDeleting}
                                onClick={() => fileInputRef.current?.click()}
                            >
                                {isUploading ? 'Загрузка…' : 'Добавить файл'}
                            </button>
                            {homework && !isConfirmingDelete && (
                                <button
                                    className="homework-view__delete"
                                    type="button"
                                    disabled={isUploading}
                                    onClick={() => setIsConfirmingDelete(true)}
                                >
                                    Удалить ДЗ
                                </button>
                            )}
                            {homework && isConfirmingDelete && (
                                <span className="homework-view__confirm" role="group" aria-label="Подтверждение удаления">
                                    Удалить ДЗ вместе со всеми файлами?
                                    <button
                                        className="homework-view__delete"
                                        type="button"
                                        disabled={isDeleting}
                                        onClick={() => void handleDeleteHomework()}
                                    >
                                        {isDeleting ? 'Удаление…' : 'Да, удалить'}
                                    </button>
                                    <button
                                        className="homework-view__cancel"
                                        type="button"
                                        disabled={isDeleting}
                                        onClick={() => setIsConfirmingDelete(false)}
                                    >
                                        Отмена
                                    </button>
                                </span>
                            )}
                            {uploadError && (
                                <p className="homework-view__upload-error" role="alert">{uploadError}</p>
                            )}
                        </div>
                    )}

                    {homework && homework.files.length > 0 && (
                        <section className="homework-view__files" aria-label="Файлы задания">
                            <h2>Файлы</h2>
                            <div className="homework-view__file-list">
                                {homework.files.map((file, index) => (
                                    <span className="homework-view__file-item" key={file.id}>
                                        <button
                                            className={`homework-view__file${index === selectedFileIndex ? ' homework-view__file--active' : ''}`}
                                            type="button"
                                            onClick={() => setSelectedFileIndex(index)}
                                            aria-pressed={index === selectedFileIndex}
                                            title={formatFileSize(file.sizeBytes)}
                                        >
                                            {file.name}
                                        </button>
                                        {canAttachFiles && (
                                            <button
                                                className="homework-view__file-remove"
                                                type="button"
                                                disabled={deletingFileId !== null || isUploading || isDeleting}
                                                onClick={() => void handleDeleteFile(file.id)}
                                                aria-label={`Удалить файл «${file.name}»`}
                                                title="Удалить файл"
                                            >
                                                {deletingFileId === file.id ? '…' : '×'}
                                            </button>
                                        )}
                                    </span>
                                ))}
                            </div>
                            {selectedFile && (
                                <div className="homework-view__file-preview" aria-live="polite">
                                    <FilePreview file={selectedFile}/>
                                    <a
                                        className="homework-view__file-open"
                                        href={selectedFile.url}
                                        target="_blank"
                                        rel="noreferrer"
                                    >
                                        Открыть в новой вкладке
                                    </a>
                                </div>
                            )}
                        </section>
                    )}
                </div>
            </main>
        );
    }

    if (fileName === '') {
        return (
            <main className="main-content">
                <h1>Основная область</h1>
                <p>
                    Здесь может находиться любой контент: дашборд, задачи, заметки и т.д.
                </p>
            </main>
        );
    }

    if (pdfExists === null) {
        return (
            <main className="main-content">
                <p>Загрузка документа...</p>
            </main>
        );
    }

    if (!pdfExists) {
        return (
            <main className="main-content">
                <h1>Ошибка</h1>
                <p>
                    Документ "{fileName}" не найден.
                </p>
            </main>
        );
    }

    return (
        <main className="main-content main-content--pdf">
            <iframe
                src={pdfUrl}
                title={fileName ?? 'PDF document'}
                className="pdf-viewer"
            />
        </main>
    );
};

export default MainContent;
import React, {useEffect, useState} from 'react';
import {useLocation} from 'react-router-dom';
import type {ScheduleItem} from '../types/ScheduleTypes.ts';
import '../css/MainContent.css';

interface MainContentProps {
    selectedLesson: ScheduleItem | null;
    onCloseLesson: () => void;
}

const MainContent: React.FC<MainContentProps> = ({selectedLesson, onCloseLesson}) => {
    const location = useLocation();
    let path = location.pathname;

    const [pdfExists, setPdfExists] = useState<boolean | null>(null);
    const [selectedFileIndex, setSelectedFileIndex] = useState(0);

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

    useEffect(() => {
        setSelectedFileIndex(0);
    }, [selectedLesson]);

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
                        ДЗ: {homework?.description ?? 'Домашнего задания нет.'}
                    </p>

                    {homework && homework.files.length > 0 && (
                        <section className="homework-view__files" aria-label="Файлы задания">
                            <h2>Файлы</h2>
                            <div className="homework-view__file-list">
                                {homework.files.map((file, index) => (
                                    <button
                                        className={`homework-view__file${index === selectedFileIndex ? ' homework-view__file--active' : ''}`}
                                        type="button"
                                        key={file}
                                        onClick={() => setSelectedFileIndex(index)}
                                        aria-pressed={index === selectedFileIndex}
                                    >
                                        {file}
                                    </button>
                                ))}
                            </div>
                            <div className="homework-view__file-preview" aria-live="polite">
                                {selectedFile}
                            </div>
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
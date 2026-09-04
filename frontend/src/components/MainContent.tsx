import React, {useEffect, useState} from 'react';
import {useLocation} from 'react-router-dom';
import '../css/MainContent.css';

const MainContent: React.FC = () => {
    const location = useLocation();
    let path = location.pathname;

    const [pdfExists, setPdfExists] = useState<boolean | null>(null);

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
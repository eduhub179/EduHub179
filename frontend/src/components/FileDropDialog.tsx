import React, {useEffect, useId, useRef, useState} from 'react';
import {createPortal} from 'react-dom';
import '../css/FileDropDialog.css';

interface FileDropDialogProps {
    open: boolean;
    title: string;
    /** Allow several files at once. */
    multiple?: boolean;
    /** Shown under the drop zone, e.g. "PDF, фото, до 100 МБ". */
    hint?: string;
    onClose: () => void;
    /** Called with the chosen files; the dialog closes right after. */
    onFiles: (files: File[]) => void;
}

/**
 * Pop-up for adding files: drop them onto the zone or click it to open the file picker.
 * Esc, the × button and a click outside close it. While it is open, a file dropped
 * outside the zone is ignored instead of the browser navigating away to open it.
 */
const FileDropDialog: React.FC<FileDropDialogProps> = ({open, title, multiple = false, hint, onClose, onFiles}) => {
    const inputRef = useRef<HTMLInputElement>(null);
    const cameraRef = useRef<HTMLInputElement>(null);
    const zoneRef = useRef<HTMLDivElement>(null);
    const [isDragging, setIsDragging] = useState(false);
    const titleId = useId();

    useEffect(() => {
        if (!open) {
            return;
        }
        setIsDragging(false);
        zoneRef.current?.focus();

        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                event.stopPropagation();
                onClose();
            }
        };
        // Without this a file dropped next to the zone replaces the whole page.
        const preventNavigation = (event: DragEvent) => event.preventDefault();
        window.addEventListener('keydown', handleKeyDown, true);
        window.addEventListener('dragover', preventNavigation);
        window.addEventListener('drop', preventNavigation);
        return () => {
            window.removeEventListener('keydown', handleKeyDown, true);
            window.removeEventListener('dragover', preventNavigation);
            window.removeEventListener('drop', preventNavigation);
        };
    }, [open, onClose]);

    if (!open) {
        return null;
    }

    const submit = (fileList: FileList | null) => {
        const files = Array.from(fileList ?? []);
        if (!files.length) {
            return;
        }
        onFiles(multiple ? files : files.slice(0, 1));
        onClose();
    };

    const openPicker = () => inputRef.current?.click();
    // Phones and tablets: `capture` opens the camera directly instead of the gallery.
    const hasCamera = window.matchMedia('(pointer: coarse)').matches;

    // Portal to <body>: always above every page modal, independent of parent styles.
    return createPortal(
        <div className="file-drop-dialog" onClick={onClose}>
            <div
                className="file-drop-dialog__window"
                role="dialog"
                aria-modal="true"
                aria-labelledby={titleId}
                onClick={(event) => event.stopPropagation()}
            >
                <div className="file-drop-dialog__head">
                    <h2 id={titleId}>{title}</h2>
                    <button type="button" className="file-drop-dialog__close" onClick={onClose} aria-label="Закрыть">
                        ×
                    </button>
                </div>

                <div
                    ref={zoneRef}
                    className={`file-drop-dialog__zone${isDragging ? ' is-dragging' : ''}`}
                    role="button"
                    tabIndex={0}
                    aria-label={multiple ? 'Перетащите файлы или нажмите, чтобы выбрать' : 'Перетащите файл или нажмите, чтобы выбрать'}
                    onClick={openPicker}
                    onKeyDown={(event) => {
                        if (event.key === 'Enter' || event.key === ' ') {
                            event.preventDefault();
                            openPicker();
                        }
                    }}
                    onDragEnter={(event) => {
                        event.preventDefault();
                        setIsDragging(true);
                    }}
                    onDragOver={(event) => {
                        event.preventDefault();
                        event.dataTransfer.dropEffect = 'copy';
                    }}
                    onDragLeave={(event) => {
                        // Leaving into a child element is not leaving the zone.
                        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                            setIsDragging(false);
                        }
                    }}
                    onDrop={(event) => {
                        event.preventDefault();
                        setIsDragging(false);
                        submit(event.dataTransfer.files);
                    }}
                >
                    <span className="file-drop-dialog__icon" aria-hidden="true">⬆</span>
                    <strong>{isDragging ? 'Отпустите, чтобы добавить' : multiple ? 'Перетащите файлы сюда' : 'Перетащите файл сюда'}</strong>
                    <span>или нажмите, чтобы выбрать на компьютере</span>
                </div>
                {hasCamera && (
                    <button
                        type="button"
                        className="file-drop-dialog__camera"
                        onClick={() => cameraRef.current?.click()}
                    >
                        <span aria-hidden="true">📷</span> Сфотографировать
                    </button>
                )}
                {hint && <p className="file-drop-dialog__hint">{hint}</p>}

                <input
                    ref={cameraRef}
                    type="file"
                    accept="image/*"
                    capture="environment"
                    hidden
                    onChange={(event) => {
                        submit(event.currentTarget.files);
                        event.currentTarget.value = '';
                    }}
                />

                <input
                    ref={inputRef}
                    type="file"
                    multiple={multiple}
                    hidden
                    onChange={(event) => {
                        submit(event.currentTarget.files);
                        event.currentTarget.value = '';
                    }}
                />
            </div>
        </div>,
        document.body,
    );
};

export default FileDropDialog;

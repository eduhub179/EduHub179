import React, {useEffect, useState} from 'react';
import {createHomework, updateHomeworkText} from '../../api';
import {SCHEDULE_CHANGED_EVENT} from '../../hooks/useSchedule';
import type {ScheduleItem} from '../../types/ScheduleTypes.ts';

interface HomeworkTextEditorProps {
    lesson: ScheduleItem;
    onLessonUpdated?: (lesson: ScheduleItem) => void;
}

/** Teachers and admins: write or change the homework text right in the lesson card. */
const HomeworkTextEditor: React.FC<HomeworkTextEditorProps> = ({lesson, onLessonUpdated}) => {
    const [text, setText] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        setText(null);
        setError(null);
    }, [lesson.id]);

    const homework = lesson.homework;

    const save = async () => {
        if (text === null) return;
        const value = text.trim() || null;
        setBusy(true);
        setError(null);
        try {
            const saved = homework
                ? await updateHomeworkText(homework.id, {text_content: value})
                : await createHomework({
                    lesson_instance_id: String(lesson.id),
                    text_content: value,
                    status: 'published',
                });
            onLessonUpdated?.({
                ...lesson,
                homework: {id: saved.id, description: saved.text_content ?? '', files: homework?.files ?? []},
            });
            setText(null);
            window.dispatchEvent(new Event(SCHEDULE_CHANGED_EVENT));
        } catch (saveError) {
            setError(saveError instanceof Error ? saveError.message : 'Не удалось сохранить ДЗ.');
        } finally {
            setBusy(false);
        }
    };

    if (text === null) {
        return (
            <button
                className="homework-view__cancel"
                type="button"
                onClick={() => setText(homework?.description ?? '')}
            >
                {homework?.description ? 'Изменить текст ДЗ' : 'Написать ДЗ'}
            </button>
        );
    }

    return (
        <div className="homework-view__text-editor">
            <textarea
                value={text}
                rows={4}
                autoFocus
                placeholder="Например: № 214–218, параграф 12"
                onChange={(event) => setText(event.target.value)}
            />
            <div className="homework-view__actions">
                <button className="homework-view__attach" type="button" disabled={busy || (!homework && !text.trim())} onClick={() => void save()}>
                    {busy ? 'Сохранение…' : 'Сохранить'}
                </button>
                <button className="homework-view__cancel" type="button" disabled={busy} onClick={() => setText(null)}>
                    Отмена
                </button>
            </div>
            {error && <p className="homework-view__upload-error" role="alert">{error}</p>}
        </div>
    );
};

export default HomeworkTextEditor;

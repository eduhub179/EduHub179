import React, {useEffect} from 'react';
import MainContent from '../MainContent';
import type {ScheduleItem} from '../../types/ScheduleTypes';
import '../../css/FullSchedule.css';

interface LessonModalProps {
    lesson: ScheduleItem | null;
    onClose: () => void;
    onLessonUpdated: (lesson: ScheduleItem) => void;
}

/** The lesson card (homework, files, admin actions) over the page; Esc or a click outside closes it. */
const LessonModal: React.FC<LessonModalProps> = ({lesson, onClose, onLessonUpdated}) => {
    useEffect(() => {
        if (!lesson) return;
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') onClose();
        };
        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [lesson, onClose]);

    if (!lesson) return null;

    return (
        <div className="schedule-lesson-modal" onClick={onClose}>
            <div
                className="schedule-lesson-modal__dialog"
                role="dialog"
                aria-modal="true"
                aria-label={`Урок ${lesson.lessonNumber}: ${lesson.title}`}
                onClick={(event) => event.stopPropagation()}
            >
                <MainContent selectedLesson={lesson} onCloseLesson={onClose} onLessonUpdated={onLessonUpdated}/>
            </div>
        </div>
    );
};

export default LessonModal;

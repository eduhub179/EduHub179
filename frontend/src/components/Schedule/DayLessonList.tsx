import React from 'react';
import {changeLabel, type ScheduleItem} from '../../types/ScheduleTypes';
import '../../css/DayLessonList.css';

interface DayLessonListProps {
    lessons: ScheduleItem[];
    onLessonSelect: (lesson: ScheduleItem) => void;
    /** Teacher view: each lesson says whether homework is set and offers to set it. */
    homeworkActions?: boolean;
    /** The day is today: mark the lesson going on now and the next one. */
    markNow?: boolean;
    emptyText?: string;
}

const nowHHMM = () => {
    const now = new Date();
    return `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
};

const homeworkPreview = (lesson: ScheduleItem): string => {
    const homework = lesson.homework;
    if (!homework) return '';
    const text = homework.description.trim();
    if (text) return text;
    const files = homework.files.length;
    return files === 1 ? '1 файл' : `${files} файл${files >= 2 && files <= 4 ? 'а' : 'ов'}`;
};

/** Lessons of one day as a list of cards, in time order. */
const DayLessonList: React.FC<DayLessonListProps> = ({
    lessons,
    onLessonSelect,
    homeworkActions = false,
    markNow = false,
    emptyText = 'В этот день уроков нет.',
}) => {
    const sorted = [...lessons].sort((first, second) => first.startTime.localeCompare(second.startTime));
    if (!sorted.length) {
        return <p className="day-lessons__empty">{emptyText}</p>;
    }

    const now = nowHHMM();
    const currentId = markNow
        ? sorted.find((lesson) => !lesson.cancelled && lesson.startTime <= now && now < lesson.endTime)?.id
        : undefined;
    const nextId = markNow && !currentId
        ? sorted.find((lesson) => !lesson.cancelled && lesson.startTime > now)?.id
        : undefined;

    return (
        <ol className="day-lessons">
            {sorted.map((lesson) => {
                const badge = changeLabel(lesson);
                const meta = [
                    lesson.targetName,
                    lesson.cabinet?.trim() && `каб. ${lesson.cabinet.trim()}`,
                    lesson.teachers?.filter((teacher) => teacher.trim()).join(', '),
                ].filter(Boolean).join(' · ');
                const state = lesson.id === currentId ? 'now' : lesson.id === nextId ? 'next' : null;

                return (
                    <li key={lesson.id}>
                        <button
                            className={[
                                'day-lessons__card',
                                lesson.cancelled && 'day-lessons__card--cancelled',
                                badge && 'day-lessons__card--changed',
                                state && `day-lessons__card--${state}`,
                            ].filter(Boolean).join(' ')}
                            onClick={() => onLessonSelect(lesson)}
                            type="button"
                        >
                            <span className="day-lessons__time">
                                <span>{lesson.startTime}</span>
                                <span>{lesson.endTime}</span>
                            </span>
                            <span className="day-lessons__body">
                                <span className="day-lessons__title">
                                    {lesson.title}
                                    {state === 'now' && <span className="day-lessons__badge day-lessons__badge--now">идёт сейчас</span>}
                                    {state === 'next' && <span className="day-lessons__badge day-lessons__badge--next">следующий</span>}
                                    {lesson.cancelled && <span className="day-lessons__badge day-lessons__badge--cancelled">отменён</span>}
                                    {badge && <span className="day-lessons__badge">{badge}</span>}
                                </span>
                                {meta && <span className="day-lessons__meta">{meta}</span>}
                                {lesson.note && <span className="day-lessons__meta">{lesson.note}</span>}
                                {lesson.homework && (
                                    <span className="day-lessons__homework">ДЗ: {homeworkPreview(lesson)}</span>
                                )}
                            </span>
                            {homeworkActions && (
                                lesson.homework ? (
                                    <span className="day-lessons__action day-lessons__action--done">
                                        ДЗ задано
                                    </span>
                                ) : (
                                    <span className="day-lessons__action">Задать ДЗ</span>
                                )
                            )}
                        </button>
                    </li>
                );
            })}
        </ol>
    );
};

export default DayLessonList;

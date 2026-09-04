import type {DaySchedule, ScheduleItem} from '../types/ScheduleTypes.ts';

const lessonTimes = [
    ['09:00', '09:45'],
    ['09:55', '10:40'],
    ['11:00', '11:45'],
    ['11:55', '12:40'],
    ['13:10', '13:55'],
    ['14:15', '15:00'],
    ['15:10', '15:55'],
] as const;

const createItems = (titles: string[]): ScheduleItem[] => titles.map((title, index) => ({
    id: index + 1,
    startTime: lessonTimes[index][0],
    endTime: lessonTimes[index][1],
    lessonNumber: index + 1,
    title,
}));

const createDay = (
    name: string,
    shortName: string,
    titles: string[],
): DaySchedule => ({
    name,
    shortName,
    items: createItems(titles),
});

export const weekSchedule: DaySchedule[] = [
    createDay('Понедельник', 'Пн', [
        'Важное',
        'Зачёт',
        'Зачёт',
        'Биология',
        'Биология',
        'Астрономия',
        'Химия',
    ]),
    createDay('Вторник', 'Вт', [
        'Русский язык',
        'Русский язык',
        'Физика',
        'Физика',
        'Английский язык',
        'Английский язык',
        'Танцы',
    ]),
    createDay('Среда', 'Ср', [
        'Информатика',
        'Информатика',
        'Геометрия',
        'Английский язык',
        'ОБЗР',
        'Проект',
        'Проект',
    ]),
    createDay('Четверг', 'Чт', [
        'Английский язык',
        'Английский язык',
        'Физика',
        'Физика',
        'Геометрия',
        'Геометрия',
        'Литература',
    ]),
    createDay('Пятница', 'Пт', [
        'Литература',
        'Литература',
        'Зачёт',
        'Зачёт',
        'Физкультура',
        'Физкультура',
        'Английский язык',
    ]),
    createDay('Суббота', 'Сб', [
        'География',
        'География',
        'История',
        'История',
        '',
        '',
        '',
    ]),
];

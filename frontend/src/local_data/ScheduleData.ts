import type {DaySchedule, Homework, ScheduleItem} from '../types/ScheduleTypes.ts';

const lessonTimes = [
    ['09:00', '09:45'],
    ['09:55', '10:40'],
    ['11:00', '11:45'],
    ['11:55', '12:40'],
    ['13:10', '13:55'],
    ['14:15', '15:00'],
    ['15:10', '15:55'],
] as const;

const makeHomework = (description: string, files: string[]): Homework => ({
    description,
    files,
});

const createItems = (
    titles: string[],
    homeworkByLesson: Partial<Record<number, Homework>> = {},
): ScheduleItem[] => titles.map((title, index) => {
    const homework = homeworkByLesson[index + 1];

    return {
        id: index + 1,
        startTime: lessonTimes[index][0],
        endTime: lessonTimes[index][1],
        lessonNumber: index + 1,
        title,
        ...(homework ? {homework} : {}),
    };
});

const createDay = (
    name: string,
    shortName: string,
    titles: string[],
    homeworkByLesson?: Partial<Record<number, Homework>>,
): DaySchedule => ({
    name,
    shortName,
    items: createItems(titles, homeworkByLesson),
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
    ], {
        1: makeHomework('Подготовить сообщение о том, как клетки получают энергию.', ['cell_energy_notes.pdf', 'biology_diagram_v2.png']),
        4: makeHomework('Прочитать § 12 и выписать функции кровеносной системы.', ['bio_ch12_scan.pdf']),
        5: makeHomework('Нарисовать схему пищевой цепи из 4–5 организмов.', ['food_chain_example.jpg', 'ecosystem_task.docx']),
        7: makeHomework('Выучить названия и свойства трёх химических элементов.', ['elements_cards.pdf']),
    }),
    createDay('Вторник', 'Вт', [
        'Русский язык',
        'Русский язык',
        'Физика',
        'Физика',
        'Английский язык',
        'Английский язык',
        'Танцы',
    ], {
        1: makeHomework('Выполнить упражнение 48: вставить пропущенные окончания.', ['russian_ex48_sheet.pdf']),
        2: makeHomework('Разобрать по составу слова из упражнения 52.', ['word_analysis_list.png', 'lesson_52_extra.pdf']),
        3: makeHomework('Решить задачи 214 и 216 из учебника.', ['physics_problem_set_214.pdf']),
        5: makeHomework('Выучить слова Unit 4 и составить с ними 5 предложений.', ['unit4_vocabulary.pdf', 'english_wordlist.png']),
        6: makeHomework('Написать короткий рассказ о любимом фильме на английском.', ['my_favorite_movie_template.docx']),
    }),
    createDay('Среда', 'Ср', [
        'Информатика',
        'Информатика',
        'Геометрия',
        'Английский язык',
        'ОБЗР',
        'Проект',
        'Проект',
    ], {
        1: makeHomework('Составить алгоритм поиска наибольшего из трёх чисел.', ['max_number_flowchart.png', 'informatics_task_07.pdf']),
        2: makeHomework('Решить 3 задачи на условные операторы в рабочей тетради.', ['conditions_practice.pdf']),
        3: makeHomework('Построить чертёж и решить задачу 308.', ['geometry_308_reference.jpg']),
        4: makeHomework('Написать 8 предложений о своём распорядке дня.', ['daily_routine_vocab.pdf', 'speaking_prompt_card.png']),
        6: makeHomework('Подготовить план презентации проекта из пяти пунктов.', ['project_outline_sample.docx']),
    }),
    createDay('Четверг', 'Чт', [
        'Английский язык',
        'Английский язык',
        'Физика',
        'Физика',
        'Геометрия',
        'Геометрия',
        'Литература',
    ], {
        1: makeHomework('Повторить неправильные глаголы из списка на странице 36.', ['irregular_verbs_p36.pdf']),
        2: makeHomework('Выполнить задания 3 и 4 в рабочей тетради.', ['english_workbook_unit5.png', 'grammar_review.pdf']),
        3: makeHomework('Решить задачи 417 и 419.', ['physics_chapter_tasks_4.pdf']),
        5: makeHomework('Доказать теорему из § 8 и оформить чертёж.', ['geometry_theorem_8.png']),
        7: makeHomework('Прочитать рассказ до конца и выписать главную мысль.', ['reading_notes_template.docx', 'story_excerpt_scan.pdf']),
    }),
    createDay('Пятница', 'Пт', [
        'Литература',
        'Литература',
        'Зачёт',
        'Зачёт',
        'Физкультура',
        'Физкультура',
        'Английский язык',
    ], {
        1: makeHomework('Прочитать главу 5 и отметить незнакомые слова.', ['literature_ch5.pdf']),
        2: makeHomework('Подготовить характеристику главного героя (5–7 предложений).', ['character_profile_guide.png', 'literature_quote_cards.pdf']),
        3: makeHomework('Повторить вопросы к зачёту по теме «Средневековье».', ['history_review_questions.pdf']),
        7: makeHomework('Написать письмо другу о планах на выходные.', ['informal_letter_layout.docx']),
    }),
    createDay('Суббота', 'Сб', [
        'География',
        'География',
        'История',
        'История',
        '',
        '',
        '',
    ], {
        1: makeHomework('Отметить на контурной карте крупные реки России.', ['russia_rivers_blank_map.png', 'geography_map_worksheet.pdf']),
        2: makeHomework('Выучить столицы стран Южной Америки.', ['south_america_capitals.pdf']),
        3: makeHomework('Составить хронологическую таблицу по теме урока.', ['history_timeline_template.docx']),
        4: makeHomework('Подготовить три интересных факта о Древнем Египте.', ['ancient_egypt_sources.pdf', 'egypt_facts_reference.jpg']),
    }),
];

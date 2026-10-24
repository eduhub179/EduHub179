/** Russian texts for the backend's error messages. */
const errorTexts: Record<string, string> = {
    'Insufficient permissions': 'Нет прав: нужен вход под администратором.',
    'Schedule week not found': 'Такой недели расписания нет. Её можно создать во вкладке «Недели расписания».',
    'Schedule week already exists': 'Такая неделя уже есть.',
    'The week already has lessons': 'В этой неделе уже есть уроки.',
    'Cabinet already exists': 'Такой кабинет уже есть.',
    'Cabinet number must be 100-999': 'Номер кабинета — от 100 до 999.',
    'Login already exists': 'Такой логин уже есть.',
    'Password must be 8-128 characters': 'Пароль — от 8 до 128 символов.',
    'Subject already exists': 'Такой предмет уже есть.',
    'Subject name is required': 'Введите название предмета.',
    'This class or group already has this subject': 'У этого класса (группы) уже есть такой предмет.',
    'This course already has a slot at this time': 'Этот предмет уже стоит в расписании в это время.',
    'This course already has an overlapping slot': 'Это время пересекается с другим уроком того же предмета.',
    'This lesson is already on that date': 'Этот урок уже стоит в это время.',
    'End time must be after start time': 'Конец урока должен быть позже начала.',
    'There are no lessons on Sunday': 'В воскресенье уроков нет.',
    'Teacher not found': 'Учитель не найден (ученика учителем назначить нельзя).',
    'Teacher account is inactive': 'Учётная запись учителя отключена.',
    'Slot not found': 'Урок в расписании не найден — обновите страницу.',
    'Lesson not found': 'Урок не найден — обновите страницу.',
};

export const errorText = (error: unknown, fallback = 'Что-то пошло не так.'): string => {
    if (!(error instanceof Error)) return fallback;
    return errorTexts[error.message] ?? error.message;
};

/** `YYYY-MM-DD` of a local date. */
export const isoDate = (date: Date): string => {
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    return `${date.getFullYear()}-${month}-${day}`;
};

/** Monday of the week containing `date`, as `YYYY-MM-DD`. */
export const mondayOf = (date: Date): string => {
    const copy = new Date(date);
    const shift = (copy.getDay() + 6) % 7;
    copy.setDate(copy.getDate() - shift);
    return isoDate(copy);
};

/** "пн, 19 окт." */
export const formatDay = (iso: string): string =>
    new Date(`${iso}T00:00:00`).toLocaleDateString('ru-RU', {
        weekday: 'short',
        day: 'numeric',
        month: 'short',
    });

export const roleTitles: Record<string, string> = {
    student: 'Ученик',
    teacher: 'Учитель',
    admin: 'Админ',
};

/** Teachers and admins without the chat agent's service account (login `agent`). */
export const staffOnly = <T extends {login: string}>(users: T[]): T[] =>
    users.filter((user) => user.login !== 'agent');

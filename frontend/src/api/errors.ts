/**
 * The only place that turns a failed request into the text a person sees.
 *
 * The backend answers `{"error": "..."}` mostly in English and sometimes with technical
 * details; axum and nginx answer plain text or HTML. None of that is shown as is:
 * - a known backend message → its Russian wording below;
 * - a message the backend already words for people (Russian, e.g. sign-in) → as is;
 * - anything else → a short message for the HTTP status.
 */

/** Backend messages a person can act on, in Russian. Internal ones are left out on purpose. */
const KNOWN: Record<string, string> = {
    // Access
    'Authentication required': 'Войдите в аккаунт, чтобы продолжить.',
    'Insufficient permissions': 'У вас нет прав на это действие.',
    'User account is inactive': 'Аккаунт отключён. Обратитесь к администратору.',
    'Teacher account is inactive': 'Аккаунт учителя отключён.',
    'Invalid login or password': 'Неверная почта или пароль.',
    'Invalid email format': 'Введите школьную почту (@179.ru).',
    'Invalid login format': 'Неверный формат логина.',
    'Invalid or expired auth code': 'Неверный или устаревший код.',
    'Telegram account is not linked': 'Telegram не привязан к аккаунту.',
    'Password must be 8-128 characters': 'Пароль должен быть от 8 до 128 символов.',

    // Not found
    'Lesson not found': 'Урок не найден.',
    'Lesson instance not found': 'Урок не найден.',
    'Homework not found': 'Домашнее задание не найдено.',
    'File not found': 'Файл не найден.',
    'Class not found': 'Класс не найден.',
    'Student not found': 'Ученик не найден.',
    'Teacher not found': 'Учитель не найден.',
    'User not found': 'Пользователь не найден.',
    'Course not found': 'Курс не найден.',
    'Subject not found': 'Предмет не найден.',
    'Group not found': 'Группа не найдена.',
    'Cabinet not found': 'Кабинет не найден.',
    'Slot not found': 'Урок в расписании не найден.',
    'Sheet not found': 'Листок не найден.',
    'Task not found': 'Задача не найдена.',
    'Topic not found': 'Тема не найдена.',
    'Material not found': 'Материал не найден.',
    'Announcement not found': 'Объявление не найдено.',
    'Schedule week not found': 'Расписание на эту неделю ещё не опубликовано.',

    // Homework and files
    'Homework already exists for this lesson': 'У этого урока уже есть домашнее задание.',
    'Invalid homework text format': 'Текст задания не должен быть пустым.',
    'File is empty or larger than 100 MB': 'Файл пустой или больше 100 МБ.',
    'Invalid file name or type': 'Недопустимое имя или тип файла.',
    'File storage is not configured': 'Загрузка файлов сейчас недоступна.',
    'This material is a link, there is no file': 'Этот материал — ссылка, файла у него нет.',
    'Link must be an http(s) URL, at most 2000 characters': 'Ссылка должна начинаться с http:// или https:// и быть не длиннее 2000 символов.',
    'Title must be 1-255 characters, description at most 5000': 'Название — до 255 символов, описание — до 5000.',
    'Title must be at most 255 characters': 'Название — не длиннее 255 символов.',
    'Topic name must be 1-255 characters': 'Название темы — от 1 до 255 символов.',
    'The course already has a topic with this name': 'В курсе уже есть тема с таким названием.',

    // Plusnik
    'Sheet name must be 1-255 characters': 'Название листка — от 1 до 255 символов.',
    'Task number must be 1-20 characters': 'Номер задачи — от 1 до 20 символов.',
    'Status must be draft, published or archived': 'Неизвестный статус листка.',
    'The sheet already has a task with this number': 'В листке уже есть задача с таким номером.',
    'The sheet has pluses and cannot be deleted; archive it instead': 'В листке уже есть плюсы, удалить его нельзя. Переведите его в архив.',
    'The task has pluses and cannot be deleted': 'За эту задачу уже ставили плюсы, удалить её нельзя.',
    'Pluses can be given only on a published sheet': 'Плюсы можно ставить только в опубликованном листке.',
    'The student is not in this lesson': 'Этот ученик не относится к курсу.',
    'The task is not on this sheet': 'Задача не из этого листка.',
    'The order must list every task of the sheet exactly once': 'Не удалось изменить порядок задач. Обновите страницу.',
    'The batch is empty, too large, or grants and revokes the same cell': 'Не удалось сохранить изменения плюсов. Обновите страницу.',

    // Admin: users, weeks, timetable, announcements
    'Login already exists': 'Пользователь с таким логином уже есть.',
    "Login: latin letters, digits, '.', '_', '-' (the part of the email before @)": 'Логин — латинские буквы, цифры, «.», «_», «-» (часть почты до @).',
    'Last and first name are required': 'Укажите фамилию и имя.',
    'Role must be student, teacher or admin': 'Неизвестная роль.',
    'Invalid role format': 'Неизвестная роль.',
    'Schedule week already exists': 'Такая неделя уже создана.',
    'The week already has lessons': 'В этой неделе уже есть уроки.',
    'A week must start on a Monday': 'Неделя должна начинаться с понедельника.',
    'There are no lessons on Sunday': 'По воскресеньям уроков нет.',
    'Invalid date, expected YYYY-MM-DD': 'Неверная дата.',
    'Invalid lesson date': 'Неверная дата урока.',
    'Invalid time, expected HH:MM': 'Неверное время.',
    'End time must be after start time': 'Урок должен заканчиваться позже, чем начинается.',
    'This course already has a slot at this time': 'У этого курса уже есть урок в это время.',
    'This course already has an overlapping slot': 'У этого курса уже есть урок, который пересекается по времени.',
    'This lesson is already on that date': 'Этот урок уже стоит на эту дату.',
    'This class or group already has this subject': 'У этого класса или группы уже есть такой предмет.',
    'Subject already exists': 'Такой предмет уже есть.',
    'Subject name is required': 'Укажите название предмета.',
    'Cabinet already exists': 'Такой кабинет уже есть.',
    'Cabinet number must be 100-999': 'Номер кабинета — от 100 до 999.',
    'Cabinet description is too long': 'Слишком длинное описание кабинета.',
    'Note must be at most 500 characters': 'Заметка — не длиннее 500 символов.',
    'Announcement needs a title or text': 'У объявления должен быть заголовок или текст.',

    // Notifications
    'Push notifications are not configured on this server': 'Уведомления на этом сервере не настроены.',
    'Invalid push subscription': 'Не удалось подписаться на уведомления. Попробуйте ещё раз.',
};

/** When there is no better text: by HTTP status. */
const BY_STATUS: Record<number, string> = {
    400: 'Не удалось выполнить действие. Проверьте введённые данные.',
    401: 'Войдите в аккаунт, чтобы продолжить.',
    403: 'У вас нет прав на это действие.',
    404: 'Не найдено. Возможно, это уже удалили.',
    409: 'Действие сейчас невозможно. Обновите страницу и попробуйте снова.',
    413: 'Файл слишком большой.',
    415: 'Этот тип файла не поддерживается.',
    422: 'Проверьте введённые данные.',
    429: 'Слишком много попыток. Подождите немного и попробуйте снова.',
};

const SERVER_ERROR = 'Ошибка сервера. Попробуйте позже.';
export const NETWORK_ERROR = 'Нет связи с сервером. Проверьте интернет и попробуйте снова.';
export const UNEXPECTED_ERROR = 'Что-то пошло не так. Попробуйте ещё раз.';

/** Russian text written for people (sign-in messages, "next lesson not published"). */
const isHumanRussian = (text: string): boolean =>
    /[А-Яа-яЁё]/.test(text) && !/[<>{}]|not initialized|Error:/i.test(text);

/** The message to show for a failed response; `backendText` is its `error` field, if any. */
export function userMessage(status: number, backendText?: string | null): string {
    const text = backendText?.trim();
    if (text) {
        if (KNOWN[text]) return KNOWN[text];
        if (isHumanRussian(text)) return text;
    }
    if (status >= 500) return SERVER_ERROR;
    return BY_STATUS[status] ?? UNEXPECTED_ERROR;
}

/** What `apiFetch` throws: `message` is for people, the rest is for code. */
export class ApiError extends Error {
    readonly status: number;
    /** The backend's own `error` text, never shown as is (e.g. for a more specific wording). */
    readonly backendText: string | null;

    constructor(message: string, status: number, backendText: string | null = null) {
        super(message);
        this.name = 'ApiError';
        this.status = status;
        this.backendText = backendText;
    }
}

/** Shown when the week asked for is not created or not published. */
export const WEEK_NOT_PUBLISHED = KNOWN['Schedule week not found'];

/**
 * Text for any caught error: its message if it is written for people (every `apiFetch`
 * error is), otherwise `fallback` — a browser's or a bug's own text is never shown.
 */
export function errorText(error: unknown, fallback: string): string {
    if (error instanceof Error && error.message && isHumanRussian(error.message)) {
        return error.message;
    }
    return fallback;
}

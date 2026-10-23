export const errorText = (error: unknown, fallback = 'Что-то пошло не так.'): string =>
    error instanceof Error ? error.message : fallback;

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

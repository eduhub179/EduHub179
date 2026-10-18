export interface Homework {
    description: string;
    files: string[];
}

export interface ScheduleItem {
    id: string | number;
    startTime: string;
    endTime: string;
    lessonNumber: number;
    title: string;
    homework?: Homework;
}

export interface DaySchedule {
    name: string;
    shortName: string;
    items: ScheduleItem[];
}
export interface ScheduleItem {
    id: number;
    startTime: string;
    endTime: string;
    lessonNumber: number;
    title: string;
}

export interface DaySchedule {
    name: string;
    shortName: string;
    items: ScheduleItem[];
}
export interface HomeworkFile {
    id: string;
    name: string;
    mimeType: string;
    sizeBytes: number;
    /** Opens/downloads the file; the session cookie authorizes the request. */
    url: string;
}

export interface Homework {
    id: string;
    /** Empty for file-only homework. */
    description: string;
    files: HomeworkFile[];
}

export interface ScheduleItem {
    id: string | number;
    startTime: string;
    endTime: string;
    lessonNumber: number;
    title: string;
    teachers?: string[];
    cabinet?: string | null;
    homework?: Homework;
}

export interface DaySchedule {
    name: string;
    shortName: string;
    items: ScheduleItem[];
}
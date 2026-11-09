//! FILE: view.rs
//! PURPOSE: Turns schedule data from `ScheduleService` into plain-text Telegram messages.
//! DEPENDENCIES: `application` (schedule DTOs), `domain` (statuses), `chrono`.
//!
//! Pure functions only: no I/O, so the texts are covered by unit tests.
//! Messages are plain text (no parse mode), so names and homework need no escaping.

use application::use_cases::schedule::schedule::{ScheduleDay, ScheduleLesson};
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc, Weekday};
use domain::value_objects::homework_status::HomeworkStatus;
use domain::value_objects::lesson_instance_status::LessonInstanceStatus;

/// Moscow time (no DST since 2014). The server runs in UTC.
pub fn moscow_now() -> NaiveDateTime {
    let msk = FixedOffset::east_opt(3 * 3600).expect("valid offset");
    Utc::now().with_timezone(&msk).naive_local()
}

/// The next day with lessons after `date`: Sunday is skipped.
pub fn next_school_day(date: NaiveDate) -> NaiveDate {
    let next = date + Duration::days(1);
    if next.weekday() == Weekday::Sun {
        next + Duration::days(1)
    } else {
        next
    }
}

/// Which day `/schedule` without arguments shows: today until 16:00, then the next
/// school day; on Sunday always Monday.
pub fn default_schedule_day(now: NaiveDateTime) -> NaiveDate {
    let today = now.date();
    if today.weekday() == Weekday::Sun || now.hour() >= 16 {
        next_school_day(today)
    } else {
        today
    }
}

/// "понедельник, 10 ноября" (+ "сегодня"/"завтра" when it applies).
pub fn day_title(date: NaiveDate, today: NaiveDate) -> String {
    const DAYS: [&str; 7] = [
        "понедельник",
        "вторник",
        "среда",
        "четверг",
        "пятница",
        "суббота",
        "воскресенье",
    ];
    const MONTHS: [&str; 12] = [
        "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября",
        "октября", "ноября", "декабря",
    ];
    let base = format!(
        "{}, {} {}",
        DAYS[date.weekday().num_days_from_monday() as usize],
        date.day(),
        MONTHS[date.month0() as usize]
    );
    if date == today {
        format!("Сегодня, {base}")
    } else if date == today + Duration::days(1) {
        format!("Завтра, {base}")
    } else {
        capitalize(&base)
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Lessons to show: a cancelled lesson is hidden when another lesson takes its time
/// (the admin panel replaced it), as on the website.
pub fn visible_lessons(day: &ScheduleDay) -> Vec<&ScheduleLesson> {
    day.lessons
        .iter()
        .filter(|l| {
            l.instance.status != LessonInstanceStatus::Cancelled
                || !day.lessons.iter().any(|other| {
                    other.instance.id != l.instance.id
                        && other.instance.status != LessonInstanceStatus::Cancelled
                        && other.start_time == l.start_time
                })
        })
        .collect()
}

/// Homework text a student may see: published and not empty.
pub fn homework_text(lesson: &ScheduleLesson) -> Option<String> {
    let details = lesson.homework.as_ref()?;
    if details.homework.status != HomeworkStatus::Published {
        return None;
    }
    let text = details.homework.text_content.as_deref().unwrap_or("").trim();
    let files = details.files.len();
    match (text.is_empty(), files) {
        (true, 0) => None,
        (true, n) => Some(format!("📎 файлов: {n} (смотри на сайте)")),
        (false, 0) => Some(text.to_string()),
        (false, n) => Some(format!("{text}\n📎 файлов: {n}")),
    }
}

fn teacher_names(lesson: &ScheduleLesson) -> String {
    let short = |u: &domain::entities::user::User| {
        let initial = u.first_name.chars().next().map(|c| format!(" {c}.")).unwrap_or_default();
        format!("{}{}", u.last_name, initial)
    };
    if let Some(sub) = &lesson.substitute_teacher {
        return format!("замена: {}", short(sub));
    }
    lesson.teachers.iter().map(short).collect::<Vec<_>>().join(", ")
}

/// One day of lessons. `with_target` adds the class/group name (teacher's view).
pub fn format_day(day: &ScheduleDay, today: NaiveDate, with_target: bool) -> String {
    let mut out = format!("📅 {}\n", day_title(day.date, today));
    let lessons = visible_lessons(day);
    if lessons.is_empty() {
        out.push_str("Уроков нет.\n");
        return out;
    }
    for l in lessons {
        let time = format!(
            "{:02}:{:02}–{:02}:{:02}",
            l.start_time.hour(),
            l.start_time.minute(),
            l.end_time.hour(),
            l.end_time.minute()
        );
        let mut line = format!("\n{time}  {}", l.subject.name);
        if with_target {
            line.push_str(&format!(" · {}", l.target_name));
        }
        if let Some(cab) = &l.cabinet {
            line.push_str(&format!(" · каб. {}", cab.number));
        }
        if l.instance.status == LessonInstanceStatus::Cancelled {
            line = format!("\n❌ {time}  {} — отменён", l.subject.name);
            out.push_str(&line);
            out.push('\n');
            continue;
        }
        out.push_str(&line);
        out.push('\n');
        let teachers = teacher_names(l);
        if !teachers.is_empty() {
            out.push_str(&format!("   👤 {teachers}\n"));
        }
        if let Some(note) = l.substitution.as_ref().and_then(|s| s.note.as_deref()) {
            if !note.trim().is_empty() {
                out.push_str(&format!("   ⚠️ {}\n", note.trim()));
            }
        }
        if l.is_extra {
            out.push_str("   ➕ добавлен в расписание\n");
        }
        if !with_target {
            if let Some(hw) = homework_text(l) {
                out.push_str(&format!("   📚 {}\n", hw.replace('\n', "\n      ")));
            }
        }
    }
    out
}

/// Homework of the given days, grouped by day. Days without homework are skipped.
pub fn format_homework(days: &[ScheduleDay], today: NaiveDate) -> String {
    let mut out = String::new();
    for day in days {
        let items: Vec<(String, String)> = visible_lessons(day)
            .into_iter()
            .filter(|l| l.instance.status != LessonInstanceStatus::Cancelled)
            .filter_map(|l| homework_text(l).map(|hw| (l.subject.name.clone(), hw)))
            .collect();
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("📅 {}\n", day_title(day.date, today)));
        for (subject, hw) in items {
            out.push_str(&format!("• {subject}: {}\n", hw.replace('\n', "\n   ")));
        }
        out.push('\n');
    }
    if out.is_empty() {
        "📚 Домашки пока нет.".to_string()
    } else {
        format!("📚 Домашнее задание\n\n{}", out.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn sunday_is_skipped() {
        // 2026-11-07 is Saturday.
        assert_eq!(next_school_day(d(2026, 11, 7)), d(2026, 11, 9));
        assert_eq!(next_school_day(d(2026, 11, 9)), d(2026, 11, 10));
    }

    #[test]
    fn evening_shows_next_day() {
        let morning = d(2026, 11, 9).and_time(NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let evening = d(2026, 11, 9).and_time(NaiveTime::from_hms_opt(18, 0, 0).unwrap());
        let sunday = d(2026, 11, 8).and_time(NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(default_schedule_day(morning), d(2026, 11, 9));
        assert_eq!(default_schedule_day(evening), d(2026, 11, 10));
        assert_eq!(default_schedule_day(sunday), d(2026, 11, 9));
    }

    #[test]
    fn titles() {
        let today = d(2026, 11, 9);
        assert_eq!(day_title(today, today), "Сегодня, понедельник, 9 ноября");
        assert_eq!(day_title(d(2026, 11, 10), today), "Завтра, вторник, 10 ноября");
        assert_eq!(day_title(d(2026, 11, 13), today), "Пятница, 13 ноября");
    }

    #[test]
    fn empty_day_and_no_homework() {
        let day = ScheduleDay { date: d(2026, 11, 9), lessons: vec![] };
        assert!(format_day(&day, d(2026, 11, 9), false).contains("Уроков нет"));
        assert_eq!(format_homework(&[day], d(2026, 11, 9)), "📚 Домашки пока нет.");
    }
}

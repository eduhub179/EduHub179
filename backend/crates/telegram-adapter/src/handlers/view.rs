//! FILE: view.rs
//! PURPOSE: Turns schedule data from `ScheduleService` into plain-text Telegram messages.
//! DEPENDENCIES: `application` (schedule DTOs), `domain` (statuses), `chrono`.
//!
//! Pure functions only: no I/O, so the texts are covered by unit tests.
//! Messages are sent with `ParseMode::Html`: every text from the database goes
//! through `esc`. A double lesson (two lessons of the same subject in a row) is
//! shown as one line, and its homework is taken from either lesson, so a homework
//! written only on the first lesson of the pair is still shown once for the pair.

use application::use_cases::schedule::schedule::{ScheduleDay, ScheduleLesson};
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc, Weekday};
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

/// School 179 bells: start of lesson 1..=10 (the same for every class).
const BELLS: [(u32, u32); 10] = [
    (9, 0),
    (9, 55),
    (11, 0),
    (11, 55),
    (13, 10),
    (14, 15),
    (15, 10),
    (16, 5),
    (16, 55),
    (17, 45),
];

/// Lesson number by its start time; `None` for a time off the bells (an extra lesson).
pub fn lesson_number(start: NaiveTime) -> Option<u32> {
    BELLS
        .iter()
        .position(|&(h, m)| start.hour() == h && start.minute() == m)
        .map(|i| i as u32 + 1)
}

/// Escapes text for `ParseMode::Html`.
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn hhmm(t: NaiveTime) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

/// "2027б" -> "11Б" in the school year of `today`; a group name stays as it is.
pub fn class_label(target: &str, today: NaiveDate) -> String {
    let digits: String = target.chars().take_while(|c| c.is_ascii_digit()).collect();
    let letter: String = target.chars().skip_while(|c| c.is_ascii_digit()).collect();
    let (Ok(graduation), true) = (digits.parse::<i32>(), digits.len() == 4 && letter.chars().count() == 1) else {
        return target.to_string();
    };
    let year_end = if today.month() >= 7 { today.year() + 1 } else { today.year() };
    let grade = 11 - (graduation - year_end);
    if (1..=11).contains(&grade) {
        format!("{grade}{}", letter.to_uppercase())
    } else {
        target.to_string()
    }
}

/// Lessons to show: a cancelled lesson is hidden when another lesson takes its time
/// (the admin panel replaced it), as on the website.
pub fn visible_lessons(day: &ScheduleDay) -> Vec<&ScheduleLesson> {
    let mut lessons: Vec<&ScheduleLesson> = day
        .lessons
        .iter()
        .filter(|l| {
            l.instance.status != LessonInstanceStatus::Cancelled
                || !day.lessons.iter().any(|other| {
                    other.instance.id != l.instance.id
                        && other.instance.status != LessonInstanceStatus::Cancelled
                        && other.start_time == l.start_time
                })
        })
        .collect();
    lessons.sort_by_key(|l| l.start_time);
    lessons
}

/// One line of the schedule: a lesson, or a double lesson (2+ lessons of the same
/// subject for the same class/group, one right after another).
pub struct Block<'a> {
    pub lessons: Vec<&'a ScheduleLesson>,
}

impl<'a> Block<'a> {
    fn first(&self) -> &'a ScheduleLesson {
        self.lessons[0]
    }

    fn last(&self) -> &'a ScheduleLesson {
        self.lessons[self.lessons.len() - 1]
    }

    pub fn is_cancelled(&self) -> bool {
        self.first().instance.status == LessonInstanceStatus::Cancelled
    }

    /// "3", "3–4", or "" for a lesson off the bells.
    pub fn numbers(&self) -> String {
        match (lesson_number(self.first().start_time), lesson_number(self.last().start_time)) {
            (Some(a), Some(b)) if a != b => format!("{a}–{b}"),
            (Some(a), _) => a.to_string(),
            _ => String::new(),
        }
    }

    /// Does `next` continue this block (the next lesson of a double lesson)?
    fn continues_with(&self, next: &ScheduleLesson) -> bool {
        let last = self.last();
        if last.subject.id != next.subject.id
            || last.target_name != next.target_name
            || (last.instance.status == LessonInstanceStatus::Cancelled)
                != (next.instance.status == LessonInstanceStatus::Cancelled)
            || next.start_time < last.end_time
        {
            return false;
        }
        match (lesson_number(last.start_time), lesson_number(next.start_time)) {
            (Some(a), Some(b)) => b == a + 1,
            _ => next.start_time - last.end_time <= Duration::minutes(30),
        }
    }

    /// Homework of the block for a student: each different text once, files counted once.
    pub fn homework(&self) -> Option<String> {
        let mut texts: Vec<String> = Vec::new();
        let mut files = 0;
        let mut seen = Vec::new();
        for l in &self.lessons {
            let Some(details) = l.homework.as_ref() else { continue };
            if details.homework.status != HomeworkStatus::Published || seen.contains(&details.homework.id) {
                continue;
            }
            seen.push(details.homework.id);
            files += details.files.len();
            let text = details.homework.text_content.as_deref().unwrap_or("").trim();
            if !text.is_empty() && !texts.iter().any(|t| t == text) {
                texts.push(text.to_string());
            }
        }
        let text = texts.join("\n");
        match (text.is_empty(), files) {
            (true, 0) => None,
            (true, n) => Some(format!("📎 файлов: {n} (смотри на сайте)")),
            (false, 0) => Some(text),
            (false, n) => Some(format!("{text}\n📎 файлов: {n}")),
        }
    }

    /// "замена: Петрова А.", the admin's note: each different one once.
    fn notes(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for l in &self.lessons {
            let mut add = |s: String| {
                if !out.contains(&s) {
                    out.push(s);
                }
            };
            if let Some(sub) = &l.substitute_teacher {
                add(format!("🔄 замена: {}", short_name(sub)));
            }
            if let Some(note) = l.substitution.as_ref().and_then(|s| s.note.as_deref()) {
                if !note.trim().is_empty() {
                    add(format!("⚠️ {}", note.trim()));
                }
            }
            if l.is_extra {
                add("➕ добавлен в расписание".to_string());
            }
        }
        out
    }

    /// "каб. 316" or "каб. 316, 318" when the lessons of a pair are in different rooms.
    fn cabinets(&self) -> Option<String> {
        let mut rooms: Vec<String> = Vec::new();
        for l in &self.lessons {
            if let Some(c) = &l.cabinet {
                let n = c.number.to_string();
                if !rooms.contains(&n) {
                    rooms.push(n);
                }
            }
        }
        (!rooms.is_empty()).then(|| format!("каб. {}", rooms.join(", ")))
    }
}

/// The day's lessons merged into blocks (double lessons become one block).
/// Lessons of other groups at the same time do not break a pair.
pub fn blocks(day: &ScheduleDay) -> Vec<Block<'_>> {
    let mut out: Vec<Block> = Vec::new();
    for l in visible_lessons(day) {
        match out.iter_mut().find(|b| b.continues_with(l)) {
            Some(block) => block.lessons.push(l),
            None => out.push(Block { lessons: vec![l] }),
        }
    }
    out
}

fn short_name(u: &domain::entities::user::User) -> String {
    let initial = u.first_name.chars().next().map(|c| format!(" {c}.")).unwrap_or_default();
    format!("{}{}", u.last_name, initial)
}

/// Shortens homework for the schedule: first line, at most `max` characters.
fn short(text: &str, max: usize) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    let more = text.lines().count() > 1;
    if first.chars().count() > max {
        format!("{}…", first.chars().take(max).collect::<String>().trim_end())
    } else if more {
        format!("{first} …")
    } else {
        first.to_string()
    }
}

/// One day of lessons. `with_target` adds the class/group (teacher's view) and
/// leaves homework out; for a student homework is shown short (full text: /homework).
///
/// ```text
/// 📅 Завтра, вторник, 10 ноября
///
/// 1    09:00  Разговоры о важном · каб. 308
/// 3–4  11:00  Алгебра ×2 · каб. 316
///      📚 №123, 125
/// ```
pub fn format_day(day: &ScheduleDay, today: NaiveDate, with_target: bool) -> String {
    let mut out = format!("📅 <b>{}</b>\n", day_title(day.date, today));
    let blocks = blocks(day);
    if blocks.is_empty() {
        out.push_str("\nУроков нет.\n");
        return out;
    }
    out.push('\n');
    for b in &blocks {
        let first = b.first();
        let lead = format!("<code>{:<4} {}</code>", b.numbers(), hhmm(first.start_time));
        let subject = esc(&first.subject.name);
        let pair = if b.lessons.len() > 1 { format!(" ×{}", b.lessons.len()) } else { String::new() };
        if b.is_cancelled() {
            out.push_str(&format!("{lead}  <s>{subject}</s>{pair} — отменён ❌\n"));
            continue;
        }
        let mut line = format!("{lead}  <b>{subject}</b>{pair}");
        if with_target {
            line.push_str(&format!(" · {}", esc(&class_label(&first.target_name, today))));
        }
        if let Some(cab) = b.cabinets() {
            line.push_str(&format!(" · {cab}"));
        }
        out.push_str(&line);
        out.push('\n');
        for note in b.notes() {
            out.push_str(&format!("      {}\n", esc(&note)));
        }
        if !with_target {
            if let Some(hw) = b.homework() {
                out.push_str(&format!("      📚 {}\n", esc(&short(&hw, 60))));
            }
        }
    }
    out
}

/// Homework of the given days, grouped by day; a double lesson is listed once.
/// Days without homework are skipped.
pub fn format_homework(days: &[ScheduleDay], today: NaiveDate) -> String {
    let mut out = String::new();
    for day in days {
        let items: Vec<(String, String)> = blocks(day)
            .into_iter()
            .filter(|b| !b.is_cancelled())
            .filter_map(|b| {
                let hw = b.homework()?;
                let mut title = format!("<b>{}</b>", esc(&b.first().subject.name));
                if b.lessons.len() > 1 && !b.numbers().is_empty() {
                    title.push_str(&format!(" ({} урок)", b.numbers()));
                }
                Some((title, hw))
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("📅 <b>{}</b>\n", day_title(day.date, today)));
        for (title, hw) in items {
            out.push_str(&format!("• {title}: {}\n", esc(&hw).replace('\n', "\n   ")));
        }
        out.push('\n');
    }
    if out.is_empty() {
        "📚 Домашки пока нет.".to_string()
    } else {
        format!("📚 <b>Домашнее задание</b>\n\n{}", out.trim_end())
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

    use application::use_cases::schedule::schedule::HomeworkDetails;
    use domain::entities::cabinet::Cabinet;
    use domain::entities::homework::Homework;
    use domain::entities::lesson::Lesson;
    use domain::entities::lesson_instance::LessonInstance;
    use domain::entities::subject::Subject;
    use domain::value_objects::lesson_target::LessonTarget;
    use domain::value_objects::role::UserRole;
    use uuid::Uuid;

    fn subject(name: &str) -> Subject {
        // The same name gives the same id, as one subject in the database.
        let mut bytes = [0u8; 16];
        for (i, b) in name.bytes().enumerate() {
            bytes[i % 16] ^= b;
        }
        Subject { id: Uuid::from_bytes(bytes), name: name.to_string() }
    }

    fn lesson(start: (u32, u32), subj: &str, target: &str, cab: i32, hw: Option<&str>) -> ScheduleLesson {
        let st = NaiveTime::from_hms_opt(start.0, start.1, 0).unwrap();
        let instance_id = Uuid::new_v4();
        ScheduleLesson {
            instance: LessonInstance {
                id: instance_id,
                template_id: Uuid::new_v4(),
                week_start_date: d(2026, 11, 9),
                lesson_date: d(2026, 11, 10),
                status: LessonInstanceStatus::Scheduled,
                cabinet_id: None,
            },
            start_time: st,
            end_time: st + Duration::minutes(45),
            lesson: Lesson {
                id: Uuid::new_v4(),
                target: LessonTarget::Class(Uuid::nil()),
                subject_id: Uuid::nil(),
                is_active: true,
            },
            subject: subject(subj),
            target_name: target.to_string(),
            teachers: vec![],
            cabinet: Some(Cabinet { id: Uuid::new_v4(), number: cab, description: None, capacity: None }),
            homework: hw.map(|t| HomeworkDetails {
                homework: Homework {
                    id: Uuid::new_v4(),
                    lesson_instance_id: instance_id,
                    created_by: Uuid::nil(),
                    created_by_role: UserRole::Teacher,
                    text_content: Some(t.to_string()),
                    status: HomeworkStatus::Published,
                    locked_by_teacher: false,
                    last_edited_by: None,
                    created_at: Utc::now(),
                },
                files: vec![],
            }),
            substitution: None,
            substitute_teacher: None,
            is_extra: false,
        }
    }

    #[test]
    fn lesson_numbers_and_classes() {
        assert_eq!(lesson_number(NaiveTime::from_hms_opt(11, 55, 0).unwrap()), Some(4));
        assert_eq!(lesson_number(NaiveTime::from_hms_opt(12, 0, 0).unwrap()), None);
        assert_eq!(class_label("2027б", d(2026, 11, 9)), "11Б");
        assert_eq!(class_label("2031в", d(2026, 11, 9)), "7В");
        assert_eq!(class_label("Англ. гр. 1", d(2026, 11, 9)), "Англ. гр. 1");
        assert_eq!(esc("a<b & c>"), "a&lt;b &amp; c&gt;");
    }

    #[test]
    fn double_lesson_is_one_line_with_homework_of_either_lesson() {
        // Homework only on the first lesson of the pair; another group's lesson
        // at the same time sits between the two in the list.
        let day = ScheduleDay {
            date: d(2026, 11, 10),
            lessons: vec![
                lesson((9, 0), "Литература", "2027б", 308, None),
                lesson((9, 55), "Алгебра", "2027б", 316, Some("№123, 125")),
                lesson((9, 55), "Английский язык", "Англ. гр. 2", 210, None),
                lesson((11, 0), "Алгебра", "2027б", 316, None),
                lesson((11, 0), "Английский язык", "Англ. гр. 2", 210, None),
            ],
        };
        let b = blocks(&day);
        assert_eq!(b.len(), 3);
        assert_eq!(b[1].numbers(), "2–3");
        assert_eq!(b[1].homework().as_deref(), Some("№123, 125"));
        let text = format_day(&day, d(2026, 11, 9), false);
        assert!(text.contains("<code>2–3  09:55</code>  <b>Алгебра</b> ×2 · каб. 316"), "{text}");
        assert_eq!(text.matches("№123").count(), 1, "{text}");
    }

    #[test]
    fn same_homework_on_both_lessons_is_listed_once() {
        let day = ScheduleDay {
            date: d(2026, 11, 10),
            lessons: vec![
                lesson((11, 55), "Физика", "2027б", 214, Some("§12 <читать>")),
                lesson((13, 10), "Физика", "2027б", 214, Some("§12 <читать>")),
            ],
        };
        let text = format_homework(&[day], d(2026, 11, 9));
        assert_eq!(text.matches("§12").count(), 1, "{text}");
        assert!(text.contains("• <b>Физика</b> (4–5 урок): §12 &lt;читать&gt;"), "{text}");
    }
}

//! FILE: fuzzy.rs
//! PURPOSE: Typo-tolerant matching for commands and their arguments: `/shedule` -> `/schedule`,
//!          `/ысрувгду` (Russian keyboard layout) -> `/schedule`, «нелеля» -> «неделя».
//! DEPENDENCIES: none (pure functions, unit-tested below).

/// Command names the bot knows (without `/`), as in `Command`.
pub const COMMANDS: &[&str] = &[
    "start", "resend", "help", "schedule", "tomorrow", "week", "homework",
];

/// Number of typos between two words (Damerau–Levenshtein, optimal string alignment): a wrong,
/// missing or extra letter, or two neighbouring letters swapped, each count as one.
pub fn typos(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// How many typos a word of this length may have: none up to 3 letters, one up to 7, then two.
fn allowed(word: &str) -> usize {
    match word.chars().count() {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// The candidate closest to `word` within the allowed number of typos; `None` if there is none
/// or two candidates are equally close (better to say «unknown» than to guess).
pub fn closest<'a>(word: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let limit = allowed(word);
    let mut scored: Vec<(usize, &'a str)> = candidates
        .iter()
        .map(|c| (typos(word, c), *c))
        .filter(|(n, _)| *n <= limit)
        .collect();
    scored.sort();
    match scored.as_slice() {
        [(_, c)] => Some(*c),
        [(n, c), (m, _), ..] if n < m => Some(*c),
        _ => None,
    }
}

/// A letter typed with the Russian layout on, as it would be on the English one: «ы» -> `s`.
fn from_russian_layout(c: char) -> char {
    const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбю";
    const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";
    RU.chars()
        .position(|r| r == c)
        .and_then(|i| EN.chars().nth(i))
        .unwrap_or(c)
}

/// `/shedule завтра` -> `/schedule завтра`, `/рудз` -> `/help`; the `@bot` mention and the
/// arguments are kept. `None` if the command is not close to any known one.
pub fn fix_command(text: &str) -> Option<String> {
    let rest = text.strip_prefix('/')?;
    let (token, args) = match rest.find(char::is_whitespace) {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let (name, mention) = match token.find('@') {
        Some(i) => token.split_at(i),
        None => (token, ""),
    };
    let name = name.to_lowercase();
    let latin: String = name.chars().map(from_russian_layout).collect();
    let best = [name.as_str(), latin.as_str()].iter().find_map(|n| {
        COMMANDS
            .contains(n)
            .then_some(*n)
            .or_else(|| closest(n, COMMANDS))
    })?;
    if best == name {
        return None; // already a known name: the parse failed for another reason
    }
    Some(format!("/{best}{mention}{args}"))
}

/// Whether some word of `text` is `target` with at most the allowed number of typos.
pub fn has_word(text: &str, targets: &[&str]) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .any(|w| targets.iter().any(|t| typos(w, t) <= allowed(t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_typos() {
        assert_eq!(typos("schedule", "schedule"), 0);
        assert_eq!(typos("shedule", "schedule"), 1);
        assert_eq!(typos("homewrok", "homework"), 1);
        assert_eq!(typos("зватра", "завтра"), 1);
        assert_eq!(typos("нелеля", "неделя"), 1);
    }

    #[test]
    fn fixes_commands() {
        assert_eq!(fix_command("/shedule").as_deref(), Some("/schedule"));
        assert_eq!(
            fix_command("/shedule завтра").as_deref(),
            Some("/schedule завтра")
        );
        assert_eq!(
            fix_command("/SHEDULE@EduHub179Bot неделя").as_deref(),
            Some("/schedule@EduHub179Bot неделя")
        );
        assert_eq!(fix_command("/homewrok").as_deref(), Some("/homework"));
        assert_eq!(fix_command("/tommorow").as_deref(), Some("/tomorrow"));
        assert_eq!(fix_command("/hepl").as_deref(), Some("/help"));
        assert_eq!(fix_command("/ысрувгду").as_deref(), Some("/schedule"));
        assert_eq!(fix_command("/ырщцу").as_deref(), None); // "show": not a command
        assert_eq!(fix_command("/рудз").as_deref(), Some("/help"));
        assert_eq!(fix_command("/foo"), None);
        assert_eq!(fix_command("/admin"), None);
        assert_eq!(fix_command("/schedule"), None);
    }

    #[test]
    fn finds_words_with_typos() {
        assert!(has_word("на нелелю", &["неделя", "неделю"]));
        assert!(has_word("зватра", &["завтра"]));
        assert!(!has_word("что-то", &["завтра", "неделя"]));
    }
}

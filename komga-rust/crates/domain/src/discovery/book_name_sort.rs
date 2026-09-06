use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::{locale, Locale};
use std::cmp::Ordering;
use std::sync::OnceLock;

pub fn compare_book_names(left: &str, right: &str) -> Ordering {
    let left = split_into_segments(left);
    let right = split_into_segments(right);
    compare_segments(&left, &right)
}

pub fn system_locale_collator() -> &'static icu::collator::CollatorBorrowed<'static> {
    static COLLATOR: OnceLock<icu::collator::CollatorBorrowed<'static>> = OnceLock::new();
    COLLATOR.get_or_init(|| {
        let locale = SORT_LOCALE_OVERRIDE
            .get()
            .cloned()
            .flatten()
            .unwrap_or_else(|| locale!("und"));
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Tertiary);
        Collator::try_new(locale.into(), options)
            .expect("unicode collator for book name sorting should construct")
    })
}

static SORT_LOCALE_OVERRIDE: OnceLock<Option<Locale>> = OnceLock::new();

pub fn set_sort_locale(locale: Option<String>) {
    SORT_LOCALE_OVERRIDE.get_or_init(|| locale.and_then(|l| parse_locale(&l)));
}

fn parse_locale(value: &str) -> Option<Locale> {
    let lang = value.split('.').next().unwrap_or(value).replace('_', "-");
    lang.parse().ok()
}

fn book_names_collator() -> &'static icu::collator::CollatorBorrowed<'static> {
    system_locale_collator()
}

enum Segment {
    Text(String),
    Number(String),
}

fn split_into_segments(value: &str) -> Vec<Segment> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return vec![Segment::Text(String::new())];
    }
    let mut segments = Vec::new();
    let mut chars = normalized.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            let mut num_str = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    num_str.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if chars.peek() == Some(&'.') {
                chars.next();
                num_str.push('.');
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        num_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
            }
            segments.push(Segment::Number(num_str));
        } else {
            let mut text = String::new();
            while let Some(&c) = chars.peek() {
                if !c.is_ascii_digit() {
                    text.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            segments.push(Segment::Text(text));
        }
    }
    merge_segments(segments)
}

fn merge_segments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut result = Vec::new();
    for segment in segments {
        match segment {
            Segment::Text(text) => {
                if text.is_empty() {
                    continue;
                }
                if let Some(Segment::Text(prev)) = result.last_mut() {
                    prev.push_str(&text);
                } else {
                    result.push(Segment::Text(text));
                }
            }
            Segment::Number(num) => {
                result.push(Segment::Number(num));
            }
        }
    }
    if result.is_empty() {
        result.push(Segment::Text(String::new()));
    }
    result
}

fn compare_numeric_strings(a: &str, b: &str) -> Ordering {
    let (a_int, a_frac) = match a.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (a, None),
    };

    let (b_int, b_frac) = match b.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (b, None),
    };

    let a_int = a_int.trim_start_matches('0');
    let b_int = b_int.trim_start_matches('0');

    let a_int = if a_int.is_empty() { "0" } else { a_int };
    let b_int = if b_int.is_empty() { "0" } else { b_int };

    let int_cmp = a_int
        .len()
        .cmp(&b_int.len())
        .then_with(|| a_int.cmp(b_int));

    if int_cmp != Ordering::Equal {
        return int_cmp;
    }

    match (a_frac, b_frac) {
        (Some(a_frac), Some(b_frac)) => {
            let max_len = a_frac.len().max(b_frac.len());

            let a_frac = format!("{:<width$}", a_frac, width = max_len);
            let b_frac = format!("{:<width$}", b_frac, width = max_len);

            a_frac.cmp(&b_frac)
        }

        (None, None) => Ordering::Equal,

        (Some(a_frac), None) => {
            if a_frac.chars().all(|c| c == '0') {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        }

        (None, Some(b_frac)) => {
            if b_frac.chars().all(|c| c == '0') {
                Ordering::Equal
            } else {
                Ordering::Less
            }
        }
    }
}

fn compare_segments(left: &[Segment], right: &[Segment]) -> Ordering {
    use std::cmp::Ordering;
    let mut left_iter = left.iter();
    let mut right_iter = right.iter();
    while let (Some(l), Some(r)) = (left_iter.next(), right_iter.next()) {
        let ordering = match (l, r) {
            (Segment::Number(nl), Segment::Number(nr)) => compare_numeric_strings(nl, nr),
            (Segment::Text(tl), Segment::Text(tr)) => book_names_collator().compare(tl, tr),
            (Segment::Number(_), Segment::Text(_)) => Ordering::Less,
            (Segment::Text(_), Segment::Number(_)) => Ordering::Greater,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

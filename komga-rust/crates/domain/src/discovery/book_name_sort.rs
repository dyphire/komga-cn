use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use std::cmp::Ordering;
use std::sync::OnceLock;

pub fn compare_book_names(left: &str, right: &str) -> Ordering {
    let left = split_into_segments(left);
    let right = split_into_segments(right);
    compare_segments(&left, &right)
}

fn book_names_collator() -> &'static icu::collator::CollatorBorrowed<'static> {
    static COLLATOR: OnceLock<icu::collator::CollatorBorrowed<'static>> = OnceLock::new();
    COLLATOR.get_or_init(|| {
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Tertiary);
        Collator::try_new(locale!("und").into(), options)
            .expect("unicode collator for book name sorting should construct")
    })
}

enum Segment {
    Text(String),
    Number(f64),
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
            if let Ok(num) = num_str.parse::<f64>() {
                segments.push(Segment::Number(num));
            } else {
                segments.push(Segment::Text(num_str));
            }
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

fn compare_segments(left: &[Segment], right: &[Segment]) -> Ordering {
    use std::cmp::Ordering;
    let mut left_iter = left.iter();
    let mut right_iter = right.iter();
    while let (Some(l), Some(r)) = (left_iter.next(), right_iter.next()) {
        let ordering = match (l, r) {
            (Segment::Number(nl), Segment::Number(nr)) => {
                nl.partial_cmp(nr).unwrap_or(Ordering::Equal)
            }
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

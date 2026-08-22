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

fn system_locale_collator() -> &'static icu::collator::CollatorBorrowed<'static> {
    static COLLATOR: OnceLock<icu::collator::CollatorBorrowed<'static>> = OnceLock::new();
    COLLATOR.get_or_init(|| {
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Tertiary);
        Collator::try_new(system_locale().into(), options)
            .expect("unicode collator for book name sorting should construct")
    })
}

fn system_locale() -> Locale {
    std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .ok()
        .as_deref()
        .and_then(parse_locale)
        .or_else(|| macos_system_locale())
        .or_else(|| windows_system_locale())
        .unwrap_or_else(|| locale!("und"))
}

fn parse_locale(value: &str) -> Option<Locale> {
    let lang = value.split('.').next().unwrap_or(value).replace('_', "-");
    lang.parse().ok()
}

#[cfg(target_os = "macos")]
fn macos_system_locale() -> Option<Locale> {
    use std::ffi::{CStr, c_char, c_int};

    unsafe extern "C" {
        fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
    }
    const LC_ALL: c_int = 6;

    unsafe {
        let locale_ptr = setlocale(LC_ALL, std::ptr::null());
        if locale_ptr.is_null() {
            return None;
        }
        CStr::from_ptr(locale_ptr)
            .to_str()
            .ok()
            .and_then(parse_locale)
    }
}

#[cfg(not(target_os = "macos"))]
fn macos_system_locale() -> Option<Locale> {
    None
}

#[cfg(windows)]
fn windows_system_locale() -> Option<Locale> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let len = unsafe {
        GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32)
    };

    if len > 0 && (len as usize) < buffer.len() {
        let locale_name = OsString::from_wide(&buffer[..len as usize - 1]);
        locale_name.into_string().ok().and_then(|s| parse_locale(&s))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn windows_system_locale() -> Option<Locale> {
    None
}

fn book_names_collator() -> &'static icu::collator::CollatorBorrowed<'static> {
    system_locale_collator()
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

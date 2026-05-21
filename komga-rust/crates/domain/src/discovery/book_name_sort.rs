use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub fn compare_book_names(left: &str, right: &str) -> std::cmp::Ordering {
    let left = normalize_sort_key(left);
    let right = normalize_sort_key(right);
    natural_cmp(&left, &right)
}

fn normalize_sort_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .nfd()
        .filter(|ch| !is_combining_mark(*ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut left_index = 0usize;
    let mut right_index = 0usize;

    while left_index < left.len() && right_index < right.len() {
        let left_is_digit = left[left_index].is_ascii_digit();
        let right_is_digit = right[right_index].is_ascii_digit();

        if left_is_digit && right_is_digit {
            let left_end = digit_run_end(left, left_index);
            let right_end = digit_run_end(right, right_index);
            let ordering =
                compare_digit_runs(&left[left_index..left_end], &right[right_index..right_end]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let ordering = left[left_index].cmp(&right[right_index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
        left_index += 1;
        right_index += 1;
    }

    left.len().cmp(&right.len())
}

fn digit_run_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    index
}

fn compare_digit_runs(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left_trimmed = trim_leading_zeroes(left);
    let right_trimmed = trim_leading_zeroes(right);
    let significant = left_trimmed.len().cmp(&right_trimmed.len());
    if significant != Ordering::Equal {
        return significant;
    }

    let lexical = left_trimmed.cmp(right_trimmed);
    if lexical != Ordering::Equal {
        return lexical;
    }

    left.len().cmp(&right.len())
}

fn trim_leading_zeroes(bytes: &[u8]) -> &[u8] {
    let first_non_zero = bytes.iter().position(|byte| *byte != b'0');
    match first_non_zero {
        Some(index) => &bytes[index..],
        None if bytes.is_empty() => bytes,
        None => &bytes[bytes.len() - 1..],
    }
}

pub fn is_valid_isbn13(value: &str) -> bool {
    let digits = value
        .chars()
        .filter_map(|character| character.to_digit(10))
        .collect::<Vec<_>>();
    if digits.len() != 13 {
        return false;
    }

    let checksum = digits
        .iter()
        .take(12)
        .enumerate()
        .map(|(index, digit)| if index % 2 == 0 { *digit } else { digit * 3 })
        .sum::<u32>();
    let expected_check_digit = (10 - (checksum % 10)) % 10;

    digits[12] == expected_check_digit
}

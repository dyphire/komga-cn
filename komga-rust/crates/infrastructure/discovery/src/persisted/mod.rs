mod authors;
mod browse;
mod facets;
mod library_mappings;
mod query_support;
pub(crate) mod runtime_queries;

pub use browse::SqliteDiscoveryBrowseService;
pub use query_support::DiscoveryQuerySupportAccess;

/// Sort string values using the ICU collator (respects the configured sort locale),
/// matching the Kotlin backend's unicode3 collation semantics.
pub(super) fn sort_values_icu(values: &mut [String]) {
    let collator = komga_domain::discovery::system_locale_collator();
    values.sort_by(|left, right| {
        // ICU compares canonically equivalent strings (e.g. "é" and "e\u{301}")
        // as equal; fall back to the raw string ordering so the result is
        // deterministic regardless of the input row order.
        collator
            .compare(left, right)
            .then_with(|| left.cmp(right))
    });
}

#[cfg(test)]
mod tests {
    use super::sort_values_icu;

    #[test]
    fn icu_sort_orders_case_and_accents_like_unicode_collation() {
        let mut values = vec![
            "b".to_string(),
            "é".to_string(),
            "A".to_string(),
            "a".to_string(),
            "café".to_string(),
            "cafe".to_string(),
        ];
        sort_values_icu(&mut values);
        // UCA (und, tertiary): lowercase before uppercase on equal primary,
        // base letter before accented variant, letter groups ordered by primary,
        // so: a < A < b < cafe < café < é (é sorts under the e group, after c words)
        assert_eq!(
            values,
            vec!["a", "A", "b", "cafe", "café", "é"]
        );
    }

    #[test]
    fn icu_sort_breaks_canonical_equivalence_ties_by_raw_string() {
        let mut values = vec![
            "b".to_string(),
            "a\u{301}".to_string(),
            "á".to_string(),
            "a".to_string(),
        ];
        sort_values_icu(&mut values);
        // "á" (U+00E1) and "a\u{301}" (a + combining acute) are canonically
        // equivalent and compare equal under ICU; the raw-string fallback
        // orders them deterministically regardless of the input row order.
        assert_eq!(values, vec!["a", "a\u{301}", "á", "b"]);
    }
}

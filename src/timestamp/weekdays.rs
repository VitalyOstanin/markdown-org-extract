use std::borrow::Cow;

/// Replace localized weekday names with English ones using the provided mappings.
///
/// Returns a borrowed `Cow` when nothing is substituted (zero allocations) and
/// an owned `Cow` only on first match.
pub(crate) fn normalize_weekdays<'a>(text: &'a str, mappings: &[(&str, &str)]) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);
    for (localized, english) in mappings {
        if result.contains(localized) {
            result = Cow::Owned(result.replace(localized, english));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_borrowed_when_no_match() {
        let mappings = [("Понедельник", "Monday")];
        let out = normalize_weekdays("English text", &mappings);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn substitutes_known_localized_name() {
        let mappings = [("Понедельник", "Monday"), ("Пн", "Mon")];
        let out = normalize_weekdays("<2024-12-09 Пн>", &mappings);
        assert_eq!(out, "<2024-12-09 Mon>");
    }

    #[test]
    fn empty_mappings_passthrough() {
        let out = normalize_weekdays("<2024-12-09 Mon>", &[]);
        assert_eq!(out, "<2024-12-09 Mon>");
    }
}

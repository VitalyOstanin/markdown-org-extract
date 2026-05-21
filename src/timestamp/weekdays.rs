use aho_corasick::{AhoCorasick, MatchKind};
use std::borrow::Cow;

/// Replace localized weekday names with English ones using the provided mappings.
///
/// Implementation uses Aho-Corasick: a single linear scan over `text` finds
/// every occurrence of any localized pattern at once, instead of the
/// repeated `str::contains` + `str::replace` passes that scaled with the
/// number of patterns. For `--locale ru` (14 patterns) this cuts the
/// per-line work from O(N * 14) probes to O(N).
///
/// Returns a borrowed `Cow` when nothing is substituted (zero allocations)
/// and an owned `Cow` only when at least one match was found.
pub(crate) fn normalize_weekdays<'a>(text: &'a str, mappings: &[(&str, &str)]) -> Cow<'a, str> {
    if mappings.is_empty() {
        return Cow::Borrowed(text);
    }

    // `LeftmostFirst` matches the order of the input slice, so the caller
    // controls which translation wins on overlap (e.g. "Понедельник" before
    // "По" if both appear). This matches the loop-based behaviour where the
    // first pattern that matched also wrote first.
    let patterns: Vec<&str> = mappings.iter().map(|(loc, _)| *loc).collect();
    let replacements: Vec<&str> = mappings.iter().map(|(_, eng)| *eng).collect();
    let ac = match AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostFirst)
        .build(&patterns)
    {
        Ok(ac) => ac,
        Err(_) => return Cow::Borrowed(text),
    };

    if !ac.is_match(text) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(ac.replace_all(text, &replacements))
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

    #[test]
    fn leftmost_first_resolves_overlap() {
        // "Понедельник" must win over the shorter prefix "По" when both are in
        // the table; otherwise a short prefix would chew off characters from
        // the longer name. LeftmostFirst is configured for exactly this.
        let mappings = [("Понедельник", "Monday"), ("По", "Mo")];
        let out = normalize_weekdays("<2024-12-09 Понедельник>", &mappings);
        assert_eq!(out, "<2024-12-09 Monday>");
    }

    #[test]
    fn substitutes_multiple_distinct_localized_names() {
        // A single linear scan must replace every distinct pattern once;
        // before Aho-Corasick this happened through 14 sequential
        // contains+replace passes.
        let mappings = [("Пн", "Mon"), ("Вт", "Tue"), ("Ср", "Wed")];
        let out = normalize_weekdays("Пн Вт Ср", &mappings);
        assert_eq!(out, "Mon Tue Wed");
    }
}

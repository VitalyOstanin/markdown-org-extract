use aho_corasick::{AhoCorasick, MatchKind};
use std::borrow::Cow;
use std::sync::LazyLock;

/// Aho-Corasick engine prebuilt for the canonical `cli::RU_WEEKDAY_MAPPINGS`
/// table. The default `--locale ru,en` produces exactly this mapping on
/// every CLI invocation, and the previous implementation rebuilt the
/// automaton on every call to `normalize_weekdays`. Materialising it once
/// per process turns the hot path into a single `is_match` + `replace_all`
/// dispatch.
///
/// Both the patterns and the replacement strings are `&'static str` because
/// the source const points at string literals; storing them in the cache
/// avoids a per-call `Vec<&str>` materialisation.
static CACHED_RU_ENGINE: LazyLock<Option<(AhoCorasick, Vec<&'static str>)>> = LazyLock::new(|| {
    let patterns: Vec<&'static str> = crate::cli::RU_WEEKDAY_MAPPINGS
        .iter()
        .map(|(loc, _)| *loc)
        .collect();
    let replacements: Vec<&'static str> = crate::cli::RU_WEEKDAY_MAPPINGS
        .iter()
        .map(|(_, eng)| *eng)
        .collect();
    AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostFirst)
        .build(&patterns)
        .ok()
        .map(|ac| (ac, replacements))
});

/// Cheap content-equality check against the canonical RU mapping. Compares
/// length first (a constant), then walks the 14 tuple entries pairwise.
/// String comparison short-circuits on length, so a non-RU table of the
/// same length aborts on the first divergent entry. Roughly an order of
/// magnitude cheaper than rebuilding the Aho-Corasick engine.
fn mappings_match_ru(mappings: &[(&str, &str)]) -> bool {
    let canonical = crate::cli::RU_WEEKDAY_MAPPINGS;
    mappings.len() == canonical.len()
        && mappings
            .iter()
            .zip(canonical.iter())
            .all(|(a, b)| a.0 == b.0 && a.1 == b.1)
}

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
///
/// Fast path: when `mappings` is content-equal to
/// `cli::RU_WEEKDAY_MAPPINGS` (the default `--locale ru,en` table), the
/// process-cached Aho-Corasick engine in `CACHED_RU_ENGINE` is used.
pub(crate) fn normalize_weekdays<'a>(text: &'a str, mappings: &[(&str, &str)]) -> Cow<'a, str> {
    if mappings.is_empty() {
        return Cow::Borrowed(text);
    }

    if mappings_match_ru(mappings) {
        if let Some((ac, replacements)) = CACHED_RU_ENGINE.as_ref() {
            if !ac.is_match(text) {
                return Cow::Borrowed(text);
            }
            return Cow::Owned(ac.replace_all(text, replacements));
        }
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

    #[test]
    fn cached_ru_engine_produces_same_output_as_uncached() {
        // The 0.5.0 hot-path optimisation (MAJ-7) caches an
        // Aho-Corasick engine for the canonical `RU_WEEKDAY_MAPPINGS`
        // table. The cached engine and the uncached per-call build
        // must produce identical output for every covered input. This
        // pins the equivalence so a future change to either branch
        // breaks the test instead of producing silent semantic drift.
        let inputs = [
            "<2024-12-09 Понедельник>",
            "<2024-12-09 Пн>",
            "<2024-12-09 Среда 10:00>",
            "SCHEDULED: <2024-12-15 Воскресенье>",
            "Полностью русский текст без weekday-имён",
            "Mixed: Пн и Tuesday в одной строке",
            "",
        ];
        let canonical = crate::cli::RU_WEEKDAY_MAPPINGS;
        // Clone the table into a separate Vec so its slice pointer
        // does not coincide with the cached one -- exercises the
        // slow path explicitly.
        let cloned: Vec<(&str, &str)> = canonical.to_vec();
        for text in inputs {
            let fast = normalize_weekdays(text, canonical);
            let slow = normalize_weekdays(text, &cloned);
            assert_eq!(
                fast, slow,
                "cached and per-call engines must agree on `{text}`"
            );
        }
    }
}

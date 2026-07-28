//! Weekday-name translation tables.
//!
//! These live apart from the CLI because both the parser and the timestamp
//! extractor need them, and neither may depend on argument parsing: the
//! library is consumed in-process by embedders that never build a `Cli`.
//! The `--locale` validator in the binary is still the single place that
//! rejects unknown locales.

/// Russian weekday names mapped to their English equivalents. The same table
/// drives both `--locale ru` weekday normalization in the CLI and the
/// integration-style parser test that reproduces the production pipeline,
/// so adding or renaming an entry only requires editing it here.
pub const RU_WEEKDAY_MAPPINGS: &[(&str, &str)] = &[
    ("Понедельник", "Monday"),
    ("Вторник", "Tuesday"),
    ("Среда", "Wednesday"),
    ("Четверг", "Thursday"),
    ("Пятница", "Friday"),
    ("Суббота", "Saturday"),
    ("Воскресенье", "Sunday"),
    ("Пн", "Mon"),
    ("Вт", "Tue"),
    ("Ср", "Wed"),
    ("Чт", "Thu"),
    ("Пт", "Fri"),
    ("Сб", "Sat"),
    ("Вс", "Sun"),
];

/// Locales for which [`get_weekday_mappings`] ships a translation table.
/// `en` is recognised as a no-op (English weekday names need no mapping) so
/// the default `--locale ru,en` works without warnings.
pub const SUPPORTED_LOCALES: &[&str] = &["ru", "en"];

/// Return the (foreign, English) weekday-name pairs for the requested
/// `locale` string. `locale` is the comma-separated value of `--locale`
/// (e.g. `"ru,en"`); each segment is looked up independently, and
/// `"en"` / empty / whitespace segments contribute nothing because
/// English weekday names need no translation.
///
/// The returned mappings are fed to the timestamp parser as a Russian-
/// to-English alias table so org-mode timestamps written with Cyrillic
/// weekday abbreviations (`<2026-01-12 Пн>`) are parsed identically to
/// their English equivalents.
///
/// Callers are expected to have run the value through `validate_locale`
/// already, so unknown locales never reach this function — see the
/// `--locale` CLI validator for the single source of truth.
pub fn get_weekday_mappings(locale: &str) -> Vec<(&'static str, &'static str)> {
    // The CLI surface validates locale entries against SUPPORTED_LOCALES via
    // `validate_locale`, so reaching this function with anything outside
    // {"ru", "en", ""} means a programmer bypassed the value_parser. Unknown
    // entries are silently dropped here rather than warned about — the
    // single source of truth for "unknown locale" is the CLI validator.
    let mut mappings = Vec::new();
    for loc in locale.split(',') {
        // "en" / empty / anything else: nothing to translate. The CLI
        // validator already rejected unrecognised entries, so this
        // catch-all should only hit "en" or whitespace in practice.
        if loc.trim() == "ru" {
            mappings.extend_from_slice(RU_WEEKDAY_MAPPINGS);
        }
    }
    mappings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_weekday_mappings_ru() {
        let mappings = get_weekday_mappings("ru");
        assert!(mappings.contains(&("Понедельник", "Monday")));
        assert!(mappings.contains(&("Пн", "Mon")));
    }

    #[test]
    fn test_get_weekday_mappings_multiple() {
        let mappings = get_weekday_mappings("ru,en");
        assert!(mappings.contains(&("Понедельник", "Monday")));
    }

    #[test]
    fn get_weekday_mappings_ru_matches_static_table() {
        // The `--locale ru` output must be exactly the static table — there
        // is no other source of truth. Catches a future regression where
        // someone edits the table but forgets to update consumers, or vice
        // versa (the parser test imports the same constant, so a missing
        // entry would fail in both places at once).
        let mappings = get_weekday_mappings("ru");
        assert_eq!(mappings.as_slice(), RU_WEEKDAY_MAPPINGS);
    }

    #[test]
    fn test_get_weekday_mappings_empty() {
        let mappings = get_weekday_mappings("en");
        assert!(mappings.is_empty());
    }
}

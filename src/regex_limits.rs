//! Centralised constructor for `Regex` instances used to parse untrusted input
//! (markdown bodies, org-mode timestamps, CLOCK entries).
//!
//! Even though the `regex` crate has worst-case linear time and no classic
//! backtracking-style ReDoS, it still allocates a DFA whose size is bounded by
//! `dfa_size_limit` (10 MiB default) and a compiled program bounded by
//! `size_limit` (10 MiB default). These defaults are generous; for the small
//! patterns this crate uses, 1 MiB is plenty. Tighter limits act as
//! defense-in-depth: if a future change introduces a pathological pattern, the
//! build fails loudly instead of consuming memory silently.

use regex::{Regex, RegexBuilder};

const SIZE_LIMIT_BYTES: usize = 1 << 20; // 1 MiB
const DFA_SIZE_LIMIT_BYTES: usize = 1 << 20; // 1 MiB

/// Upper bound on the body of a single bracketed org-mode timestamp
/// (the run of `[^>]` chars between `<` and `>`). Caps how far the regex
/// engine will scan if the closing `>` is missing — defense in depth, not
/// a semantic limit. Used by every timestamp pattern in
/// `src/timestamp/extract.rs`.
///
/// The unit is **Unicode code points, not bytes**: `regex` runs in Unicode
/// mode by default, so each `[^>]` repetition consumes one code point. A
/// Cyrillic body therefore reaches the cap in fewer characters' worth of
/// bytes than an ASCII body of the same length. This is intentional — the
/// cap bounds the scan window regardless of per-code-point byte width
/// (ADR-0019); it is not a byte-size budget.
pub const TS_BODY_MAX: usize = 256;

/// Upper bound on the body of a single CLOCK timestamp inside `[…]` / `<…>`.
/// CLOCK bodies are well-formed `YYYY-MM-DD Day HH:MM` strings (~22 chars),
/// so this cap is generous but bounded. Used by `src/clock.rs`. Counted in
/// Unicode code points, not bytes — see `TS_BODY_MAX` and ADR-0019.
pub const CLOCK_BODY_MAX: usize = 128;

/// Compile a regex with conservative size limits. Panics if `pattern` is invalid
/// or exceeds the limits — both indicate a programmer error and should be caught
/// in tests (every call site goes through `LazyLock::new`, so the panic happens
/// on first use which is exercised by the unit tests).
pub fn compile_bounded(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .size_limit(SIZE_LIMIT_BYTES)
        .dfa_size_limit(DFA_SIZE_LIMIT_BYTES)
        .build()
        .unwrap_or_else(|e| panic!("Failed to compile regex {pattern:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_bounded_accepts_normal_pattern() {
        let re = compile_bounded(r"^\d{4}-\d{2}-\d{2}$");
        assert!(re.is_match("2026-05-16"));
        assert!(!re.is_match("not a date"));
    }

    #[test]
    fn compile_bounded_handles_actual_production_pattern_sizes() {
        // Smoke check: the largest patterns this crate compiles today
        // (CLOCK and the range timestamp from src/timestamp/extract.rs)
        // stay well below SIZE_LIMIT_BYTES even after the TS_BODY_MAX /
        // CLOCK_BODY_MAX bounded quantifiers are interpolated. If a
        // future edit raises either constant past the limit, this test
        // panics on regex build and shouts at the author.
        let ts_range = format!(
            r"^\s*<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>--?-?<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>"
        );
        let _ = compile_bounded(&ts_range);
        let clock_full = format!(
            r"CLOCK:\s*(?:\[([^\]<>]{{1,{CLOCK_BODY_MAX}}})\]|<([^\]<>]{{1,{CLOCK_BODY_MAX}}})>)(?:--(?:\[([^\]<>]{{1,{CLOCK_BODY_MAX}}})\]|<([^\]<>]{{1,{CLOCK_BODY_MAX}}})>))?(?:\s*=>\s*([0-9]{{1,5}}:[0-9]{{1,2}}))?"
        );
        let _ = compile_bounded(&clock_full);
    }

    #[test]
    fn compile_bounded_rejects_oversized_pattern() {
        // The whole point of SIZE_LIMIT_BYTES is defense-in-depth: if a
        // pathological pattern ever sneaks in, regex's compile step must
        // refuse it instead of allocating arbitrarily. A literal string
        // of ~1.5 MiB compiles into a program at least that large, which
        // exceeds the 1 MiB cap and must panic via `compile_bounded`.
        let huge: String = "a".repeat(1_500_000);
        let result = std::panic::catch_unwind(|| compile_bounded(&huge));
        assert!(
            result.is_err(),
            "an oversized pattern must panic at build time, not compile silently"
        );
    }

    #[test]
    fn compile_bounded_pathological_input_terminates() {
        // Long input with no closing `>` must not let the engine wander
        // off into a quadratic scan. The bounded body quantifier in our
        // timestamp patterns ([^>]{0,TS_BODY_MAX}) limits the search
        // window per anchor; with a 1 MiB filler we still finish in
        // single-digit milliseconds in practice. This test pins the
        // contract by capping the time budget and failing loudly on
        // regression rather than asserting an exact runtime.
        let ts_anchor = format!(r"^\s*<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>");
        let re = compile_bounded(&ts_anchor);
        let mut huge_input = String::from("<2024-12-05");
        huge_input.push_str(&" ".repeat(1_000_000));
        // No closing `>`: the match should fail, fast. The deadline is
        // intentionally loose (5s) to ride out a slow CI runner; the
        // pathological case used to be a runaway scan.
        let start = std::time::Instant::now();
        assert!(re.find(&huge_input).is_none());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "pathological input took too long: {:?}",
            start.elapsed()
        );
    }
}

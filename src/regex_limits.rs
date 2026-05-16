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

/// Compile a regex with conservative size limits. Panics if `pattern` is invalid
/// or exceeds the limits — both indicate a programmer error and should be caught
/// in tests (every call site goes through `Lazy::new`, so the panic happens on
/// first use which is exercised by the unit tests).
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
}

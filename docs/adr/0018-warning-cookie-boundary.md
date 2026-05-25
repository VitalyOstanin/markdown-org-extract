# ADR-0018: Warning-cookie boundary divergence from upstream

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted. Records a pre-existing intentional divergence from upstream
Emacs Org-mode in the warning-period cookie scanner, as required by
[ADR-0012](0012-verify-org-semantics-against-upstream.md). Relates to
[ADR-0014](0014-active-and-inactive-timestamps.md) (which added the
inactive `[...]` form whose closing bracket this scanner must also
accept). Does not change any decision in ADR-0014.

## Context

A timestamp may carry a warning-period cookie `-N<unit>`
(`<2026-05-21 Thu -3d>`) that overrides the global deadline warning
lead time for that one task. The project scans the bracket body for
this cookie with `WARNING_BODY_RE` in
[`src/timestamp/parser.rs`](../../src/timestamp/parser.rs).

Upstream `org-get-wdays` (`lisp/org.el:14937`) matches the cookie with:

```elisp
"-\\([0-9]+\\)\\([hdwmy]\\)\\(\\'\\|>\\| \\)"
```

That is: a `-`, one or more digits, a unit char (`h`/`d`/`w`/`m`/`y`),
then a terminator that is end-of-string, `>`, or a single literal
space. Upstream places **no** requirement on what precedes the `-`; it
relies on the unit char plus terminator to avoid matching the date's
own `-MM` / `-DD` runs, because a date component is never immediately
followed by a `[hdwmy]` char.

The 2026-05-25 logic review (F5) flagged that the project's regex
diverges from this shape and that the divergence was not recorded in an
ADR, only in a code comment. ADR-0012 requires the record.

## Decision

`WARNING_BODY_RE` stays as `\s-(\d+)([hdwmy])(?:[\s>\]]|$)`, which
diverges from upstream in two deliberate ways:

1. **A leading whitespace separator is required** before `-`. Upstream
   accepts a cookie with no separator. Requiring one is stricter and
   fail-closed: a cookie glued to preceding text (for example the
   second cookie in a malformed `-3d-2d`) is ignored rather than
   guessed at.
2. **The terminator class is `[\s>\]]|$`** instead of upstream's
   ` |>|\'`. It adds `]` so an inactive `[... -3d]` cookie (a form the
   project accepts since [ADR-0014](0014-active-and-inactive-timestamps.md))
   reads the same as the active `<... -3d>` form, and it uses `\s` (any
   whitespace) rather than a single literal space.

Consequences of (1) and (2): `-3day` is not a cookie (`a` is outside
the terminator class), and a pathological double cookie `-3d-2d`
matches nothing here, whereas upstream would extract the trailing
`-2d`. Neither body is produced by Emacs Org-mode; both are
hand-written edge cases. The fail-closed reading is preferred over the
upstream "extract whatever the trailing token is" behaviour because it
never silently invents a warning lead time the author did not clearly
write.

The behaviour is pinned by
`warning_cookie_requires_separator_and_terminator` in
`src/timestamp/parser.rs`.

## Consequences

Easier:

- The cookie scanner is unambiguous: a warning lead time is read only
  when it is whitespace-separated and terminated. There is no position
  on the line where a stray `-N<unit>` is silently interpreted.
- The inactive `[...]` form introduced by ADR-0014 is handled by the
  same regex without a second pattern.

Harder:

- A vault authored against strict upstream semantics that relies on a
  cookie with no leading separator, or on the trailing cookie of a
  `-3d-2d` pair, reads as "no cookie" here. This is a documented
  divergence, not a bug; such inputs are not produced by Emacs.
- A future change toward exact upstream parity (dropping the required
  leading `\s`) is a behaviour change and must supersede this ADR.

## References

- Upstream warning-cookie scanner: `org-get-wdays`
  (`lisp/org.el:14923`-`14945`), verified against the local checkout
  per [ADR-0012](0012-verify-org-semantics-against-upstream.md).
- Project code: `WARNING_BODY_RE` and
  `warning_cookie_requires_separator_and_terminator` in
  [`src/timestamp/parser.rs`](../../src/timestamp/parser.rs).
- Warning-cookie day-conversion factors are documented alongside
  `warning_cookie_to_days` in the same file and mirror upstream
  (`d=1`, `w=7`, `m=30.4`, `y=365.25`, `h=1/24`).
- Related ADRs:
  [ADR-0012](0012-verify-org-semantics-against-upstream.md) (the
  verify-before-shipping rule this ADR satisfies),
  [ADR-0014](0014-active-and-inactive-timestamps.md) (added the
  inactive `[...]` form whose `]` close this scanner accepts),
  [ADR-0002](0002-supported-org-mode-subset.md) (the supported subset
  this divergence is part of).

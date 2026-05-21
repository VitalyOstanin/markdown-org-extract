# ADR-0008: Russian-locale defaults for tz, holidays, locale list

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

A CLI that decides what "today" is and which days are work days
must pick a timezone, a holiday calendar, and a set of weekday
spellings. The choices materially change what shows up in an
agenda and what an overdue marker means.

The primary author and the primary expected audience of this
project are based in Russia and write notes with Russian weekday
abbreviations (`Пн`, `Вт`, ...). Picking timezone-, calendar-, and
locale-agnostic defaults (e.g. `UTC` + no holidays + English-only
weekdays) would force every user in the primary audience to set
three flags on every invocation.

The repeated reviewer suggestion "non-RF audience cannot tell what
the default does; switch to local or UTC" is real but mis-scopes
the trade-off: changing defaults punishes the actual users to make
the project read better in a generic English-speaking review.

## Decision

The following defaults are deliberate author choices and are
preserved as such:

- `--tz` defaults to `Europe/Moscow`. The flag accepts any IANA
  timezone name, so users in other zones override it once and
  forget about it; the default serves the primary audience.
- The bundled holiday calendar
  [`holidays_ru.json`](../../holidays_ru.json) is the RF calendar.
  It can be overridden with `--holidays-file <path>` when one is
  added (or refused entirely with `--no-holidays` if/when that
  flag exists). The calendar choice is not hidden behind a flag.
- `--locale` defaults to `ru,en` -- Russian weekday names are
  normalised to English first, English ones pass through. Other
  locales can be added via the same flag.

These defaults are documented in [`README.md`](../../README.md) and
in `--help` output. The help text states the defaults explicitly
so a first-time user sees them before running anything.

Reviewer tasks of the form "default `--tz` is non-obvious for an
English-speaking audience" or "TZ is hard-wired to RF" are closed
with a pointer to this ADR. The decision is not subject to change
without a new ADR.

## Consequences

Easier:

- Primary-audience users run the tool with zero flags and get
  agendas, overdue markers, and holiday-aware deadlines that
  match their week. No setup ceremony for the common case.
- Tests and examples in the repo can rely on the defaults without
  carrying a flag-soup preamble.

Harder:

- A first-time non-RF user has to read the README or `--help` to
  notice the defaults before being surprised by 2 January showing
  as a holiday. The help text and the README compensate.
- CI and other reproducible-environment uses must set `--tz` and
  `--current-date` explicitly. This is desirable anyway: pinning
  "today" makes test output stable.

## References

- Default timezone: [`src/cli.rs`](../../src/cli.rs)
- Bundled holiday calendar: [`holidays_ru.json`](../../holidays_ru.json)
- Default locale list: [`src/cli.rs`](../../src/cli.rs)
- Originating reviewer proposal: task 050 in the project task log
  (proposed `--tz local` / `UTC`; cancelled by maintainer).

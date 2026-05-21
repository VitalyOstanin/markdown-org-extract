use std::fmt;
use std::io;

/// Application error. Wraps IO and validation failures encountered by the CLI.
#[derive(Debug)]
pub enum AppError {
    /// Underlying IO error (file read, write, etc.)
    Io(io::Error),
    /// `--dir` does not exist or is not a directory
    InvalidDirectory(String),
    /// `--glob` pattern is malformed or uses an unsupported feature
    InvalidGlob(String),
    /// CLI date argument is not parseable as YYYY-MM-DD
    InvalidDate(String),
    /// `--tz` is not a valid IANA timezone
    InvalidTimezone(String),
    /// `--output` path is unsafe (missing parent, symlink, etc.)
    InvalidOutput(String),
    /// `--from` and `--to` form an invalid range
    DateRange(String),
    /// JSON or other serializer reported an error
    Serialization(String),
    /// Regex compilation failed
    Regex(String),
    /// Directory traversal (`ignore` crate) failed
    Walk(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The CLI prepends `error: ` to whatever this returns. Avoid adding a
        // second category prefix when the inner `msg` is already a complete
        // sentence; for opaque variants (Io / Walk / Regex / Serialization /
        // InvalidTimezone) keep a short lowercase tag so output stays grepable.
        match self {
            AppError::Io(e) => write!(f, "io: {e}"),
            AppError::InvalidDirectory(msg) => write!(f, "{msg}"),
            AppError::InvalidGlob(msg) => write!(f, "{msg}"),
            AppError::InvalidDate(msg) => write!(f, "{msg}"),
            AppError::InvalidTimezone(tz) => write!(f, "invalid timezone: {tz}"),
            AppError::InvalidOutput(msg) => write!(f, "{msg}"),
            AppError::DateRange(msg) => write!(f, "{msg}"),
            AppError::Serialization(msg) => write!(f, "serialization: {msg}"),
            AppError::Regex(msg) => write!(f, "regex: {msg}"),
            AppError::Walk(msg) => write!(f, "walk: {msg}"),
        }
    }
}

impl AppError {
    /// Process exit code that classifies this error category.
    ///
    /// - `2`  -- usage / input-validation failures the user can correct by
    ///   changing CLI arguments (matches clap's own argument-error exit).
    /// - `74` -- IO failures (`EX_IOERR` from `sysexits.h`): unreadable files,
    ///   directory traversal errors, write failures.
    /// - `70` -- internal software errors (`EX_SOFTWARE`): a regex we built
    ///   ourselves did not compile, or our own serializer failed.
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::InvalidDirectory(_)
            | AppError::InvalidGlob(_)
            | AppError::InvalidDate(_)
            | AppError::InvalidTimezone(_)
            | AppError::InvalidOutput(_)
            | AppError::DateRange(_) => 2,
            AppError::Io(_) | AppError::Walk(_) => 74,
            AppError::Regex(_) | AppError::Serialization(_) => 70,
        }
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Serialization(err.to_string())
    }
}

impl From<ignore::Error> for AppError {
    fn from(err: ignore::Error) -> Self {
        AppError::Walk(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, ErrorKind};

    #[test]
    fn display_invalid_directory() {
        let e = AppError::InvalidDirectory("directory does not exist: /no/such".into());
        assert_eq!(e.to_string(), "directory does not exist: /no/such");
    }

    #[test]
    fn display_invalid_glob() {
        let e = AppError::InvalidGlob("invalid pattern '[': ...".into());
        assert_eq!(e.to_string(), "invalid pattern '[': ...");
    }

    #[test]
    fn display_invalid_timezone() {
        assert_eq!(
            AppError::InvalidTimezone("X".into()).to_string(),
            "invalid timezone: X"
        );
    }

    #[test]
    fn display_invalid_output() {
        assert_eq!(
            AppError::InvalidOutput("refusing to overwrite symlink: /tmp/foo".into()).to_string(),
            "refusing to overwrite symlink: /tmp/foo"
        );
    }

    #[test]
    fn display_date_range() {
        assert_eq!(
            AppError::DateRange("from > to".into()).to_string(),
            "from > to"
        );
    }

    #[test]
    fn from_io_error_wraps() {
        let io = io::Error::new(ErrorKind::NotFound, "missing");
        let e: AppError = io.into();
        assert!(matches!(e, AppError::Io(_)));
        assert!(e.to_string().starts_with("io: "));
    }

    #[test]
    fn from_serde_json_error_wraps() {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str("not json");
        let e: AppError = parsed.unwrap_err().into();
        assert!(matches!(e, AppError::Serialization(_)));
        assert!(e.to_string().starts_with("serialization: "));
    }

    #[test]
    fn errors_are_send_sync() {
        // Compile-time check that AppError can flow across threads — matters if
        // we ever spawn worker threads (e.g. for parallel walker).
        fn is_send_sync<T: Send + Sync>() {}
        is_send_sync::<AppError>();
    }

    #[test]
    fn exit_code_usage_errors_return_2() {
        assert_eq!(
            AppError::InvalidDirectory("x".into()).exit_code(),
            2,
            "InvalidDirectory is a usage error and must map to exit 2"
        );
        assert_eq!(AppError::InvalidGlob("x".into()).exit_code(), 2);
        assert_eq!(AppError::InvalidDate("x".into()).exit_code(), 2);
        assert_eq!(AppError::InvalidTimezone("x".into()).exit_code(), 2);
        assert_eq!(AppError::InvalidOutput("x".into()).exit_code(), 2);
        assert_eq!(AppError::DateRange("x".into()).exit_code(), 2);
    }

    #[test]
    fn exit_code_io_and_walk_return_74() {
        let io = io::Error::new(ErrorKind::NotFound, "missing");
        assert_eq!(
            AppError::Io(io).exit_code(),
            74,
            "Io maps to EX_IOERR (74) from sysexits.h"
        );
        assert_eq!(AppError::Walk("x".into()).exit_code(), 74);
    }

    #[test]
    fn exit_code_software_errors_return_70() {
        assert_eq!(
            AppError::Regex("x".into()).exit_code(),
            70,
            "Regex compile failure is an internal software error (EX_SOFTWARE = 70)"
        );
        assert_eq!(AppError::Serialization("x".into()).exit_code(), 70);
    }
}

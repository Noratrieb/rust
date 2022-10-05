//! Library used by tidy and other tools.
//!
//! This library contains the tidy lints and exposes it
//! to be used by tools.

/// A helper macro to `unwrap` a result except also print out details like:
///
/// * The expression that failed
/// * The error itself
/// * (optionally) a path connected to the error (e.g. failure to open a file)
#[macro_export]
macro_rules! t {
    ($e:expr, $p:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed on {} with {}", stringify!($e), ($p).display(), e),
        }
    };

    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

macro_rules! tidy_error {
    ($bad:expr, $fmt:expr) => ({
        *$bad = true;
        eprint!("tidy error: ");
        eprintln!($fmt);
    });
    ($bad:expr, $fmt:expr, $($arg:tt)*) => ({
        *$bad = true;
        eprint!("tidy error: ");
        eprintln!($fmt, $($arg)*);
    });
}

trait TidyCheck {
    fn check(path: &std::path::Path, errors: &TidyErrors);
}

pub type LineNumber = usize;

pub use tidy_errors::TidyErrors;

mod tidy_errors {
    use std::sync::atomic::Ordering;
    use std::{fmt::Display, path::PathBuf, sync::atomic::AtomicBool};

    use crate::LineNumber;

    struct TidyError {
        file: PathBuf,
        line: Option<LineNumber>,
        msg: String,
    }

    impl Display for TidyError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.file.display())?;
            if let Some(line) = self.line {
                write!(f, ":{}", line)?;
            }
            write!(f, ":{}", self.msg)
        }
    }

    pub struct TidyErrors {
        has_error: AtomicBool,
    }

    impl TidyErrors {
        pub fn new() -> Self {
            Self { has_error: AtomicBool::new(false) }
        }

        pub fn has_errors(&self) -> bool {
            self.has_error.load(Ordering::Relaxed)
        }

        pub fn file_error(&self, file: impl Into<PathBuf>, msg: impl Into<String>) {
            self.raw_error(TidyError { file: file.into(), line: None, msg: msg.into() });
        }

        pub fn error(&self, file: impl Into<PathBuf>, line: LineNumber, msg: impl Into<String>) {
            self.raw_error(TidyError { file: file.into(), line: Some(line), msg: msg.into() });
        }

        fn raw_error(&self, error: TidyError) {
            eprintln!("{error}");
            self.has_error.store(true, Ordering::Relaxed);
        }
    }
}

pub mod bins;
pub mod debug_artifacts;
pub mod deps;
pub mod edition;
pub mod error_codes_check;
pub mod errors;
pub mod extdeps;
pub mod features;
pub mod pal;
pub mod primitive_docs;
pub mod style;
pub mod target_specific_tests;
pub mod ui_tests;
pub mod unit_tests;
pub mod unstable_book;
pub mod walk;

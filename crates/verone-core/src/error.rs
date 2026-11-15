//! The error type shared by every Verone crate.
//!
//! An error carries three things: a kind (which decides the process exit code), a
//! message saying what went wrong, and an optional hint saying what to do next.

use std::fmt;

/// What sort of failure this is. Each kind maps to one exit code of the `verone`
/// command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The operation was attempted and failed.
    Failed,
    /// The request itself was wrong (bad argument, invalid name).
    Usage,
    /// The host cannot run Verone as it is.
    HostCheck,
    /// The phone or image does not exist.
    NotFound,
    /// Another `verone` command is working on the same phone.
    Busy,
}

impl ErrorKind {
    /// The exit code the `verone` command uses for this kind.
    pub fn exit_code(self) -> u8 {
        match self {
            ErrorKind::Failed => 1,
            ErrorKind::Usage => 2,
            ErrorKind::HostCheck => 3,
            ErrorKind::NotFound => 4,
            ErrorKind::Busy => 5,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ErrorKind::Failed => "failed",
            ErrorKind::Usage => "usage",
            ErrorKind::HostCheck => "host-check",
            ErrorKind::NotFound => "not-found",
            ErrorKind::Busy => "busy",
        };
        f.write_str(text)
    }
}

/// An error from Verone.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    kind: ErrorKind,
    message: String,
    hint: Option<String>,
}

impl Error {
    /// Creates an error of the given kind.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    /// The operation was attempted and failed.
    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Failed, message)
    }

    /// The request itself was wrong.
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Usage, message)
    }

    /// The host cannot run Verone as it is.
    pub fn host_check(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::HostCheck, message)
    }

    /// The phone or image does not exist.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    /// Another `verone` command is working on the same phone.
    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Busy, message)
    }

    /// Adds a hint telling the user what to do next.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The kind of failure.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// What went wrong.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// What to do next, if known.
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    /// The exit code the `verone` command uses for this error.
    pub fn exit_code(&self) -> u8 {
        self.kind.exit_code()
    }
}

/// Result type used throughout Verone.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_has_its_documented_exit_code() {
        assert_eq!(Error::failed("x").exit_code(), 1);
        assert_eq!(Error::usage("x").exit_code(), 2);
        assert_eq!(Error::host_check("x").exit_code(), 3);
        assert_eq!(Error::not_found("x").exit_code(), 4);
        assert_eq!(Error::busy("x").exit_code(), 5);
    }

    #[test]
    fn display_shows_the_message_only() {
        let error = Error::failed("the phone did not boot").with_hint("Run verone doctor.");
        assert_eq!(error.to_string(), "the phone did not boot");
    }

    #[test]
    fn hint_is_absent_until_added() {
        let error = Error::not_found("no phone named work");
        assert_eq!(error.hint(), None);
        let error = error.with_hint("Run verone list.");
        assert_eq!(error.hint(), Some("Run verone list."));
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(error.message(), "no phone named work");
    }

    #[test]
    fn kind_names_are_stable() {
        assert_eq!(ErrorKind::HostCheck.to_string(), "host-check");
        assert_eq!(ErrorKind::NotFound.to_string(), "not-found");
    }
}

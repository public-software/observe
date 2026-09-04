//! The errors of the crate: a payload that is not the JSON the mapping describes, and a payload
//! that decodes but breaks a rule of the protocol.

use std::fmt;

/// Why a payload was refused.
#[derive(Debug)]
pub enum Error {
    /// The text is not JSON, or not the JSON the OTLP mapping describes (a wrong type, an enum
    /// given by name, an id that is not hex of the right length, a missing required field).
    Json(serde_json::Error),
    /// The payload decoded but breaks a rule of the protocol; see [`Invalid`].
    Invalid(Invalid),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Json(error) => write!(f, "not OTLP/JSON: {error}"),
            Error::Invalid(invalid) => invalid.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Json(error) => Some(error),
            Error::Invalid(_) => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Json(error)
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Error::Invalid(invalid)
    }
}

/// A rule of the protocol broken by a value, with where it was found.
///
/// `at` is the path of the offending message in the JSON mapping's own names
/// (`resourceSpans[0].scopeSpans[0].spans[2]`); `reason` names the rule in the mapping's field names
/// (`endTimeUnixNano 10 precedes startTimeUnixNano 20`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    /// Where: the path of the message that breaks the rule.
    pub at: String,
    /// What: the rule, in the wire names of the fields involved.
    pub reason: String,
}

impl Invalid {
    /// A refusal at `at` for `reason`.
    pub fn new(at: impl Into<String>, reason: impl Into<String>) -> Self {
        Invalid {
            at: at.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.at, self.reason)
    }
}

impl std::error::Error for Invalid {}

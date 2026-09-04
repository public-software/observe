//! The log signal: log records grouped by resource and scope, with a severity, a body and the
//! trace context they belong to.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{AnyValue, InstrumentationScope, KeyValue};
use crate::error::Invalid;
use crate::ids::{SpanId, TraceId};
use crate::resource::Resource;
use crate::validate::{Path, unique_keys};
use crate::wire;

/// A logs payload: the body of an `ExportLogsServiceRequest`, what `/v1/logs` receives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsData {
    /// The log records, grouped by the resource that produced them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_logs: Vec<ResourceLogs>,
}

impl LogsData {
    pub(crate) fn check(&self) -> Result<(), Invalid> {
        let root = Path::root();
        for (i, group) in self.resource_logs.iter().enumerate() {
            group.check(&root.item("resourceLogs", i))?;
        }
        Ok(())
    }
}

/// The log records of one resource, grouped by instrumentation scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLogs {
    /// The resource; absent when no resource information is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<Resource>,
    /// The records, grouped by scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope_logs: Vec<ScopeLogs>,
    /// The schema URL of the resource's attributes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_url: String,
}

impl ResourceLogs {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if let Some(resource) = &self.resource {
            resource.check(&at.field("resource"))?;
        }
        for (i, group) in self.scope_logs.iter().enumerate() {
            group.check(&at.item("scopeLogs", i))?;
        }
        Ok(())
    }
}

/// The log records of one instrumentation scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeLogs {
    /// The scope; absent is an empty scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<InstrumentationScope>,
    /// The records.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_records: Vec<LogRecord>,
    /// The schema URL of the scope's and the records' attributes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_url: String,
}

impl ScopeLogs {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if let Some(scope) = &self.scope {
            unique_keys(&scope.attributes, &at.field("scope"))?;
        }
        for (i, record) in self.log_records.iter().enumerate() {
            record.check(&at.item("logRecords", i))?;
        }
        Ok(())
    }
}

/// One log record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    /// When the event the record describes happened, in nanoseconds since the Unix epoch; zero
    /// when unknown.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub time_unix_nano: u64,
    /// When the record was observed by the collecting system; the fallback for
    /// [`time_unix_nano`](Self::time_unix_nano).
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub observed_time_unix_nano: u64,
    /// The normalized severity.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub severity_number: SeverityNumber,
    /// The severity as the source spelled it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub severity_text: String,
    /// The body: a string or a structured value; absent when the record has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<AnyValue>,
    /// The record's attributes; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// How many attributes the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_attributes_count: u32,
    /// Bits 0–7 the W3C trace flags; the rest reserved.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub flags: u32,
    /// The trace the record belongs to, when it was emitted inside one.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "wire::opt_id::deserialize"
    )]
    pub trace_id: Option<TraceId>,
    /// The span the record belongs to; needs [`trace_id`](Self::trace_id).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "wire::opt_id::deserialize"
    )]
    pub span_id: Option<SpanId>,
    /// The event's name, when the record is an event: a unique identifier of its kind.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event_name: String,
}

impl LogRecord {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if self.trace_id.is_some_and(|id| id.is_zero()) {
            return Err(at.refuse("traceId is all zeros"));
        }
        if self.span_id.is_some_and(|id| id.is_zero()) {
            return Err(at.refuse("spanId is all zeros"));
        }
        if self.span_id.is_some() && self.trace_id.is_none() {
            return Err(at.refuse("spanId without traceId"));
        }
        unique_keys(&self.attributes, at)
    }
}

/// The normalized severity of a log record: 0 is unspecified, then six levels of four numbers
/// each, 1 (`TRACE`) to 24 (`FATAL4`); an integer on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeverityNumber(u8);

macro_rules! severity_consts {
    ($($name:ident = $value:literal),* $(,)?) => {
        $(
            #[doc = concat!("`SEVERITY_NUMBER_", stringify!($name), "`, ", stringify!($value), ".")]
            pub const $name: SeverityNumber = SeverityNumber($value);
        )*
    };
}

impl SeverityNumber {
    severity_consts! {
        UNSPECIFIED = 0,
        TRACE = 1, TRACE2 = 2, TRACE3 = 3, TRACE4 = 4,
        DEBUG = 5, DEBUG2 = 6, DEBUG3 = 7, DEBUG4 = 8,
        INFO = 9, INFO2 = 10, INFO3 = 11, INFO4 = 12,
        WARN = 13, WARN2 = 14, WARN3 = 15, WARN4 = 16,
        ERROR = 17, ERROR2 = 18, ERROR3 = 19, ERROR4 = 20,
        FATAL = 21, FATAL2 = 22, FATAL3 = 23, FATAL4 = 24,
    }

    /// The severity for its number, `0..=24`.
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 24 {
            Some(SeverityNumber(value))
        } else {
            None
        }
    }

    /// The number.
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// The level the number falls in; none when unspecified.
    pub const fn level(self) -> Option<Severity> {
        Some(match self.0 {
            0 => return None,
            1..=4 => Severity::Trace,
            5..=8 => Severity::Debug,
            9..=12 => Severity::Info,
            13..=16 => Severity::Warn,
            17..=20 => Severity::Error,
            _ => Severity::Fatal,
        })
    }
}

impl Serialize for SeverityNumber {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for SeverityNumber {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        wire::deserialize_enum(deserializer, "severityNumber", |value| {
            u8::try_from(value).ok().and_then(SeverityNumber::new)
        })
    }
}

/// The six severity levels, each covering four severity numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// Numbers 1–4: fine-grained debugging.
    Trace,
    /// Numbers 5–8: debugging.
    Debug,
    /// Numbers 9–12: informational.
    Info,
    /// Numbers 13–16: a warning.
    Warn,
    /// Numbers 17–20: an error.
    Error,
    /// Numbers 21–24: a fatal error.
    Fatal,
}

impl Severity {
    /// The level's name in capitals, as the data model spells it.
    pub const fn name(self) -> &'static str {
        match self {
            Severity::Trace => "TRACE",
            Severity::Debug => "DEBUG",
            Severity::Info => "INFO",
            Severity::Warn => "WARN",
            Severity::Error => "ERROR",
            Severity::Fatal => "FATAL",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_numbers_are_integers_in_range() {
        for value in 0..=24u8 {
            let number = SeverityNumber::new(value).unwrap();
            assert_eq!(number.as_u8(), value);
            assert_eq!(serde_json::to_string(&number).unwrap(), value.to_string());
            assert_eq!(
                serde_json::from_str::<SeverityNumber>(&value.to_string()).unwrap(),
                number
            );
        }
        assert_eq!(SeverityNumber::new(25), None);
        assert!(
            serde_json::from_str::<SeverityNumber>("25")
                .unwrap_err()
                .to_string()
                .contains("severityNumber 25")
        );
        assert!(serde_json::from_str::<SeverityNumber>("\"INFO\"").is_err());
        assert_eq!(SeverityNumber::default(), SeverityNumber::UNSPECIFIED);
        assert!(SeverityNumber::WARN < SeverityNumber::ERROR4);
    }

    #[test]
    fn the_levels_cover_four_numbers_each() {
        let expected = [
            (
                SeverityNumber::TRACE,
                SeverityNumber::TRACE4,
                Severity::Trace,
            ),
            (
                SeverityNumber::DEBUG,
                SeverityNumber::DEBUG4,
                Severity::Debug,
            ),
            (SeverityNumber::INFO, SeverityNumber::INFO4, Severity::Info),
            (SeverityNumber::WARN, SeverityNumber::WARN4, Severity::Warn),
            (
                SeverityNumber::ERROR,
                SeverityNumber::ERROR4,
                Severity::Error,
            ),
            (
                SeverityNumber::FATAL,
                SeverityNumber::FATAL4,
                Severity::Fatal,
            ),
        ];
        for (low, high, level) in expected {
            assert_eq!(high.as_u8() - low.as_u8(), 3);
            for value in low.as_u8()..=high.as_u8() {
                assert_eq!(
                    SeverityNumber::new(value).unwrap().level(),
                    Some(level),
                    "{value}"
                );
            }
        }
        assert_eq!(SeverityNumber::UNSPECIFIED.level(), None);
        assert_eq!(Severity::Fatal.to_string(), "FATAL");
        assert_eq!(Severity::Info.name(), "INFO");
    }

    #[test]
    fn a_record_with_ids_needs_the_trace_first() {
        let at = Path::root().field("record");
        let record = LogRecord {
            span_id: Some(SpanId::from([1; 8])),
            ..LogRecord::default()
        };
        assert_eq!(
            record.check(&at).unwrap_err().reason,
            "spanId without traceId"
        );
        let record = LogRecord {
            trace_id: Some(TraceId::default()),
            ..LogRecord::default()
        };
        assert_eq!(
            record.check(&at).unwrap_err().reason,
            "traceId is all zeros"
        );
        let record = LogRecord {
            trace_id: Some(TraceId::from([1; 16])),
            span_id: Some(SpanId::default()),
            ..LogRecord::default()
        };
        assert_eq!(record.check(&at).unwrap_err().reason, "spanId is all zeros");
        assert!(LogRecord::default().check(&at).is_ok());
    }
}

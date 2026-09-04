//! `pub-observe-schema` — the signal schema of [`observe`](https://github.com/public-software/observe),
//! the observability stack of Public Software.
//!
//! The typed model of the three signals every exporter, store and dashboard of the suite speaks:
//! a [`Resource`] and an [`InstrumentationScope`] describing who produced the data, [`Span`]s with
//! their events, links and status ([`TracesData`]), [`Metric`]s as gauges, sums, histograms,
//! exponential histograms and summaries over their data points ([`MetricsData`]), and
//! [`LogRecord`]s with a severity, a body and their trace context ([`LogsData`]). The model is the
//! OpenTelemetry protocol's (listed in the repository's `PROVENANCE.md`) and its JSON encoding is
//! the OTLP/JSON mapping, so a payload from any OpenTelemetry SDK decodes here and what this crate
//! writes any OTLP receiver accepts:
//!
//! - keys in lowerCamelCase, proto3 defaults left out when writing and assumed when reading;
//! - `traceId` and `spanId` as hex, either case in, lowercase out;
//! - enums as integers only, never their names;
//! - 64-bit integers written as decimal strings and read from strings or numbers;
//! - unknown fields ignored, as the protocol requires of a receiver.
//!
//! [`Signal::from_json`] parses and then validates: what the protocol calls invalid (a span
//! ending before it starts, an all-zero identifier, a sum without a temporality, a histogram whose
//! buckets do not match its bounds, duplicate attribute keys) is an [`Invalid`] naming the path
//! and the rule. The serde implementations are public, so a reader that wants the lenient parse
//! alone has it. Not here, by design: the profiles signal, entity references, the string-table
//! indices of the profiling extension, `NaN` and `Infinity` literals, the protobuf binary
//! encoding, and any transport (ADR-0001).
//!
//! ```
//! use pub_observe_schema::{
//!     KeyValue, Resource, ResourceSpans, ScopeSpans, Signal, Span, SpanId, SpanKind, TraceId,
//!     TracesData,
//! };
//!
//! let span = Span {
//!     trace_id: TraceId::from_hex("5B8EFFF798038103D269B633813FC60C")?,
//!     span_id: SpanId::from_hex("eee19b7ec3c1b174")?,
//!     name: "GET /".into(),
//!     kind: SpanKind::Server,
//!     start_time_unix_nano: 1_544_712_660_000_000_000,
//!     end_time_unix_nano: 1_544_712_661_000_000_000,
//!     attributes: vec![KeyValue::new("http.request.method", "GET")],
//!     ..Span::default()
//! };
//! let traces = TracesData {
//!     resource_spans: vec![ResourceSpans {
//!         resource: Some(Resource { attributes: vec![KeyValue::new("service.name", "web")], ..Resource::default() }),
//!         scope_spans: vec![ScopeSpans { spans: vec![span], ..ScopeSpans::default() }],
//!         ..ResourceSpans::default()
//!     }],
//! };
//! let json = traces.to_json();
//! assert!(json.contains(r#""traceId":"5b8efff798038103d269b633813fc60c""#));
//! assert!(json.contains(r#""startTimeUnixNano":"1544712660000000000""#));
//! assert!(json.contains(r#""kind":2"#));
//! assert_eq!(TracesData::from_json(&json)?, traces);
//! # Ok::<(), pub_observe_schema::Error>(())
//! ```

#![forbid(unsafe_code)]

mod base64;
mod common;
mod error;
mod ids;
mod logs;
mod metrics;
mod resource;
mod trace;
mod validate;
mod wire;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use common::{AnyValue, InstrumentationScope, KeyValue};
pub use error::{Error, Invalid};
pub use ids::{SpanId, TraceId};
pub use logs::{LogRecord, LogsData, ResourceLogs, ScopeLogs, Severity, SeverityNumber};
pub use metrics::{
    AggregationTemporality, Buckets, Exemplar, ExponentialHistogram, ExponentialHistogramDataPoint,
    Gauge, Histogram, HistogramDataPoint, Metric, MetricData, MetricsData, NumberDataPoint,
    NumberValue, ResourceMetrics, ScopeMetrics, Sum, Summary, SummaryDataPoint, ValueAtQuantile,
};
pub use resource::Resource;
pub use trace::{
    Event, Link, ResourceSpans, ScopeSpans, Span, SpanKind, Status, StatusCode, TracesData,
};

/// The bit masks of the `flags` fields.
pub mod flags {
    /// Bits 0–7 of a span's, a link's or a log record's flags: the W3C trace flags.
    pub const TRACE_FLAGS_MASK: u32 = 0x0000_00ff;
    /// Bit 8 of a span's or a link's flags: whether the remote bit below is known.
    pub const CONTEXT_HAS_IS_REMOTE_MASK: u32 = 0x0000_0100;
    /// Bit 9 of a span's or a link's flags: whether the parent (or the linked span) is remote.
    pub const CONTEXT_IS_REMOTE_MASK: u32 = 0x0000_0200;
    /// Bit 0 of a data point's flags: the point carries no recorded value (a series ended).
    pub const NO_RECORDED_VALUE_MASK: u32 = 0x0000_0001;
}

/// A payload of one signal: what decodes from, and encodes to, OTLP/JSON.
pub trait Signal: Serialize + DeserializeOwned + Sized {
    /// The rules of the protocol the serde implementation cannot express; the first one broken
    /// is returned with its path.
    fn validate(&self) -> Result<(), Invalid>;

    /// Parse OTLP/JSON and validate the result.
    fn from_json(text: &str) -> Result<Self, Error> {
        let signal: Self = serde_json::from_str(text)?;
        signal.validate()?;
        Ok(signal)
    }

    /// The compact OTLP/JSON encoding.
    fn to_json(&self) -> String {
        serde_json::to_string(self).expect("the schema types serialize without error")
    }

    /// The OTLP/JSON encoding, indented for reading.
    fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("the schema types serialize without error")
    }
}

impl Signal for TracesData {
    fn validate(&self) -> Result<(), Invalid> {
        self.check()
    }
}

impl Signal for MetricsData {
    fn validate(&self) -> Result<(), Invalid> {
        self.check()
    }
}

impl Signal for LogsData {
    fn validate(&self) -> Result<(), Invalid> {
        self.check()
    }
}

/// The crate's name, as `CATALOG.toml` and crates.io know it.
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// The crate's version, as Cargo knows it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_follows_the_naming_rule() {
        assert_eq!(NAME, "pub-observe-schema");
        assert!(NAME.starts_with("pub-observe-"));
    }

    #[test]
    fn version_is_semver_shaped() {
        assert_eq!(VERSION.split('.').count(), 3, "{VERSION}");
    }

    #[test]
    fn the_pretty_form_decodes_too() {
        let logs = LogsData::default();
        assert_eq!(logs.to_json(), "{}");
        assert_eq!(LogsData::from_json(&logs.to_json_pretty()).unwrap(), logs);
    }
}

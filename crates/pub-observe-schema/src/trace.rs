//! The trace signal: spans grouped by resource and scope, with events, links and a status.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{InstrumentationScope, KeyValue};
use crate::error::Invalid;
use crate::ids::{SpanId, TraceId};
use crate::resource::Resource;
use crate::validate::{Path, unique_keys};
use crate::wire;

/// A traces payload: the body of an `ExportTraceServiceRequest`, what `/v1/traces` receives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TracesData {
    /// The spans, grouped by the resource that produced them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_spans: Vec<ResourceSpans>,
}

impl TracesData {
    pub(crate) fn check(&self) -> Result<(), Invalid> {
        let root = Path::root();
        for (i, group) in self.resource_spans.iter().enumerate() {
            group.check(&root.item("resourceSpans", i))?;
        }
        Ok(())
    }
}

/// The spans of one resource, grouped by instrumentation scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSpans {
    /// The resource; absent when no resource information is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<Resource>,
    /// The spans, grouped by scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope_spans: Vec<ScopeSpans>,
    /// The schema URL of the resource's attributes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_url: String,
}

impl ResourceSpans {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if let Some(resource) = &self.resource {
            resource.check(&at.field("resource"))?;
        }
        for (i, group) in self.scope_spans.iter().enumerate() {
            group.check(&at.item("scopeSpans", i))?;
        }
        Ok(())
    }
}

/// The spans of one instrumentation scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeSpans {
    /// The scope; absent is an empty scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<InstrumentationScope>,
    /// The spans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
    /// The schema URL of the scope's and the spans' attributes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_url: String,
}

impl ScopeSpans {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if let Some(scope) = &self.scope {
            unique_keys(&scope.attributes, &at.field("scope"))?;
        }
        for (i, span) in self.spans.iter().enumerate() {
            span.check(&at.item("spans", i))?;
        }
        Ok(())
    }
}

/// One operation within a trace.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    /// The trace this span belongs to; all zeros is invalid.
    #[serde(default, skip_serializing_if = "TraceId::is_zero")]
    pub trace_id: TraceId,
    /// The span's own identifier, unique within the trace; all zeros is invalid.
    #[serde(default, skip_serializing_if = "SpanId::is_zero")]
    pub span_id: SpanId,
    /// The W3C `tracestate` header value.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trace_state: String,
    /// The parent span; absent for a root span.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "wire::opt_id::deserialize"
    )]
    pub parent_span_id: Option<SpanId>,
    /// Bits 0–7 the W3C trace flags, bit 8 whether the remote state is known, bit 9 whether the
    /// parent is remote; see [`flags`](crate::flags).
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub flags: u32,
    /// The operation's name; empty means unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// What kind of operation the span records.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub kind: SpanKind,
    /// When the operation started, in nanoseconds since the Unix epoch.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub start_time_unix_nano: u64,
    /// When the operation ended, in nanoseconds since the Unix epoch; never before the start.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub end_time_unix_nano: u64,
    /// The span's attributes; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// How many attributes the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_attributes_count: u32,
    /// Time-stamped annotations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// How many events the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_events_count: u32,
    /// References to spans in this or another trace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    /// How many links the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_links_count: u32,
    /// The final status; absent means unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

impl Span {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if self.trace_id.is_zero() {
            return Err(at.refuse("traceId is all zeros"));
        }
        if self.span_id.is_zero() {
            return Err(at.refuse("spanId is all zeros"));
        }
        if self.parent_span_id.is_some_and(|id| id.is_zero()) {
            return Err(at.refuse("parentSpanId is all zeros"));
        }
        if self.end_time_unix_nano < self.start_time_unix_nano {
            return Err(at.refuse(format!(
                "endTimeUnixNano {} precedes startTimeUnixNano {}",
                self.end_time_unix_nano, self.start_time_unix_nano
            )));
        }
        unique_keys(&self.attributes, at)?;
        for (i, event) in self.events.iter().enumerate() {
            event.check(&at.item("events", i))?;
        }
        for (i, link) in self.links.iter().enumerate() {
            link.check(&at.item("links", i))?;
        }
        Ok(())
    }
}

/// What kind of operation a span records; an integer on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SpanKind {
    /// Unspecified; a consumer may treat it as [`Internal`](Self::Internal).
    #[default]
    Unspecified = 0,
    /// An operation within the application.
    Internal = 1,
    /// The handling of a remote request.
    Server = 2,
    /// A request to a remote service.
    Client = 3,
    /// A message sent to a broker.
    Producer = 4,
    /// A message received from a broker.
    Consumer = 5,
}

impl SpanKind {
    /// The kind for its protocol number, if there is one.
    pub const fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            0 => SpanKind::Unspecified,
            1 => SpanKind::Internal,
            2 => SpanKind::Server,
            3 => SpanKind::Client,
            4 => SpanKind::Producer,
            5 => SpanKind::Consumer,
            _ => return None,
        })
    }

    /// The protocol number of the kind.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

impl Serialize for SpanKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.as_u32())
    }
}

impl<'de> Deserialize<'de> for SpanKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        wire::deserialize_enum(deserializer, "kind", SpanKind::from_u32)
    }
}

/// A time-stamped annotation on a span.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// When the event happened, in nanoseconds since the Unix epoch.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub time_unix_nano: u64,
    /// The event's name; required and non-empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The event's attributes; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// How many attributes the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_attributes_count: u32,
}

impl Event {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if self.name.is_empty() {
            return Err(at.refuse("an event needs a name"));
        }
        unique_keys(&self.attributes, at)
    }
}

/// A reference from a span to another span, in the same trace or another.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    /// The trace of the linked span; all zeros is invalid.
    #[serde(default, skip_serializing_if = "TraceId::is_zero")]
    pub trace_id: TraceId,
    /// The linked span; all zeros is invalid.
    #[serde(default, skip_serializing_if = "SpanId::is_zero")]
    pub span_id: SpanId,
    /// The W3C `tracestate` of the linked span.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trace_state: String,
    /// The link's attributes; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// How many attributes the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_attributes_count: u32,
    /// The same bit field as [`Span::flags`], for the linked span.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub flags: u32,
}

impl Link {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if self.trace_id.is_zero() {
            return Err(at.refuse("traceId is all zeros"));
        }
        if self.span_id.is_zero() {
            return Err(at.refuse("spanId is all zeros"));
        }
        unique_keys(&self.attributes, at)
    }
}

/// The final status of a span.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// A developer-facing description, usually of an error.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// The status code.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub code: StatusCode,
}

/// The status code of a span; an integer on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StatusCode {
    /// No status was set.
    #[default]
    Unset = 0,
    /// The operation completed successfully, as judged by the developer or operator.
    Ok = 1,
    /// The operation failed.
    Error = 2,
}

impl StatusCode {
    /// The code for its protocol number, if there is one.
    pub const fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            0 => StatusCode::Unset,
            1 => StatusCode::Ok,
            2 => StatusCode::Error,
            _ => return None,
        })
    }

    /// The protocol number of the code.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

impl Serialize for StatusCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.as_u32())
    }
}

impl<'de> Deserialize<'de> for StatusCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        wire::deserialize_enum(deserializer, "status.code", StatusCode::from_u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_are_integers_both_ways() {
        for kind in [
            SpanKind::Unspecified,
            SpanKind::Internal,
            SpanKind::Server,
            SpanKind::Client,
            SpanKind::Producer,
            SpanKind::Consumer,
        ] {
            assert_eq!(SpanKind::from_u32(kind.as_u32()), Some(kind));
            let text = serde_json::to_string(&kind).unwrap();
            assert_eq!(text, kind.as_u32().to_string());
            assert_eq!(serde_json::from_str::<SpanKind>(&text).unwrap(), kind);
        }
        assert_eq!(SpanKind::from_u32(6), None);
        assert!(
            serde_json::from_str::<SpanKind>("\"SPAN_KIND_SERVER\"")
                .unwrap_err()
                .to_string()
                .contains("names are not allowed")
        );
        assert!(
            serde_json::from_str::<SpanKind>("-1")
                .unwrap_err()
                .to_string()
                .contains("kind -1")
        );
        for code in [StatusCode::Unset, StatusCode::Ok, StatusCode::Error] {
            assert_eq!(StatusCode::from_u32(code.as_u32()), Some(code));
            assert_eq!(
                serde_json::from_str::<StatusCode>(&code.as_u32().to_string()).unwrap(),
                code
            );
        }
        assert_eq!(StatusCode::from_u32(3), None);
    }

    #[test]
    fn a_default_span_is_written_empty_and_refused() {
        assert_eq!(serde_json::to_string(&Span::default()).unwrap(), "{}");
        let span: Span = serde_json::from_str("{}").unwrap();
        assert_eq!(span, Span::default());
        assert_eq!(
            span.check(&Path::root().field("span"))
                .unwrap_err()
                .to_string(),
            "span: traceId is all zeros"
        );
    }
}

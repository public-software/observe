# ADR-0001: The signal schema is the OpenTelemetry protocol's, and its JSON is OTLP/JSON

- Status: accepted
- Date: 2026-09-03
- Scope: this repository only (cross-repo decisions are RFCs in public-software/rfcs)

## Context

`observe` is the observability stack of the suite: the schema of metrics, traces and logs, the
exporters every other repository will emit through, and the dashboards and alerting product on
the Rust stores. Everything in it rests on one question: what is a metric point, a span, a log
record, and how does it travel? The first crate has to settle that, and settle it so that the
suite neither invents a telemetry dialect nor reads the incumbents: Grafana and Loki are AGPL-3
and nothing of them, of Prometheus, of the OpenTelemetry collector or of any SDK was consulted
(`PROVENANCE.md`). The OpenTelemetry protocol (OTLP) is an Apache-2.0 specification with stable
messages for the three signals and a stable JSON mapping, spoken by every SDK; a schema that is
OTLP's makes every exporter of the suite an OTLP producer and every store an OTLP receiver.

## Decision

1. **The model is OTLP's, message for message.** `Resource`, `InstrumentationScope`, `AnyValue`
   and `KeyValue`; `TracesData → ResourceSpans → ScopeSpans → Span` with events, links and a
   status; `MetricsData → ResourceMetrics → ScopeMetrics → Metric` with a gauge, a sum, a
   histogram, an exponential histogram or a summary and their data points and exemplars;
   `LogsData → ResourceLogs → ScopeLogs → LogRecord`. Field names, enum values and semantics are
   the protocol's; the Rust names are the messages' names. The dashboards product and the alert
   rules consume these types and nothing else, so an exporter and a dashboard never disagree on
   what a span is.

2. **The JSON encoding is the OTLP/JSON mapping, with its four deviations from proto3 JSON kept
   exactly.** Keys are lowerCamelCase; `traceId` and `spanId` are hex, read in either case and
   written in lowercase; enum values are integers only, a name is refused; 64-bit integers are
   written as decimal strings and read from strings or numbers; proto3 default values are left
   out when writing and assumed when reading; a receiver ignores fields it does not know. The
   crate is therefore a receiver of what any SDK sends and a producer of what any receiver
   accepts, verified by the protocol's own three example payloads as fixtures.

3. **Parsing is lenient where the protocol says so and strict where it says so; validation is a
   separate step.** Unknown fields and numbers-or-strings are the protocol's leniencies and are
   honoured. An identifier of the wrong length, an enum by name or an unknown enum number, an
   `AnyValue` with no value or two, are refused at parse time: they are not a valid encoding of
   any message. What the protocol calls invalid *data* (a span ending before it starts, an all-zero
   identifier, a sum or histogram with an unspecified temporality, a histogram whose bucket counts
   are not one more than its bounds or whose bounds do not increase, a summary quantile outside
   `[0, 1]`, an exponential scale outside `[-10, 20]`, duplicate attribute keys, a log record with
   a span but no trace, an unnamed event or metric) is refused by `validate`, which names the path
   in the mapping's own names (`resourceSpans[0].scopeSpans[0].spans[2]`) and the rule.
   `Signal::from_json` parses and validates; the serde implementations are public for a reader
   that wants the parse alone (a store ingesting best-effort, a debugging tool).

4. **`serde` and `serde_json` are the only dependencies.** Both are audited in the organization's
   vet store (trusted publishers and the Mozilla and Google pools, no exemption); base64 for
   `bytesValue` is forty lines in the crate rather than a third crate to audit.

5. **Out, by design, for now.** The profiles signal (still in development in the protocol); entity
   references on a resource (development); the string-table indices of the profiling extension of
   `AnyValue` and `KeyValue`; the `NaN` and `Infinity` string literals proto3 JSON allows for
   doubles (`serde_json` has none; a payload carrying them is refused at parse time); the protobuf
   binary encoding, the gRPC and HTTP transports, and the export request and response messages.
   Each is a later crate or a later chunk of this one; none changes the types above.

## Consequences

- Every exporter of the suite builds these types and calls `to_json`; every store and the
  dashboards daemon call `from_json`. Both ends speak OTLP without either having read an
  OpenTelemetry implementation.
- A validation error is actionable by a machine: the path and the rule are strings a dashboard
  can show or a collector can log without parsing prose.
- Adding a message field the protocol adds (they are additive by its stability guarantee) is a
  minor change here; a field this crate does not yet know is already ignored on the way in.
- The protobuf binary encoding, when it comes, is a second `Serialize`/`Deserialize` pair or a
  separate crate over the same types; the JSON mapping's deviations live in one module (`wire`)
  so they do not leak into that work.

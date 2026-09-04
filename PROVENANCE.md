# Provenance

This repository is a spec-first cleanroom implementation. Record here what was consulted.

## Specifications used
- OpenTelemetry Protocol (OTLP) specification, `docs/specification.md` of
  https://github.com/open-telemetry/opentelemetry-proto (Apache-2.0; consulted 2026-09-03): the
  section "JSON Protobuf Encoding" (hex `traceId`/`spanId`, integer-only enums, 64-bit integers as
  decimal strings, lowerCamelCase keys, unknown fields ignored), for the JSON encoding of
  `pub-observe-schema`.
- The OTLP message definitions of the same repository (Apache-2.0; consulted 2026-09-03):
  `opentelemetry/proto/common/v1/common.proto`, `resource/v1/resource.proto`,
  `trace/v1/trace.proto`, `metrics/v1/metrics.proto`, `logs/v1/logs.proto`: every message, field,
  enum value and the constraints stated in their comments (identifier lengths and the all-zero
  rule, the bucket-count and bound relation of a histogram, the temporalities, the severity
  numbers, the flag masks), for the types and the validation of `pub-observe-schema`.
- The example payloads `examples/trace.json`, `examples/metrics.json` and `examples/logs.json` of
  the same repository (Apache-2.0; consulted 2026-09-03), reproduced unchanged as the test fixtures
  under `crates/pub-observe-schema/tests/fixtures/`.
- RFC 4648, The Base16, Base32, and Base64 Data Encodings (IETF, https://www.rfc-editor.org/rfc/rfc4648;
  consulted 2026-09-03): §4 and the §10 test vectors, for the base64 of `bytesValue`.

## Behavioural references (cited, not copied)
- _none yet_ — list project, licence, and what behaviour was observed

## Copyleft sources
None consulted. Nothing of Grafana or Loki (AGPL-3) was opened, and nothing of Prometheus, the
OpenTelemetry collector or any OpenTelemetry SDK either: the schema is designed in
`docs/adr/0001-signal-schema.md` from the specification and the message definitions above. Contributors who have studied GPL/AGPL implementations of this domain do not author the corresponding modules (two-team rule; see the Charter §09).

## AI assistance
Prompts point at the specifications and conformance suites above, never at copyleft source. Generated code is reviewed against this list before merge.

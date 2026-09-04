# pub-observe-schema

The `schema` library of [observe](https://github.com/public-software/observe), part of Public Software. Kind: `lib`.

The typed model of the three signals the suite's exporters, stores and dashboards share: a resource
and an instrumentation scope, spans with events, links and a status, metrics as gauges, sums,
histograms, exponential histograms and summaries over their data points, and log records with a
severity, a body and their trace context. The model is the OpenTelemetry protocol's (listed in the
repository's `PROVENANCE.md`) and the JSON encoding is the OTLP/JSON mapping with its four
deviations from proto3 JSON (hex identifiers, integer-only enums, 64-bit integers as decimal
strings, unknown fields ignored), verified against the protocol's own example payloads.
`Signal::from_json` parses then validates, naming the path and the rule of what the protocol calls
invalid. Not yet: the profiles signal, entity references, the protobuf binary encoding, any
transport (ADR-0001).

```sh
cargo nextest run -p pub-observe-schema
```

Its entry in the repository's `CATALOG.toml`:

```toml
[[component]]
crate     = "pub-observe-schema"
kind      = "lib"
ledger    = "signal schema"
readiness = "seed"
effort    = 3
specs     = ["otlp"]
provides  = []
requires  = []
```

# Fixtures

`trace.json`, `metrics.json` and `logs.json` are the example OTLP/JSON payloads of
[open-telemetry/opentelemetry-proto](https://github.com/open-telemetry/opentelemetry-proto)
(`examples/`), Apache-2.0, reproduced unchanged; they are listed in the repository's
`PROVENANCE.md`. The tests decode them, encode them again and decode the result.

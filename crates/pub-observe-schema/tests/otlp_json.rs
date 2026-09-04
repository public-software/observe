//! The OTLP/JSON contract of `pub-observe-schema`: the specification's own example payloads decode,
//! re-encode and decode to the same value; the wire rules of the JSON mapping hold; the validation
//! refuses what the protocol calls invalid and names where.

use pub_observe_schema::{
    AggregationTemporality, AnyValue, Error, Histogram, HistogramDataPoint, KeyValue, LogRecord,
    LogsData, Metric, MetricData, MetricsData, NumberDataPoint, NumberValue, Resource,
    ResourceLogs, ResourceMetrics, ResourceSpans, ScopeLogs, ScopeMetrics, ScopeSpans, Severity,
    SeverityNumber, Signal, Span, SpanId, SpanKind, StatusCode, Sum, Summary, SummaryDataPoint,
    TraceId, TracesData, ValueAtQuantile,
};
use serde_json::Value;

const TRACE: &str = include_str!("fixtures/trace.json");
const METRICS: &str = include_str!("fixtures/metrics.json");
const LOGS: &str = include_str!("fixtures/logs.json");

// ---------- the specification's examples ----------

#[test]
fn the_trace_example_decodes_and_round_trips() {
    let traces = TracesData::from_json(TRACE).expect("the specification's trace example decodes");
    let span = &traces.resource_spans[0].scope_spans[0].spans[0];
    assert_eq!(span.name, "I'm a server span");
    assert_eq!(span.kind, SpanKind::Server);
    assert_eq!(
        span.trace_id.to_string(),
        "5b8efff798038103d269b633813fc60c"
    );
    assert_eq!(span.span_id.to_string(), "eee19b7ec3c1b174");
    assert_eq!(
        span.parent_span_id.map(|id| id.to_string()).as_deref(),
        Some("eee19b7ec3c1b173")
    );
    assert_eq!(span.start_time_unix_nano, 1_544_712_660_000_000_000);
    assert_eq!(span.end_time_unix_nano, 1_544_712_661_000_000_000);
    assert_eq!(span.attributes[0].key, "my.span.attr");
    let resource = traces.resource_spans[0]
        .resource
        .as_ref()
        .expect("resource");
    assert_eq!(
        resource.attributes[0].value,
        Some(AnyValue::from("my.service"))
    );
    let scope = traces.resource_spans[0].scope_spans[0]
        .scope
        .as_ref()
        .expect("scope");
    assert_eq!(
        (scope.name.as_str(), scope.version.as_str()),
        ("my.library", "1.0.0")
    );

    let again = TracesData::from_json(&traces.to_json()).expect("our own encoding decodes");
    assert_eq!(again, traces);
}

#[test]
fn the_metrics_example_decodes_and_round_trips() {
    let metrics =
        MetricsData::from_json(METRICS).expect("the specification's metrics example decodes");
    let list = &metrics.resource_metrics[0].scope_metrics[0].metrics;
    assert_eq!(list.len(), 4);
    match &list[0].data {
        Some(MetricData::Sum(sum)) => {
            assert_eq!(sum.aggregation_temporality, AggregationTemporality::Delta);
            assert!(sum.is_monotonic);
            assert_eq!(sum.data_points[0].value, Some(NumberValue::Double(5.0)));
        }
        other => panic!("my.counter is a sum, not {other:?}"),
    }
    match &list[1].data {
        Some(MetricData::Gauge(gauge)) => {
            assert_eq!(gauge.data_points[0].value, Some(NumberValue::Double(10.0)));
            assert_eq!(gauge.data_points[0].start_time_unix_nano, 0);
        }
        other => panic!("my.gauge is a gauge, not {other:?}"),
    }
    match &list[2].data {
        Some(MetricData::Histogram(histogram)) => {
            let point = &histogram.data_points[0];
            assert_eq!(point.count, 2);
            assert_eq!(point.sum, Some(2.0));
            assert_eq!(point.bucket_counts, vec![1, 1]);
            assert_eq!(point.explicit_bounds, vec![1.0]);
            assert_eq!((point.min, point.max), (Some(0.0), Some(2.0)));
        }
        other => panic!("my.histogram is a histogram, not {other:?}"),
    }
    match &list[3].data {
        Some(MetricData::ExponentialHistogram(histogram)) => {
            let point = &histogram.data_points[0];
            assert_eq!((point.count, point.zero_count, point.scale), (3, 1, 0));
            let positive = point.positive.as_ref().expect("positive buckets");
            assert_eq!(
                (positive.offset, positive.bucket_counts.as_slice()),
                (1, &[0, 2][..])
            );
            assert!(point.negative.is_none());
        }
        other => panic!("my.exponential.histogram is an exponential histogram, not {other:?}"),
    }

    let again = MetricsData::from_json(&metrics.to_json()).expect("our own encoding decodes");
    assert_eq!(again, metrics);
}

#[test]
fn the_logs_example_decodes_and_round_trips() {
    let logs = LogsData::from_json(LOGS).expect("the specification's logs example decodes");
    let record = &logs.resource_logs[0].scope_logs[0].log_records[0];
    assert_eq!(record.severity_number, SeverityNumber::INFO2);
    assert_eq!(record.severity_number.level(), Some(Severity::Info));
    assert_eq!(record.severity_text, "Information");
    assert_eq!(record.body, Some(AnyValue::from("Example log record")));
    assert_eq!(
        record.trace_id.map(|id| id.to_string()).as_deref(),
        Some("5b8efff798038103d269b633813fc60c")
    );
    assert_eq!(
        record.span_id.map(|id| id.to_string()).as_deref(),
        Some("eee19b7ec3c1b174")
    );
    let values: Vec<&AnyValue> = record
        .attributes
        .iter()
        .map(|kv| kv.value.as_ref().expect("value"))
        .collect();
    assert_eq!(values[0], &AnyValue::from("some string"));
    assert_eq!(values[1], &AnyValue::Bool(true));
    assert_eq!(values[2], &AnyValue::Int(10));
    assert_eq!(values[3], &AnyValue::Double(637.704));
    assert_eq!(
        values[4],
        &AnyValue::Array(vec![AnyValue::from("many"), AnyValue::from("values")])
    );
    assert_eq!(
        values[5],
        &AnyValue::KvList(vec![KeyValue::new("some.map.key", "some value")])
    );

    let again = LogsData::from_json(&logs.to_json()).expect("our own encoding decodes");
    assert_eq!(again, logs);
}

// ---------- the wire rules of the JSON mapping ----------

fn one_span() -> TracesData {
    let span = Span {
        trace_id: TraceId::new([
            0x5b, 0x8e, 0xff, 0xf7, 0x98, 0x03, 0x81, 0x03, 0xd2, 0x69, 0xb6, 0x33, 0x81, 0x3f,
            0xc6, 0x0c,
        ]),
        span_id: SpanId::new([0xee, 0xe1, 0x9b, 0x7e, 0xc3, 0xc1, 0xb1, 0x74]),
        name: "GET /".into(),
        kind: SpanKind::Server,
        start_time_unix_nano: 1_544_712_660_000_000_000,
        end_time_unix_nano: 1_544_712_661_000_000_000,
        ..Span::default()
    };
    TracesData {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![KeyValue::new("service.name", "web")],
                ..Resource::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: vec![span],
                ..ScopeSpans::default()
            }],
            ..ResourceSpans::default()
        }],
    }
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("valid JSON")
}

#[test]
fn the_encoding_follows_the_mapping() {
    let value = json(&one_span().to_json());
    let span = &value["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
    // lowerCamelCase keys, hex ids in lowercase, 64-bit integers as decimal strings, enums as integers
    assert_eq!(
        span["traceId"],
        Value::from("5b8efff798038103d269b633813fc60c")
    );
    assert_eq!(span["spanId"], Value::from("eee19b7ec3c1b174"));
    assert_eq!(
        span["startTimeUnixNano"],
        Value::from("1544712660000000000")
    );
    assert_eq!(span["endTimeUnixNano"], Value::from("1544712661000000000"));
    assert_eq!(span["kind"], Value::from(2));
    // proto3 defaults are omitted: no parent, no status, no attributes, no events, no flags
    for absent in [
        "parentSpanId",
        "status",
        "attributes",
        "events",
        "links",
        "flags",
        "droppedAttributesCount",
        "traceState",
    ] {
        assert!(
            span.get(absent).is_none(),
            "{absent} is a default and is not written: {span}"
        );
    }
    assert!(value["resourceSpans"][0].get("schemaUrl").is_none());
    assert!(
        value["resourceSpans"][0]["scopeSpans"][0]
            .get("scope")
            .is_none()
    );
    // no snake_case key anywhere
    let text = one_span().to_json();
    assert!(!text.contains('_'), "a snake_case key leaked: {text}");
}

#[test]
fn sixty_four_bit_fields_accept_numbers_and_strings() {
    let as_number = r#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"traceId":"5b8efff798038103d269b633813fc60c","spanId":"eee19b7ec3c1b174","name":"n","startTimeUnixNano":1544712660000000000,"endTimeUnixNano":1544712661000000000}]}]}]}"#;
    let as_string = as_number
        .replace("1544712660000000000", "\"1544712660000000000\"")
        .replace("1544712661000000000", "\"1544712661000000000\"");
    let a = TracesData::from_json(as_number).expect("a JSON number is accepted");
    let b = TracesData::from_json(&as_string).expect("a decimal string is accepted");
    assert_eq!(a, b);
    assert_eq!(
        a.resource_spans[0].scope_spans[0].spans[0].end_time_unix_nano,
        1_544_712_661_000_000_000
    );
    // the int value of an attribute likewise, and negative values keep their sign
    let record = r#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"attributes":[{"key":"a","value":{"intValue":-7}},{"key":"b","value":{"intValue":"-8"}}]}]}]}]}"#;
    let logs = LogsData::from_json(record).expect("int values decode");
    let attributes = &logs.resource_logs[0].scope_logs[0].log_records[0].attributes;
    assert_eq!(attributes[0].value, Some(AnyValue::Int(-7)));
    assert_eq!(attributes[1].value, Some(AnyValue::Int(-8)));
    assert!(
        logs.to_json().contains(r#""intValue":"-7""#),
        "int values are written as strings: {}",
        logs.to_json()
    );
}

#[test]
fn enum_names_are_rejected_and_unknown_values_too() {
    let named = r#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"traceId":"5b8efff798038103d269b633813fc60c","spanId":"eee19b7ec3c1b174","name":"n","kind":"SPAN_KIND_SERVER","startTimeUnixNano":"1","endTimeUnixNano":"2"}]}]}]}"#;
    let error = TracesData::from_json(named).expect_err("an enum name is not an integer");
    assert!(matches!(error, Error::Json(_)), "{error}");
    let unknown = named.replace("\"SPAN_KIND_SERVER\"", "9");
    let error = TracesData::from_json(&unknown).expect_err("9 is not a span kind");
    assert!(error.to_string().contains("kind"), "{error}");
    let status = named.replace("\"kind\":\"SPAN_KIND_SERVER\"", "\"status\":{\"code\":3}");
    let error = TracesData::from_json(&status).expect_err("3 is not a status code");
    assert!(error.to_string().contains("status"), "{error}");
}

#[test]
fn unknown_fields_are_ignored() {
    let with_extras = r#"{"resourceSpans":[{"futureField":1,"scopeSpans":[{"spans":[{"traceId":"5b8efff798038103d269b633813fc60c","spanId":"eee19b7ec3c1b174","name":"n","startTimeUnixNano":"1","endTimeUnixNano":"2","somethingNew":{"a":[1,2]}}]}]}],"alsoNew":true}"#;
    let traces = TracesData::from_json(with_extras).expect("unknown fields are ignored");
    assert_eq!(traces.resource_spans[0].scope_spans[0].spans[0].name, "n");
    // a metric with an unknown data kind is a metric without data
    let metric = r#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"name":"m","futureKind":{"dataPoints":[]}}]}]}]}"#;
    let metrics = MetricsData::from_json(metric).expect("unknown metric data is ignored");
    assert_eq!(
        metrics.resource_metrics[0].scope_metrics[0].metrics[0].data,
        None
    );
}

#[test]
fn ids_are_hex_of_the_right_length_in_either_case() {
    assert_eq!(
        TraceId::from_hex("5B8EFFF798038103D269B633813FC60C")
            .unwrap()
            .to_string(),
        "5b8efff798038103d269b633813fc60c"
    );
    assert_eq!(
        SpanId::from_hex("eee19b7ec3c1b174").unwrap().as_bytes(),
        &[0xee, 0xe1, 0x9b, 0x7e, 0xc3, 0xc1, 0xb1, 0x74]
    );
    for bad in [
        "",
        "5b8efff7",
        "5b8efff798038103d269b633813fc60c00",
        "5b8efff798038103d269b633813fc60g",
    ] {
        let error = TraceId::from_hex(bad).expect_err(bad);
        assert!(
            error.to_string().contains("32 hexadecimal digits"),
            "{error}"
        );
    }
    let error = SpanId::from_hex("eee19b7ec3c1b17").expect_err("15 digits");
    assert!(
        error.to_string().contains("16 hexadecimal digits"),
        "{error}"
    );
    assert!(TraceId::new([0; 16]).is_zero());
    assert!(!SpanId::new([0, 0, 0, 0, 0, 0, 0, 1]).is_zero());
    // on the wire: a wrong length is a decoding error; an empty optional id is absent
    let short = r#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"traceId":"5b8e","spanId":"eee19b7ec3c1b174","name":"n","startTimeUnixNano":"1","endTimeUnixNano":"2"}]}]}]}"#;
    let error = TracesData::from_json(short).expect_err("a 4-digit trace id");
    assert!(
        matches!(error, Error::Json(_)) && error.to_string().contains("32 hexadecimal digits"),
        "{error}"
    );
    let empty_parent = short.replace(
        "\"5b8e\"",
        "\"5b8efff798038103d269b633813fc60c\",\"parentSpanId\":\"\"",
    );
    let traces = TracesData::from_json(&empty_parent).expect("an empty parent is no parent");
    assert_eq!(
        traces.resource_spans[0].scope_spans[0].spans[0].parent_span_id,
        None
    );
}

#[test]
fn bytes_values_travel_as_base64() {
    let record = LogRecord {
        body: Some(AnyValue::Bytes(vec![0x00, 0xff, 0x10, 0x20, 0x30])),
        ..LogRecord::default()
    };
    let logs = LogsData {
        resource_logs: vec![ResourceLogs {
            scope_logs: vec![ScopeLogs {
                log_records: vec![record],
                ..ScopeLogs::default()
            }],
            ..ResourceLogs::default()
        }],
    };
    let text = logs.to_json();
    assert!(text.contains(r#""bytesValue":"AP8QIDA=""#), "{text}");
    assert_eq!(LogsData::from_json(&text).unwrap(), logs);
    // the URL-safe alphabet and a missing pad are accepted on input
    let lenient = text.replace("AP8QIDA=", "AP8QIDA");
    assert_eq!(LogsData::from_json(&lenient).unwrap(), logs);
    let error = LogsData::from_json(&text.replace("AP8QIDA=", "A?8QIDA=")).expect_err("not base64");
    assert!(error.to_string().contains("base64"), "{error}");
}

#[test]
fn severity_numbers_map_to_six_levels() {
    assert_eq!(SeverityNumber::UNSPECIFIED.level(), None);
    assert_eq!(
        SeverityNumber::new(1).unwrap().level(),
        Some(Severity::Trace)
    );
    assert_eq!(
        SeverityNumber::new(8).unwrap().level(),
        Some(Severity::Debug)
    );
    assert_eq!(
        SeverityNumber::new(9).unwrap().level(),
        Some(Severity::Info)
    );
    assert_eq!(
        SeverityNumber::new(16).unwrap().level(),
        Some(Severity::Warn)
    );
    assert_eq!(
        SeverityNumber::new(17).unwrap().level(),
        Some(Severity::Error)
    );
    assert_eq!(
        SeverityNumber::new(24).unwrap().level(),
        Some(Severity::Fatal)
    );
    assert_eq!(SeverityNumber::new(25), None);
    assert_eq!(SeverityNumber::FATAL4.as_u8(), 24);
    assert_eq!(Severity::Warn.to_string(), "WARN");
    let record = r#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"severityNumber":25}]}]}]}"#;
    let error = LogsData::from_json(record).expect_err("25 is outside the range");
    assert!(error.to_string().contains("severityNumber"), "{error}");
}

// ---------- validation ----------

fn invalid(error: impl Into<Error>) -> (String, String) {
    match error.into() {
        Error::Invalid(invalid) => (invalid.at, invalid.reason),
        Error::Json(json) => panic!("expected a validation error, got a decoding error: {json}"),
    }
}

#[test]
fn a_span_whose_end_precedes_its_start_is_rejected() {
    let mut traces = one_span();
    let span = &mut traces.resource_spans[0].scope_spans[0].spans[0];
    span.start_time_unix_nano = 20;
    span.end_time_unix_nano = 10;
    let (at, reason) = invalid(traces.validate().expect_err("end before start"));
    assert_eq!(at, "resourceSpans[0].scopeSpans[0].spans[0]");
    assert!(
        reason.contains("endTimeUnixNano") && reason.contains("precedes"),
        "{reason}"
    );
    let (at, _) =
        invalid(TracesData::from_json(&traces.to_json()).expect_err("from_json validates too"));
    assert_eq!(at, "resourceSpans[0].scopeSpans[0].spans[0]");
    // an end equal to the start (a zero-length span) is fine
    traces.resource_spans[0].scope_spans[0].spans[0].end_time_unix_nano = 20;
    traces.validate().expect("equal times are allowed");
}

#[test]
fn zero_ids_are_rejected_where_the_protocol_requires_one() {
    let mut traces = one_span();
    traces.resource_spans[0].scope_spans[0].spans[0].trace_id = TraceId::new([0; 16]);
    let (at, reason) = invalid(traces.validate().expect_err("zero trace id"));
    assert_eq!(
        (at.as_str(), reason.as_str()),
        (
            "resourceSpans[0].scopeSpans[0].spans[0]",
            "traceId is all zeros"
        )
    );

    let mut traces = one_span();
    traces.resource_spans[0].scope_spans[0].spans[0].span_id = SpanId::new([0; 8]);
    let (_, reason) = invalid(traces.validate().expect_err("zero span id"));
    assert_eq!(reason, "spanId is all zeros");

    let mut traces = one_span();
    traces.resource_spans[0].scope_spans[0].spans[0]
        .links
        .push(pub_observe_schema::Link {
            trace_id: TraceId::new([1; 16]),
            span_id: SpanId::new([0; 8]),
            ..Default::default()
        });
    let (at, reason) = invalid(traces.validate().expect_err("a link to a zero span id"));
    assert_eq!(
        (at.as_str(), reason.as_str()),
        (
            "resourceSpans[0].scopeSpans[0].spans[0].links[0]",
            "spanId is all zeros"
        )
    );

    // a log record may carry no ids, but a span id needs its trace id
    let record = LogRecord {
        span_id: Some(SpanId::new([1; 8])),
        ..LogRecord::default()
    };
    let logs = LogsData {
        resource_logs: vec![ResourceLogs {
            scope_logs: vec![ScopeLogs {
                log_records: vec![record],
                ..ScopeLogs::default()
            }],
            ..ResourceLogs::default()
        }],
    };
    let (at, reason) = invalid(logs.validate().expect_err("span id without trace id"));
    assert_eq!(
        (at.as_str(), reason.as_str()),
        (
            "resourceLogs[0].scopeLogs[0].logRecords[0]",
            "spanId without traceId"
        )
    );
    LogsData::from_json(r#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"body":{"stringValue":"no ids at all"}}]}]}]}"#)
        .expect("a record without ids is valid");
}

#[test]
fn an_event_needs_a_name_and_a_status_code_is_checked() {
    let mut traces = one_span();
    traces.resource_spans[0].scope_spans[0].spans[0]
        .events
        .push(pub_observe_schema::Event {
            time_unix_nano: 5,
            ..Default::default()
        });
    let (at, reason) = invalid(traces.validate().expect_err("an unnamed event"));
    assert_eq!(at, "resourceSpans[0].scopeSpans[0].spans[0].events[0]");
    assert!(reason.contains("name"), "{reason}");
    let mut traces = one_span();
    traces.resource_spans[0].scope_spans[0].spans[0].status = Some(pub_observe_schema::Status {
        code: StatusCode::Ok,
        message: "fine".into(),
    });
    traces
        .validate()
        .expect("an OK status with a message is allowed");
}

fn one_metric(metric: Metric) -> MetricsData {
    MetricsData {
        resource_metrics: vec![ResourceMetrics {
            scope_metrics: vec![ScopeMetrics {
                metrics: vec![metric],
                ..ScopeMetrics::default()
            }],
            ..ResourceMetrics::default()
        }],
    }
}

#[test]
fn a_sum_or_histogram_needs_a_temporality() {
    let sum = Metric {
        name: "requests".into(),
        data: Some(MetricData::Sum(Sum {
            data_points: vec![NumberDataPoint {
                time_unix_nano: 1,
                value: Some(NumberValue::Int(3)),
                ..Default::default()
            }],
            aggregation_temporality: AggregationTemporality::Unspecified,
            is_monotonic: true,
        })),
        ..Metric::default()
    };
    let (at, reason) = invalid(
        one_metric(sum)
            .validate()
            .expect_err("unspecified temporality"),
    );
    assert_eq!(
        (at.as_str(), reason.as_str()),
        (
            "resourceMetrics[0].scopeMetrics[0].metrics[0].sum",
            "aggregationTemporality is unspecified"
        )
    );
    let histogram = Metric {
        name: "latency".into(),
        data: Some(MetricData::Histogram(Histogram {
            data_points: vec![],
            aggregation_temporality: AggregationTemporality::Unspecified,
        })),
        ..Metric::default()
    };
    let (at, _) = invalid(
        one_metric(histogram)
            .validate()
            .expect_err("unspecified temporality"),
    );
    assert_eq!(
        at,
        "resourceMetrics[0].scopeMetrics[0].metrics[0].histogram"
    );
    // a gauge has no temporality and an unnamed metric is refused
    let unnamed = Metric {
        data: Some(MetricData::Gauge(Default::default())),
        ..Metric::default()
    };
    let (at, reason) = invalid(one_metric(unnamed).validate().expect_err("unnamed"));
    assert_eq!(at, "resourceMetrics[0].scopeMetrics[0].metrics[0]");
    assert!(reason.contains("name"), "{reason}");
}

#[test]
fn histogram_buckets_and_bounds_are_checked() {
    let point = |counts: Vec<u64>, bounds: Vec<f64>| Metric {
        name: "latency".into(),
        data: Some(MetricData::Histogram(Histogram {
            data_points: vec![HistogramDataPoint {
                time_unix_nano: 1,
                count: counts.iter().sum(),
                bucket_counts: counts,
                explicit_bounds: bounds,
                ..Default::default()
            }],
            aggregation_temporality: AggregationTemporality::Cumulative,
        })),
        ..Metric::default()
    };
    one_metric(point(vec![1, 2, 3], vec![10.0, 20.0]))
        .validate()
        .expect("three buckets, two bounds");
    one_metric(point(vec![4], vec![]))
        .validate()
        .expect("one bucket, no bounds");
    one_metric(point(vec![], vec![]))
        .validate()
        .expect("no buckets at all is allowed");
    let (at, reason) = invalid(
        one_metric(point(vec![1, 2], vec![10.0, 20.0]))
            .validate()
            .expect_err("two buckets, two bounds"),
    );
    assert_eq!(
        at,
        "resourceMetrics[0].scopeMetrics[0].metrics[0].histogram.dataPoints[0]"
    );
    assert!(
        reason.contains("bucketCounts") && reason.contains("explicitBounds"),
        "{reason}"
    );
    let (_, reason) = invalid(
        one_metric(point(vec![1, 2, 3], vec![20.0, 10.0]))
            .validate()
            .expect_err("bounds not increasing"),
    );
    assert!(reason.contains("strictly increasing"), "{reason}");
    let (_, reason) = invalid(
        one_metric(point(vec![1, 2, 3], vec![10.0, 10.0]))
            .validate()
            .expect_err("equal bounds"),
    );
    assert!(reason.contains("strictly increasing"), "{reason}");
    let mut mismatched = point(vec![1, 2, 1], vec![10.0, 20.0]);
    match &mut mismatched.data {
        Some(MetricData::Histogram(histogram)) => histogram.data_points[0].count = 99,
        _ => unreachable!(),
    }
    let (_, reason) = invalid(
        one_metric(mismatched)
            .validate()
            .expect_err("count is the sum of the buckets"),
    );
    assert!(reason.contains("count 99"), "{reason}");
}

#[test]
fn summary_quantiles_stay_in_the_unit_interval() {
    let summary = |quantile: f64| Metric {
        name: "latency".into(),
        data: Some(MetricData::Summary(Summary {
            data_points: vec![SummaryDataPoint {
                time_unix_nano: 1,
                quantile_values: vec![ValueAtQuantile {
                    quantile,
                    value: 1.0,
                }],
                ..Default::default()
            }],
        })),
        ..Metric::default()
    };
    one_metric(summary(0.99)).validate().expect("0.99");
    one_metric(summary(1.0)).validate().expect("1.0");
    let (at, reason) = invalid(one_metric(summary(1.5)).validate().expect_err("1.5"));
    assert_eq!(
        at,
        "resourceMetrics[0].scopeMetrics[0].metrics[0].summary.dataPoints[0].quantileValues[0]"
    );
    assert!(reason.contains("[0, 1]"), "{reason}");
}

#[test]
fn duplicate_attribute_keys_are_rejected_everywhere() {
    let mut traces = one_span();
    traces.resource_spans[0]
        .resource
        .as_mut()
        .unwrap()
        .attributes
        .push(KeyValue::new("service.name", "twice"));
    let (at, reason) = invalid(traces.validate().expect_err("duplicate resource key"));
    assert_eq!(
        (at.as_str(), reason.as_str()),
        (
            "resourceSpans[0].resource",
            "duplicate attribute key \"service.name\""
        )
    );
    let mut traces = one_span();
    let span = &mut traces.resource_spans[0].scope_spans[0].spans[0];
    span.attributes.push(KeyValue::new("http.method", "GET"));
    span.attributes.push(KeyValue::new("http.method", "POST"));
    let (at, _) = invalid(traces.validate().expect_err("duplicate span key"));
    assert_eq!(at, "resourceSpans[0].scopeSpans[0].spans[0]");
    let point = NumberDataPoint {
        time_unix_nano: 1,
        attributes: vec![KeyValue::new("host", "a"), KeyValue::new("host", "b")],
        value: Some(NumberValue::Double(1.0)),
        ..Default::default()
    };
    let gauge = Metric {
        name: "g".into(),
        data: Some(MetricData::Gauge(pub_observe_schema::Gauge {
            data_points: vec![point],
        })),
        ..Metric::default()
    };
    let (at, _) = invalid(
        one_metric(gauge)
            .validate()
            .expect_err("duplicate point key"),
    );
    assert_eq!(
        at,
        "resourceMetrics[0].scopeMetrics[0].metrics[0].gauge.dataPoints[0]"
    );
}

#[test]
fn an_any_value_needs_exactly_one_value() {
    let error =
        LogsData::from_json(r#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"body":{}}]}]}]}"#)
            .expect_err("an empty AnyValue");
    assert!(matches!(error, Error::Json(_)), "{error}");
    let error = LogsData::from_json(r#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"body":{"stringValue":"a","boolValue":true}}]}]}]}"#).expect_err("two values");
    assert!(matches!(error, Error::Json(_)), "{error}");
}

#[test]
fn errors_display_their_path_and_reason() {
    let mut traces = one_span();
    traces.resource_spans[0].scope_spans[0].spans[0].end_time_unix_nano = 1;
    let error = traces.validate().expect_err("end before start");
    let text = error.to_string();
    assert!(
        text.starts_with("resourceSpans[0].scopeSpans[0].spans[0]: "),
        "{text}"
    );
    assert!(std::error::Error::source(&error).is_none());
    let json = TracesData::from_json("{").expect_err("truncated");
    assert!(
        std::error::Error::source(&json).is_some(),
        "the serde_json error is the source"
    );
}

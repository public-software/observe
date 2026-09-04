//! The metrics signal: metrics grouped by resource and scope, each a gauge, a sum, a histogram, an
//! exponential histogram or a summary over its data points.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{InstrumentationScope, KeyValue};
use crate::error::Invalid;
use crate::ids::{SpanId, TraceId};
use crate::resource::Resource;
use crate::validate::{Path, unique_keys};
use crate::wire;

/// A metrics payload: the body of an `ExportMetricsServiceRequest`, what `/v1/metrics` receives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsData {
    /// The metrics, grouped by the resource that produced them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_metrics: Vec<ResourceMetrics>,
}

impl MetricsData {
    pub(crate) fn check(&self) -> Result<(), Invalid> {
        let root = Path::root();
        for (i, group) in self.resource_metrics.iter().enumerate() {
            group.check(&root.item("resourceMetrics", i))?;
        }
        Ok(())
    }
}

/// The metrics of one resource, grouped by instrumentation scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetrics {
    /// The resource; absent when no resource information is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<Resource>,
    /// The metrics, grouped by scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope_metrics: Vec<ScopeMetrics>,
    /// The schema URL of the resource's attributes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_url: String,
}

impl ResourceMetrics {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if let Some(resource) = &self.resource {
            resource.check(&at.field("resource"))?;
        }
        for (i, group) in self.scope_metrics.iter().enumerate() {
            group.check(&at.item("scopeMetrics", i))?;
        }
        Ok(())
    }
}

/// The metrics of one instrumentation scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeMetrics {
    /// The scope; absent is an empty scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<InstrumentationScope>,
    /// The metrics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<Metric>,
    /// The schema URL of the scope's and the metrics' attributes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_url: String,
}

impl ScopeMetrics {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if let Some(scope) = &self.scope {
            unique_keys(&scope.attributes, &at.field("scope"))?;
        }
        for (i, metric) in self.metrics.iter().enumerate() {
            metric.check(&at.item("metrics", i))?;
        }
        Ok(())
    }
}

/// One metric: a name, a unit, a description and its data of one of five kinds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    /// The metric's name; required.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// What the metric measures, for documentation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The unit, in UCUM form (`ms`, `By`, `1`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// The data: which kind of aggregation and its points. Absent when the payload named none, or
    /// named a kind this crate does not know.
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub data: Option<MetricData>,
    /// Non-identifying metadata about the metric; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<KeyValue>,
}

impl Metric {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        if self.name.is_empty() {
            return Err(at.refuse("a metric needs a name"));
        }
        unique_keys(&self.metadata, at)?;
        match &self.data {
            None => Ok(()),
            Some(MetricData::Gauge(gauge)) => {
                let at = at.field("gauge");
                check_number_points(&gauge.data_points, &at)
            }
            Some(MetricData::Sum(sum)) => {
                let at = at.field("sum");
                check_temporality(sum.aggregation_temporality, &at)?;
                check_number_points(&sum.data_points, &at)
            }
            Some(MetricData::Histogram(histogram)) => {
                let at = at.field("histogram");
                check_temporality(histogram.aggregation_temporality, &at)?;
                for (i, point) in histogram.data_points.iter().enumerate() {
                    point.check(&at.item("dataPoints", i))?;
                }
                Ok(())
            }
            Some(MetricData::ExponentialHistogram(histogram)) => {
                let at = at.field("exponentialHistogram");
                check_temporality(histogram.aggregation_temporality, &at)?;
                for (i, point) in histogram.data_points.iter().enumerate() {
                    point.check(&at.item("dataPoints", i))?;
                }
                Ok(())
            }
            Some(MetricData::Summary(summary)) => {
                let at = at.field("summary");
                for (i, point) in summary.data_points.iter().enumerate() {
                    point.check(&at.item("dataPoints", i))?;
                }
                Ok(())
            }
        }
    }
}

fn check_temporality(temporality: AggregationTemporality, at: &Path) -> Result<(), Invalid> {
    if temporality == AggregationTemporality::Unspecified {
        return Err(at.refuse("aggregationTemporality is unspecified"));
    }
    Ok(())
}

fn check_number_points(points: &[NumberDataPoint], at: &Path) -> Result<(), Invalid> {
    for (i, point) in points.iter().enumerate() {
        let at = at.item("dataPoints", i);
        unique_keys(&point.attributes, &at)?;
        check_exemplars(&point.exemplars, &at)?;
    }
    Ok(())
}

fn check_exemplars(exemplars: &[Exemplar], at: &Path) -> Result<(), Invalid> {
    for (i, exemplar) in exemplars.iter().enumerate() {
        let at = at.item("exemplars", i);
        unique_keys(&exemplar.filtered_attributes, &at)?;
        if exemplar.trace_id.is_some_and(|id| id.is_zero()) {
            return Err(at.refuse("traceId is all zeros"));
        }
        if exemplar.span_id.is_some_and(|id| id.is_zero()) {
            return Err(at.refuse("spanId is all zeros"));
        }
    }
    Ok(())
}

/// The data of a metric: the kind of aggregation and its points. On the wire the kind is the key
/// (`gauge`, `sum`, `histogram`, `exponentialHistogram`, `summary`) beside the metric's name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricData {
    /// A value sampled at a point in time.
    Gauge(Gauge),
    /// A sum, monotonic or not, delta or cumulative.
    Sum(Sum),
    /// A histogram over explicit bucket bounds.
    Histogram(Histogram),
    /// A histogram over exponential (base-2 scaled) buckets.
    ExponentialHistogram(ExponentialHistogram),
    /// Quantile values over a population.
    Summary(Summary),
}

/// A value sampled at a point in time: the last value wins, no temporality.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gauge {
    /// The points, one per attribute set and time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_points: Vec<NumberDataPoint>,
}

/// A sum over a population: a counter when monotonic, an up-down counter otherwise.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sum {
    /// The points, one per attribute set and time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_points: Vec<NumberDataPoint>,
    /// Whether each point reports the change since the last one or since the start; never
    /// unspecified.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub aggregation_temporality: AggregationTemporality,
    /// Whether the sum never decreases.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub is_monotonic: bool,
}

/// A histogram over explicit bucket bounds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Histogram {
    /// The points, one per attribute set and time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_points: Vec<HistogramDataPoint>,
    /// Delta or cumulative; never unspecified.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub aggregation_temporality: AggregationTemporality,
}

/// A histogram over exponential buckets: bucket `i` covers `(base^i, base^(i+1)]` with
/// `base = 2^(2^-scale)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExponentialHistogram {
    /// The points, one per attribute set and time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_points: Vec<ExponentialHistogramDataPoint>,
    /// Delta or cumulative; never unspecified.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub aggregation_temporality: AggregationTemporality,
}

/// Quantile values over a population, as legacy instrumentation reports them; always cumulative.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// The points, one per attribute set and time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_points: Vec<SummaryDataPoint>,
}

/// How a point relates to the ones before it; an integer on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AggregationTemporality {
    /// Not stated; invalid on a sum or a histogram.
    #[default]
    Unspecified = 0,
    /// Each point reports the change since the previous point.
    Delta = 1,
    /// Each point reports the total since a fixed start time.
    Cumulative = 2,
}

impl AggregationTemporality {
    /// The temporality for its protocol number, if there is one.
    pub const fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            0 => AggregationTemporality::Unspecified,
            1 => AggregationTemporality::Delta,
            2 => AggregationTemporality::Cumulative,
            _ => return None,
        })
    }

    /// The protocol number of the temporality.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

impl Serialize for AggregationTemporality {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.as_u32())
    }
}

impl<'de> Deserialize<'de> for AggregationTemporality {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        wire::deserialize_enum(
            deserializer,
            "aggregationTemporality",
            AggregationTemporality::from_u32,
        )
    }
}

/// The value of a number point or an exemplar: a double or a 64-bit integer. On the wire the key
/// says which, `asDouble` or `asInt` (the integer as a decimal string).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum NumberValue {
    /// A double.
    #[serde(rename = "asDouble")]
    Double(f64),
    /// A 64-bit signed integer.
    #[serde(rename = "asInt")]
    Int(#[serde(with = "wire::i64_str")] i64),
}

/// One point of a gauge or a sum.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberDataPoint {
    /// The attributes identifying the time series; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// The start of the interval the point covers, in nanoseconds since the Unix epoch; zero when
    /// unknown.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub start_time_unix_nano: u64,
    /// The end of the interval, in nanoseconds since the Unix epoch.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub time_unix_nano: u64,
    /// The value; absent when the payload named none.
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub value: Option<NumberValue>,
    /// Sampled measurements that contributed to the point.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exemplars: Vec<Exemplar>,
    /// The point's flags; see [`flags::NO_RECORDED_VALUE_MASK`](crate::flags::NO_RECORDED_VALUE_MASK).
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub flags: u32,
}

/// One point of a histogram over explicit bounds: `bucketCounts` has one entry more than
/// `explicitBounds`, the last bucket reaching to infinity.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistogramDataPoint {
    /// The attributes identifying the time series; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// The start of the interval the point covers; zero when unknown.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub start_time_unix_nano: u64,
    /// The end of the interval.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub time_unix_nano: u64,
    /// How many values the population holds; the sum of the bucket counts.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub count: u64,
    /// The sum of the values, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sum: Option<f64>,
    /// The count per bucket; empty, or one entry more than `explicit_bounds`.
    #[serde(
        default,
        with = "wire::vec_u64_str",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub bucket_counts: Vec<u64>,
    /// The upper bounds of the buckets, strictly increasing; bucket `i` covers
    /// `(bounds[i-1], bounds[i]]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explicit_bounds: Vec<f64>,
    /// Sampled measurements that contributed to the point.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exemplars: Vec<Exemplar>,
    /// The point's flags.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub flags: u32,
    /// The smallest value, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// The largest value, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl HistogramDataPoint {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        unique_keys(&self.attributes, at)?;
        if !self.bucket_counts.is_empty() {
            if self.bucket_counts.len() != self.explicit_bounds.len() + 1 {
                return Err(at.refuse(format!(
                    "bucketCounts has {} entries and explicitBounds {}: a histogram has one bucket more than bounds",
                    self.bucket_counts.len(),
                    self.explicit_bounds.len()
                )));
            }
            let total = self
                .bucket_counts
                .iter()
                .try_fold(0u64, |acc, n| acc.checked_add(*n));
            if total != Some(self.count) {
                return Err(at.refuse(format!(
                    "count {} is not the sum of bucketCounts",
                    self.count
                )));
            }
        }
        check_increasing(&self.explicit_bounds, at)?;
        check_exemplars(&self.exemplars, at)
    }
}

fn check_increasing(bounds: &[f64], at: &Path) -> Result<(), Invalid> {
    for (i, pair) in bounds.windows(2).enumerate() {
        if pair[0].partial_cmp(&pair[1]) != Some(std::cmp::Ordering::Less) {
            return Err(at.refuse(format!(
                "explicitBounds is not strictly increasing at index {}",
                i + 1
            )));
        }
    }
    Ok(())
}

/// One point of an exponential histogram.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExponentialHistogramDataPoint {
    /// The attributes identifying the time series; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// The start of the interval the point covers; zero when unknown.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub start_time_unix_nano: u64,
    /// The end of the interval.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub time_unix_nano: u64,
    /// How many values the population holds, the zero bucket included.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub count: u64,
    /// The sum of the values, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sum: Option<f64>,
    /// The resolution: bucket bounds grow by `2^(2^-scale)`; between −10 and 20.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub scale: i32,
    /// How many values were zero, or within `zero_threshold` of it.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub zero_count: u64,
    /// The buckets of the positive values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positive: Option<Buckets>,
    /// The buckets of the negative values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative: Option<Buckets>,
    /// The point's flags.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub flags: u32,
    /// Sampled measurements that contributed to the point.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exemplars: Vec<Exemplar>,
    /// The smallest value, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// The largest value, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The half-width of the zero bucket.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub zero_threshold: f64,
}

impl ExponentialHistogramDataPoint {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        unique_keys(&self.attributes, at)?;
        if !(-10..=20).contains(&self.scale) {
            return Err(at.refuse(format!("scale {} is outside [-10, 20]", self.scale)));
        }
        check_exemplars(&self.exemplars, at)
    }
}

/// A run of exponential buckets starting at index `offset`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Buckets {
    /// The index of the first bucket in `bucket_counts`.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub offset: i32,
    /// The count per bucket from `offset` on.
    #[serde(
        default,
        with = "wire::vec_u64_str",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub bucket_counts: Vec<u64>,
}

/// One point of a summary.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDataPoint {
    /// The attributes identifying the time series; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// The start of the interval the point covers; zero when unknown.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub start_time_unix_nano: u64,
    /// The end of the interval.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub time_unix_nano: u64,
    /// How many values the population holds.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub count: u64,
    /// The sum of the values.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub sum: f64,
    /// The values at the reported quantiles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantile_values: Vec<ValueAtQuantile>,
    /// The point's flags.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub flags: u32,
}

impl SummaryDataPoint {
    fn check(&self, at: &Path) -> Result<(), Invalid> {
        unique_keys(&self.attributes, at)?;
        for (i, entry) in self.quantile_values.iter().enumerate() {
            if !(0.0..=1.0).contains(&entry.quantile) {
                return Err(at
                    .item("quantileValues", i)
                    .refuse(format!("quantile {} is outside [0, 1]", entry.quantile)));
            }
        }
        Ok(())
    }
}

/// The value at one quantile of a summary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueAtQuantile {
    /// The quantile, between 0 and 1 inclusive.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub quantile: f64,
    /// The value at that quantile.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub value: f64,
}

/// One sampled measurement, with the trace context it was taken in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exemplar {
    /// The measurement's attributes the aggregation dropped; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filtered_attributes: Vec<KeyValue>,
    /// When the measurement was taken, in nanoseconds since the Unix epoch.
    #[serde(
        default,
        with = "wire::u64_str",
        skip_serializing_if = "wire::is_default"
    )]
    pub time_unix_nano: u64,
    /// The measured value.
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub value: Option<NumberValue>,
    /// The span active when the measurement was taken, if any.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "wire::opt_id::deserialize"
    )]
    pub span_id: Option<SpanId>,
    /// The trace active when the measurement was taken, if any.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "wire::opt_id::deserialize"
    )]
    pub trace_id: Option<TraceId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporality_is_an_integer_both_ways() {
        for value in [
            AggregationTemporality::Unspecified,
            AggregationTemporality::Delta,
            AggregationTemporality::Cumulative,
        ] {
            assert_eq!(
                AggregationTemporality::from_u32(value.as_u32()),
                Some(value)
            );
            assert_eq!(
                serde_json::from_str::<AggregationTemporality>(&value.as_u32().to_string())
                    .unwrap(),
                value
            );
        }
        assert_eq!(AggregationTemporality::from_u32(3), None);
        assert!(
            serde_json::from_str::<AggregationTemporality>("\"AGGREGATION_TEMPORALITY_DELTA\"")
                .is_err()
        );
    }

    #[test]
    fn the_number_value_is_flattened_beside_the_times() {
        let point = NumberDataPoint {
            time_unix_nano: 7,
            value: Some(NumberValue::Int(-2)),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&point).unwrap(),
            r#"{"timeUnixNano":"7","asInt":"-2"}"#
        );
        assert_eq!(
            serde_json::from_str::<NumberDataPoint>(r#"{"timeUnixNano":"7","asInt":"-2"}"#)
                .unwrap(),
            point
        );
        let none = NumberDataPoint {
            time_unix_nano: 7,
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&none).unwrap(),
            r#"{"timeUnixNano":"7"}"#
        );
        assert_eq!(
            serde_json::from_str::<NumberDataPoint>(r#"{"timeUnixNano":7}"#).unwrap(),
            none
        );
        let double = serde_json::from_str::<NumberDataPoint>(r#"{"asDouble":2.5}"#).unwrap();
        assert_eq!(double.value, Some(NumberValue::Double(2.5)));
    }

    #[test]
    fn the_metric_data_is_flattened_beside_the_name() {
        let metric = Metric {
            name: "m".into(),
            data: Some(MetricData::Gauge(Gauge::default())),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&metric).unwrap(),
            r#"{"name":"m","gauge":{}}"#
        );
        assert_eq!(
            serde_json::from_str::<Metric>(r#"{"name":"m","gauge":{}}"#).unwrap(),
            metric
        );
        let bare = serde_json::from_str::<Metric>(r#"{"name":"m"}"#).unwrap();
        assert_eq!(bare.data, None);
        assert_eq!(serde_json::to_string(&bare).unwrap(), r#"{"name":"m"}"#);
    }

    #[test]
    fn histogram_rules() {
        let at = Path::root().field("p");
        let mut point = HistogramDataPoint {
            count: 3,
            bucket_counts: vec![1, 2],
            explicit_bounds: vec![5.0],
            ..Default::default()
        };
        assert!(point.check(&at).is_ok());
        point.count = 4;
        assert!(point.check(&at).unwrap_err().reason.contains("count 4"));
        point.count = 3;
        point.explicit_bounds = vec![5.0, 5.0];
        assert!(
            point
                .check(&at)
                .unwrap_err()
                .reason
                .contains("one bucket more")
        );
        point.bucket_counts = vec![1, 1, 1];
        assert_eq!(
            point.check(&at).unwrap_err().reason,
            "explicitBounds is not strictly increasing at index 1"
        );
        point.explicit_bounds = vec![f64::NAN, 5.0];
        assert!(
            point.check(&at).is_err(),
            "NaN never compares as increasing"
        );
        let exponential = ExponentialHistogramDataPoint {
            scale: 21,
            ..Default::default()
        };
        assert_eq!(
            exponential.check(&at).unwrap_err().reason,
            "scale 21 is outside [-10, 20]"
        );
        let bad_exemplar = NumberDataPoint {
            exemplars: vec![Exemplar {
                trace_id: Some(TraceId::default()),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            check_number_points(&[bad_exemplar], &at)
                .unwrap_err()
                .to_string(),
            "p.dataPoints[0].exemplars[0]: traceId is all zeros"
        );
    }
}

//! The resource: the entity producing telemetry, described by its attributes.

use serde::{Deserialize, Serialize};

use crate::common::KeyValue;
use crate::error::Invalid;
use crate::validate::{Path, unique_keys};
use crate::wire;

/// The entity producing telemetry (a service instance, a host, a process), described by
/// attributes whose keys are unique.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    /// The attributes describing the resource; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// How many attributes the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_attributes_count: u32,
}

impl Resource {
    pub(crate) fn check(&self, at: &Path) -> Result<(), Invalid> {
        unique_keys(&self.attributes, at)
    }
}

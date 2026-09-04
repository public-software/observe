//! The path a validation error names, and the checks shared by every signal.

use std::collections::HashSet;

use crate::common::KeyValue;
use crate::error::Invalid;

/// Where a message sits in a payload, in the JSON mapping's own names:
/// `resourceSpans[0].scopeSpans[0].spans[2].events[1]`.
#[derive(Clone, Debug)]
pub(crate) struct Path(String);

impl Path {
    /// The root of a payload (the path of its first field starts here).
    pub(crate) fn root() -> Self {
        Path(String::new())
    }

    /// A field of this message: `.name`.
    pub(crate) fn field(&self, name: &str) -> Self {
        if self.0.is_empty() {
            Path(name.to_owned())
        } else {
            Path(format!("{}.{name}", self.0))
        }
    }

    /// An element of a repeated field of this message: `.name[index]`.
    pub(crate) fn item(&self, name: &str, index: usize) -> Self {
        Path(format!("{}[{index}]", self.field(name).0))
    }

    /// A refusal at this path.
    pub(crate) fn refuse(&self, reason: impl Into<String>) -> Invalid {
        Invalid::new(self.0.clone(), reason)
    }
}

/// Attribute keys must be unique within one attribute list.
pub(crate) fn unique_keys(attributes: &[KeyValue], at: &Path) -> Result<(), Invalid> {
    let mut seen = HashSet::with_capacity(attributes.len());
    for attribute in attributes {
        if !seen.insert(attribute.key.as_str()) {
            return Err(at.refuse(format!("duplicate attribute key {:?}", attribute.key)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_read_like_the_json_mapping() {
        let root = Path::root();
        assert_eq!(
            root.item("resourceSpans", 0)
                .item("scopeSpans", 1)
                .item("spans", 2)
                .0,
            "resourceSpans[0].scopeSpans[1].spans[2]"
        );
        assert_eq!(
            root.item("resourceSpans", 0).field("resource").0,
            "resourceSpans[0].resource"
        );
        assert_eq!(root.field("x").refuse("why").to_string(), "x: why");
    }

    #[test]
    fn duplicate_keys_are_named() {
        let at = Path::root().field("resource");
        let ok = [KeyValue::new("a", 1i64), KeyValue::new("b", 2i64)];
        assert!(unique_keys(&ok, &at).is_ok());
        let dup = [KeyValue::new("a", 1i64), KeyValue::new("a", 2i64)];
        assert_eq!(
            unique_keys(&dup, &at).unwrap_err().to_string(),
            "resource: duplicate attribute key \"a\""
        );
    }
}

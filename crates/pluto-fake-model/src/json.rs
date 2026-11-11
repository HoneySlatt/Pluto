//! The one place that writes compact JSON, which the byte counts, the wire and the journal share.

use serde::Serialize;

/// The compact JSON of plain data (a `serde_json::Value`, the journal's entries).
pub(crate) fn compact(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("plain data with string keys always serialises")
}

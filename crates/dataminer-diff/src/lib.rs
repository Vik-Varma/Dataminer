//! Structural diffs between two spec payloads across game versions.
//!
//! When Mojang ships an update, the question Ferrock needs answered is "what
//! changed?" — which blocks, items, commands or symbols were added, removed or
//! altered. This crate diffs two JSON values and reports exactly that, matching
//! arrays of named objects by their `name` field so that reordering is not
//! mistaken for change.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single change between two values at a given path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "kebab-case")]
pub enum Change {
    /// A key or named entry present only on the right (new) side.
    Added {
        /// JSON-pointer-like path, e.g. `/blocks/minecraft:tuff_stairs`.
        path: String,
        /// The added value.
        value: Value,
    },
    /// A key or named entry present only on the left (old) side.
    Removed {
        /// The path that disappeared.
        path: String,
        /// The removed value.
        value: Value,
    },
    /// A scalar or structurally different value at the same path.
    Modified {
        /// The path that changed.
        path: String,
        /// The old value.
        from: Value,
        /// The new value.
        to: Value,
    },
}

impl Change {
    /// The path this change applies to.
    pub fn path(&self) -> &str {
        match self {
            Change::Added { path, .. }
            | Change::Removed { path, .. }
            | Change::Modified { path, .. } => path,
        }
    }
}

/// The result of diffing two values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    /// Every change, in stable path order.
    pub changes: Vec<Change>,
}

impl Diff {
    /// Whether the two values were identical.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Count of added, removed and modified changes.
    pub fn summary(&self) -> (usize, usize, usize) {
        let mut added = 0;
        let mut removed = 0;
        let mut modified = 0;
        for c in &self.changes {
            match c {
                Change::Added { .. } => added += 1,
                Change::Removed { .. } => removed += 1,
                Change::Modified { .. } => modified += 1,
            }
        }
        (added, removed, modified)
    }
}

/// Diff two JSON values.
pub fn diff(old: &Value, new: &Value) -> Diff {
    let mut changes = Vec::new();
    diff_into("", old, new, &mut changes);
    changes.sort_by(|a, b| a.path().cmp(b.path()));
    Diff { changes }
}

fn diff_into(path: &str, old: &Value, new: &Value, out: &mut Vec<Change>) {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, av) in a {
                let child = format!("{path}/{k}");
                match b.get(k) {
                    Some(bv) => diff_into(&child, av, bv, out),
                    None => out.push(Change::Removed {
                        path: child,
                        value: av.clone(),
                    }),
                }
            }
            for (k, bv) in b {
                if !a.contains_key(k) {
                    out.push(Change::Added {
                        path: format!("{path}/{k}"),
                        value: bv.clone(),
                    });
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if is_named_array(a) && is_named_array(b) => {
            diff_named_arrays(path, a, b, out);
        }
        _ => {
            if old != new {
                out.push(Change::Modified {
                    path: path.to_string(),
                    from: old.clone(),
                    to: new.clone(),
                });
            }
        }
    }
}

/// An array is "named" if every element is an object with a string `name`.
fn is_named_array(arr: &[Value]) -> bool {
    !arr.is_empty()
        && arr
            .iter()
            .all(|v| v.get("name").and_then(Value::as_str).is_some())
}

fn entry_name(v: &Value) -> &str {
    v.get("name").and_then(Value::as_str).unwrap_or_default()
}

fn diff_named_arrays(path: &str, a: &[Value], b: &[Value], out: &mut Vec<Change>) {
    use std::collections::BTreeMap;
    let left: BTreeMap<&str, &Value> = a.iter().map(|v| (entry_name(v), v)).collect();
    let right: BTreeMap<&str, &Value> = b.iter().map(|v| (entry_name(v), v)).collect();

    for (name, av) in &left {
        let child = format!("{path}/{name}");
        match right.get(name) {
            Some(bv) => diff_into(&child, av, bv, out),
            None => out.push(Change::Removed {
                path: child,
                value: (*av).clone(),
            }),
        }
    }
    for (name, bv) in &right {
        if !left.contains_key(name) {
            out.push(Change::Added {
                path: format!("{path}/{name}"),
                value: (*bv).clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_added_removed_modified_by_name() {
        let old = json!({ "blocks": [
            { "name": "a", "raw_id": 1 },
            { "name": "b", "raw_id": 2 }
        ]});
        let new = json!({ "blocks": [
            { "name": "b", "raw_id": 2 },
            { "name": "c", "raw_id": 3 },
            { "name": "a", "raw_id": 9 }
        ]});
        // `a` is modified (1 -> 9), `c` is added, `b` is unchanged, nothing removed.
        let d = diff(&old, &new);
        assert_eq!(d.summary(), (1, 0, 1));
    }

    #[test]
    fn reordering_is_not_a_change() {
        let old = json!({ "x": [ { "name": "a" }, { "name": "b" } ] });
        let new = json!({ "x": [ { "name": "b" }, { "name": "a" } ] });
        assert!(diff(&old, &new).is_empty());
    }

    #[test]
    fn scalar_modification() {
        let old = json!({ "v": 1 });
        let new = json!({ "v": 2 });
        let d = diff(&old, &new);
        assert_eq!(d.changes.len(), 1);
        assert!(matches!(d.changes[0], Change::Modified { .. }));
    }
}

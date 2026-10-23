//! Molang queries and math functions, from `molang_modules/mojang-molang-queries.json`.
//!
//! Molang is the expression language vanilla uses in animations, render controllers
//! and attachables. Ferrock needs the query and math-function surface (names, arity,
//! return types) to evaluate those expressions the way the client does.

use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::MetadataError;

/// The spec `kind` for Molang documents.
pub const KIND: &str = "molang";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct RawMolang {
    #[serde(default)]
    math_functions: Vec<RawFn>,
    #[serde(default)]
    queries: Vec<RawQuery>,
}

#[derive(Debug, Deserialize)]
struct RawFn {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    min_args: Option<i64>,
    #[serde(default)]
    max_args: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RawQuery {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    min_args: Option<i64>,
    #[serde(default)]
    max_args: Option<i64>,
    #[serde(default)]
    return_type: Option<String>,
}

/// A Molang math function, e.g. `math.abs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathFunction {
    /// The function name.
    pub name: String,
    /// Its description.
    pub description: String,
    /// Minimum argument count, when stated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_args: Option<i64>,
    /// Maximum argument count, when stated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_args: Option<i64>,
}

/// A Molang query, e.g. `query.above_top_solid`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    /// The query name.
    pub name: String,
    /// Its description.
    pub description: String,
    /// Minimum argument count, when stated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_args: Option<i64>,
    /// Maximum argument count, when stated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_args: Option<i64>,
    /// The return type, when stated, e.g. `float`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
}

/// The Molang spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MolangPayload {
    /// The math functions, sorted by name.
    pub math_functions: Vec<MathFunction>,
    /// The queries, sorted by name.
    pub queries: Vec<Query>,
}

/// Parse `mojang-molang-queries.json`.
pub fn parse(
    bytes: &[u8],
    game_version: &str,
    repo_ref: &str,
) -> Result<SpecDocument<MolangPayload>, MetadataError> {
    const FILE: &str = "molang_modules/mojang-molang-queries.json";
    let raw: RawMolang = serde_json::from_slice(bytes).map_err(|source| MetadataError::Parse {
        file: FILE.to_string(),
        source,
    })?;
    if raw.queries.is_empty() && raw.math_functions.is_empty() {
        return Err(MetadataError::Sanity {
            file: FILE.to_string(),
            message: "no queries or math functions found; the format may have changed".to_string(),
        });
    }

    let mut math_functions: Vec<MathFunction> = raw
        .math_functions
        .into_iter()
        .map(|f| MathFunction {
            name: f.name,
            description: f.description,
            min_args: f.min_args,
            max_args: f.max_args,
        })
        .collect();
    math_functions.sort_by(|a, b| a.name.cmp(&b.name));

    let mut queries: Vec<Query> = raw
        .queries
        .into_iter()
        .map(|q| Query {
            name: q.name,
            description: q.description,
            min_args: q.min_args,
            max_args: q.max_args,
            return_type: q.return_type,
        })
        .collect();
    queries.sort_by(|a, b| a.name.cmp(&b.name));

    let provenance = crate::provenance_for(game_version, repo_ref, FILE, bytes);
    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        MolangPayload {
            math_functions,
            queries,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "math_functions": [ { "name": "math.abs", "description": "Absolute Value", "min_args": 1, "max_args": 1 } ],
        "queries": [ { "name": "query.above_top_solid", "description": "height", "min_args": 0, "return_type": "float" } ],
        "module_type": "molang", "name": "engine-molang"
    }"#;

    #[test]
    fn parses_sample() {
        let doc = parse(SAMPLE.as_bytes(), "1.26.50.4", "v1.26.50.4").unwrap();
        assert_eq!(doc.payload.math_functions.len(), 1);
        assert_eq!(doc.payload.math_functions[0].name, "math.abs");
        assert_eq!(doc.payload.queries[0].return_type.as_deref(), Some("float"));
        assert_eq!(doc.payload.queries[0].max_args, None);
    }
}

//! Blocks and their state properties, from `vanilladata_modules/mojang-blocks.json`.

use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::MetadataError;

/// The spec `kind` string for block documents.
pub const KIND: &str = "blocks";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

// ---- Raw shapes, exactly as Mojang writes them -----------------------------

#[derive(Debug, Deserialize)]
struct RawBlocks {
    block_properties: Vec<RawProperty>,
    data_items: Vec<RawBlock>,
}

#[derive(Debug, Deserialize)]
struct RawProperty {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    values: Vec<RawPropertyValue>,
}

#[derive(Debug, Deserialize)]
struct RawPropertyValue {
    value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct RawBlock {
    name: String,
    #[serde(default)]
    properties: Vec<RawBlockPropertyRef>,
    raw_id: i64,
    serialization_id: String,
}

#[derive(Debug, Deserialize)]
struct RawBlockPropertyRef {
    name: String,
}

// ---- Normalized payload ----------------------------------------------------

/// A block state property (what vanilla calls a block state), e.g. `facing_direction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockProperty {
    /// The property's identifier, e.g. `button_pressed_bit`.
    pub name: String,
    /// Mojang's type tag: `bool`, `int`, or `enum` (as a string).
    pub value_type: String,
    /// The allowed values, in Mojang's order. `bool`/`int` values are JSON
    /// scalars; `enum` values are strings. Order is significant — it is the
    /// order used when enumerating permutations.
    pub values: Vec<serde_json::Value>,
}

/// A vanilla block type and the properties attached to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// The namespaced id, e.g. `minecraft:acacia_button`.
    pub name: String,
    /// Mojang's internal numeric id (`raw_id`). Not the network runtime id.
    pub raw_id: i64,
    /// The legacy serialization id, e.g. `tile.acacia_button`.
    pub serialization_id: String,
    /// The names of the state properties this block carries, in Mojang's order.
    pub properties: Vec<String>,
}

/// The normalized block spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlocksPayload {
    /// Every block state property, keyed by name within [`BlockProperty::name`].
    pub properties: Vec<BlockProperty>,
    /// Every vanilla block type.
    pub blocks: Vec<Block>,
}

/// Parse `mojang-blocks.json` bytes into a spec document.
///
/// `game_version` and `repo_ref` describe the source for provenance (e.g.
/// `1.26.50.4` and `v1.26.50.4`).
pub fn parse(
    bytes: &[u8],
    game_version: &str,
    repo_ref: &str,
) -> Result<SpecDocument<BlocksPayload>, MetadataError> {
    const FILE: &str = "vanilladata_modules/mojang-blocks.json";
    let raw: RawBlocks = serde_json::from_slice(bytes).map_err(|source| MetadataError::Parse {
        file: FILE.to_string(),
        source,
    })?;

    if raw.data_items.is_empty() {
        return Err(MetadataError::Sanity {
            file: FILE.to_string(),
            message: "no blocks found; the file format may have changed".to_string(),
        });
    }

    let properties = raw
        .block_properties
        .into_iter()
        .map(|p| BlockProperty {
            name: p.name,
            value_type: p.ty,
            values: p.values.into_iter().map(|v| v.value).collect(),
        })
        .collect();

    let mut properties: Vec<BlockProperty> = properties;
    properties.sort_by(|a, b| a.name.cmp(&b.name));

    let mut blocks: Vec<Block> = raw
        .data_items
        .into_iter()
        .map(|b| Block {
            name: b.name,
            raw_id: b.raw_id,
            serialization_id: b.serialization_id,
            properties: b.properties.into_iter().map(|r| r.name).collect(),
        })
        .collect();
    // Sort by name for stable, reviewable diffs across versions. Each record
    // keeps its raw_id, so ordering carries no information that is lost here.
    blocks.sort_by(|a, b| a.name.cmp(&b.name));

    let payload = BlocksPayload { properties, blocks };
    let provenance = crate::provenance_for(game_version, repo_ref, FILE, bytes);
    Ok(SpecDocument::new(KIND, SCHEMA_VERSION, provenance, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "block_properties": [
            { "name": "active", "type": "bool", "values": [ {"value": false}, {"value": true} ] },
            { "name": "facing_direction", "type": "int", "values": [ {"value": 0}, {"value": 1} ] }
        ],
        "data_items": [
            { "name": "minecraft:acacia_button",
              "properties": [ {"name": "button_pressed_bit"}, {"name": "facing_direction"} ],
              "raw_id": 395, "serialization_id": "tile.acacia_button" }
        ],
        "module_type": "vanilla_data",
        "name": "mojang-block",
        "vanilla_data_type": "block"
    }"#;

    #[test]
    fn parses_sample() {
        let doc = parse(SAMPLE.as_bytes(), "1.26.50.4", "v1.26.50.4").unwrap();
        assert_eq!(doc.meta.kind, KIND);
        assert_eq!(doc.payload.properties.len(), 2);
        assert_eq!(doc.payload.properties[0].name, "active");
        assert_eq!(
            doc.payload.properties[0].values,
            vec![serde_json::json!(false), serde_json::json!(true)]
        );
        assert_eq!(doc.payload.blocks.len(), 1);
        let b = &doc.payload.blocks[0];
        assert_eq!(b.name, "minecraft:acacia_button");
        assert_eq!(b.raw_id, 395);
        assert_eq!(b.serialization_id, "tile.acacia_button");
        assert_eq!(b.properties, vec!["button_pressed_bit", "facing_direction"]);
    }

    #[test]
    fn empty_blocks_is_a_sanity_error() {
        let bad = r#"{ "block_properties": [], "data_items": [] }"#;
        let err = parse(bad.as_bytes(), "1", "v1").unwrap_err();
        assert!(matches!(err, MetadataError::Sanity { .. }));
    }

    #[test]
    fn provenance_pins_the_bytes() {
        let doc = parse(SAMPLE.as_bytes(), "1.26.50.4", "v1.26.50.4").unwrap();
        assert!(doc.meta.provenance.artifact_sha256.is_some());
        assert_eq!(doc.meta.provenance.game_version, "1.26.50.4");
    }
}

//! The flat vanilladata registries: items, entities, biomes, effects, enchantments.
//!
//! These live under `metadata/vanilladata_modules/`. Items carry several ids;
//! the rest are mostly name lists, sometimes with a numeric id. We keep each
//! entry's full set of known fields and preserve Mojang's ordering.

use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::MetadataError;

/// The document schema version shared by the registry specs.
pub const SCHEMA_VERSION: u32 = 1;

// ---- Items -----------------------------------------------------------------

/// The spec `kind` for item documents.
pub const ITEMS_KIND: &str = "items";

#[derive(Debug, Deserialize)]
struct RawItems {
    data_items: Vec<RawItem>,
}

#[derive(Debug, Deserialize)]
struct RawItem {
    #[serde(default)]
    command_name: Option<String>,
    name: String,
    #[serde(default)]
    raw_id: Option<i64>,
    #[serde(default)]
    serialization_id: Option<String>,
    #[serde(default)]
    serialization_name: Option<String>,
}

/// A vanilla item type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// The namespaced id, e.g. `minecraft:acacia_boat`.
    pub name: String,
    /// The name accepted on the command line, when it differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_name: Option<String>,
    /// Mojang's internal numeric id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_id: Option<i64>,
    /// The legacy serialization id, e.g. `item.acacia_boat`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serialization_id: Option<String>,
    /// The modern serialization name, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serialization_name: Option<String>,
}

/// The normalized item spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemsPayload {
    /// Every vanilla item type, in Mojang's order.
    pub items: Vec<Item>,
}

/// Parse `mojang-items.json`.
pub fn parse_items(
    bytes: &[u8],
    game_version: &str,
    repo_ref: &str,
) -> Result<SpecDocument<ItemsPayload>, MetadataError> {
    const FILE: &str = "vanilladata_modules/mojang-items.json";
    let raw: RawItems = serde_json::from_slice(bytes).map_err(|source| MetadataError::Parse {
        file: FILE.to_string(),
        source,
    })?;
    if raw.data_items.is_empty() {
        return Err(MetadataError::Sanity {
            file: FILE.to_string(),
            message: "no items found; the file format may have changed".to_string(),
        });
    }
    let items = raw
        .data_items
        .into_iter()
        .map(|i| Item {
            name: i.name,
            command_name: i.command_name,
            raw_id: i.raw_id,
            serialization_id: i.serialization_id,
            serialization_name: i.serialization_name,
        })
        .collect();
    let provenance = crate::provenance_for(game_version, repo_ref, FILE, bytes);
    Ok(SpecDocument::new(
        ITEMS_KIND,
        SCHEMA_VERSION,
        provenance,
        ItemsPayload { items },
    ))
}

// ---- Simple name/id registries ---------------------------------------------

#[derive(Debug, Deserialize)]
struct RawSimpleRegistry {
    data_items: Vec<RawSimpleEntry>,
}

#[derive(Debug, Deserialize)]
struct RawSimpleEntry {
    name: String,
    #[serde(default)]
    id: Option<i64>,
}

/// One entry of a simple registry: a namespaced name and an optional numeric id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// The namespaced id, e.g. `minecraft:bamboo_jungle`.
    pub name: String,
    /// The numeric id, when Mojang assigns one in this file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
}

/// The payload for a simple registry spec.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPayload {
    /// The entries, in Mojang's order.
    pub entries: Vec<RegistryEntry>,
}

/// One of the flat registries Dataminer knows how to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimpleRegistry {
    /// `mojang-entities.json`.
    Entities,
    /// `mojang-biomes.json`.
    Biomes,
    /// `mojang-effects.json`.
    Effects,
    /// `mojang-enchantments.json`.
    Enchantments,
}

impl SimpleRegistry {
    /// The spec `kind` string.
    pub fn kind(self) -> &'static str {
        match self {
            SimpleRegistry::Entities => "entities",
            SimpleRegistry::Biomes => "biomes",
            SimpleRegistry::Effects => "effects",
            SimpleRegistry::Enchantments => "enchantments",
        }
    }

    /// The source file path under `metadata/`.
    pub fn rel_path(self) -> &'static str {
        match self {
            SimpleRegistry::Entities => "vanilladata_modules/mojang-entities.json",
            SimpleRegistry::Biomes => "vanilladata_modules/mojang-biomes.json",
            SimpleRegistry::Effects => "vanilladata_modules/mojang-effects.json",
            SimpleRegistry::Enchantments => "vanilladata_modules/mojang-enchantments.json",
        }
    }
}

/// Parse one of the simple registries.
pub fn parse_simple(
    registry: SimpleRegistry,
    bytes: &[u8],
    game_version: &str,
    repo_ref: &str,
) -> Result<SpecDocument<RegistryPayload>, MetadataError> {
    let file = registry.rel_path();
    let raw: RawSimpleRegistry =
        serde_json::from_slice(bytes).map_err(|source| MetadataError::Parse {
            file: file.to_string(),
            source,
        })?;
    if raw.data_items.is_empty() {
        return Err(MetadataError::Sanity {
            file: file.to_string(),
            message: "no entries found; the file format may have changed".to_string(),
        });
    }
    let entries = raw
        .data_items
        .into_iter()
        .map(|e| RegistryEntry {
            name: e.name,
            id: e.id,
        })
        .collect();
    let provenance = crate::provenance_for(game_version, repo_ref, file, bytes);
    Ok(SpecDocument::new(
        registry.kind(),
        SCHEMA_VERSION,
        provenance,
        RegistryPayload { entries },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_items() {
        let s = r#"{"data_items":[
            {"command_name":"minecraft:acacia_boat","name":"minecraft:acacia_boat",
             "raw_id":373,"serialization_id":"item.acacia_boat",
             "serialization_name":"minecraft:acacia_boat"}]}"#;
        let doc = parse_items(s.as_bytes(), "1.26.50.4", "v1.26.50.4").unwrap();
        assert_eq!(doc.payload.items.len(), 1);
        assert_eq!(doc.payload.items[0].raw_id, Some(373));
    }

    #[test]
    fn parses_biomes_with_ids() {
        let s = r#"{"data_items":[{"id":7,"name":"minecraft:bamboo_jungle"}]}"#;
        let doc = parse_simple(SimpleRegistry::Biomes, s.as_bytes(), "1", "v1").unwrap();
        assert_eq!(doc.meta.kind, "biomes");
        assert_eq!(doc.payload.entries[0].id, Some(7));
    }

    #[test]
    fn parses_name_only_registry() {
        let s = r#"{"data_items":[{"name":"minecraft:agent"}]}"#;
        let doc = parse_simple(SimpleRegistry::Entities, s.as_bytes(), "1", "v1").unwrap();
        assert_eq!(doc.payload.entries[0].name, "minecraft:agent");
        assert_eq!(doc.payload.entries[0].id, None);
    }
}

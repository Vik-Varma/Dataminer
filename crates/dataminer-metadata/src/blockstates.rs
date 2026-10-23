//! Enumerate every block state and compute its network runtime hash.
//!
//! Bedrock defines a block's state space as the full cartesian product of its
//! properties' value domains. We take the block list and the property domains
//! from `mojang-blocks.json` (already parsed by [`crate::blocks`]), enumerate
//! each block's states, and compute the network block hash for each via
//! [`dataminer_core::block_hash`]. Ferrock uses this to map chunk-palette
//! entries to runtime ids when `UseBlockNetworkIDHashes` is set.
//!
//! Honesty: the per-state hash is a deterministic rule (algorithm replicated
//! verbatim from Dragonfly, FNV-validated). The *state set* is the cartesian
//! product per Bedrock's model; confirming it against a live BDS capture is a
//! still-pending cross-check, noted in the provenance. Any two distinct states
//! that collide on a hash are reported rather than silently merged.

use std::collections::BTreeMap;

use dataminer_core::block_hash::{StateValue, network_block_hash};
use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::blocks::BlocksPayload;

/// The spec `kind` for block-state documents.
pub const KIND: &str = "blockstates";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// One concrete block state and its network hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateEntry {
    /// The network block hash (FNV-1a over the state's little-endian NBT).
    pub hash: u32,
    /// The property values for this state, keys sorted.
    pub properties: BTreeMap<String, StateValue>,
}

/// A block and all of its states.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockStates {
    /// The namespaced block name.
    pub name: String,
    /// Every state, sorted by hash.
    pub states: Vec<StateEntry>,
}

/// Two or more distinct states that produced the same hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collision {
    /// The shared hash.
    pub hash: u32,
    /// Human-readable `name{props}` descriptions of the colliding states.
    pub states: Vec<String>,
}

/// The block-state spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockStatesPayload {
    /// Number of block types.
    pub block_count: usize,
    /// Total number of states across all blocks.
    pub state_count: usize,
    /// Blocks and their states, sorted by block name.
    pub blocks: Vec<BlockStates>,
    /// Blocks that reference a property missing from the property table, which
    /// we could not enumerate. Listed for honest coverage, not silently dropped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
    /// Distinct states that share a hash (expected empty; reported if not).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collisions: Vec<Collision>,
}

fn to_state_value(v: &Value) -> Option<StateValue> {
    match v {
        Value::Bool(b) => Some(StateValue::Bool(*b)),
        Value::Number(n) => n.as_i64().map(|i| StateValue::Int(i as i32)),
        Value::String(s) => Some(StateValue::Str(s.clone())),
        _ => None,
    }
}

fn describe(name: &str, props: &BTreeMap<String, StateValue>) -> String {
    let inner: Vec<String> = props
        .iter()
        .map(|(k, v)| match v {
            StateValue::Bool(b) => format!("{k}={b}"),
            StateValue::Int(i) => format!("{k}={i}"),
            StateValue::Str(s) => format!("{k}={s}"),
        })
        .collect();
    format!("{name}{{{}}}", inner.join(","))
}

/// Enumerate block states and hashes from a parsed blocks spec document,
/// carrying its provenance forward (with added notes).
pub fn enumerate(blocks_doc: &SpecDocument<BlocksPayload>) -> SpecDocument<BlockStatesPayload> {
    let payload = &blocks_doc.payload;

    // property name -> its value domain, as StateValues.
    let mut domains: BTreeMap<&str, Vec<StateValue>> = BTreeMap::new();
    for p in &payload.properties {
        let values: Vec<StateValue> = p.values.iter().filter_map(to_state_value).collect();
        domains.insert(p.name.as_str(), values);
    }

    let mut blocks = Vec::new();
    let mut unresolved = Vec::new();
    let mut state_count = 0usize;
    // hash -> first state description, to detect collisions.
    let mut seen: BTreeMap<u32, String> = BTreeMap::new();
    let mut collisions: BTreeMap<u32, Vec<String>> = BTreeMap::new();

    for block in &payload.blocks {
        // Gather this block's property domains; bail to unresolved if any missing.
        let mut props: Vec<(&str, &Vec<StateValue>)> = Vec::new();
        let mut missing = false;
        for pname in &block.properties {
            match domains.get(pname.as_str()) {
                Some(vals) if !vals.is_empty() => props.push((pname.as_str(), vals)),
                _ => {
                    missing = true;
                    break;
                }
            }
        }
        if missing {
            unresolved.push(block.name.clone());
            continue;
        }

        let mut states = Vec::new();
        for combo in cartesian(&props) {
            let map: BTreeMap<String, StateValue> = combo
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect();
            let pairs: Vec<(String, StateValue)> =
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let hash = network_block_hash(&block.name, &pairs);

            let desc = describe(&block.name, &map);
            match seen.get(&hash) {
                None => {
                    seen.insert(hash, desc);
                }
                Some(first) if first != &desc => {
                    let entry = collisions
                        .entry(hash)
                        .or_insert_with(|| vec![first.clone()]);
                    entry.push(desc);
                }
                Some(_) => {}
            }

            states.push(StateEntry {
                hash,
                properties: map,
            });
            state_count += 1;
        }
        states.sort_by_key(|s| s.hash);
        blocks.push(BlockStates {
            name: block.name.clone(),
            states,
        });
    }

    blocks.sort_by(|a, b| a.name.cmp(&b.name));
    let collisions: Vec<Collision> = collisions
        .into_iter()
        .map(|(hash, states)| Collision { hash, states })
        .collect();

    let mut provenance = blocks_doc.meta.provenance.clone();
    provenance.notes = vec![
        "Network block hash = FNV-1a (32-bit) over the state's little-endian NBT; algorithm \
         replicated verbatim from Dragonfly's network_block_hash.go (MIT) and FNV-validated."
            .to_string(),
        "State set is the full cartesian product of each block's property domains (Bedrock's \
         model), from mojang-blocks.json."
            .to_string(),
        "Cross-checked against Dragonfly's block_states.nbt (MIT): 22,055 of its 22,091 \
         authoritative hashes are reproduced exactly. ~134 extra states here are cartesian \
         over-generations for a few blocks (cocoa, rails, nether_wart, frosted_ice, chorus_flower, \
         shelf_mushroom); ~36 Dragonfly states are deprecated/education/internal blocks outside \
         Mojang's public vanilladata. A live-BDS capture of the target version remains the final \
         oracle for the exact valid set. See docs/validation/blockstates-crosscheck-2026-10-23.md."
            .to_string(),
    ];

    let block_count = blocks.len();
    SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        BlockStatesPayload {
            block_count,
            state_count,
            blocks,
            unresolved,
            collisions,
        },
    )
}

/// The cartesian product of the property domains. Each yielded item is a vector
/// of `(property name, value)` in the input order.
fn cartesian<'a>(props: &[(&'a str, &'a Vec<StateValue>)]) -> Vec<Vec<(&'a str, StateValue)>> {
    let mut out: Vec<Vec<(&str, StateValue)>> = vec![Vec::new()];
    for (name, values) in props {
        let mut next = Vec::with_capacity(out.len() * values.len());
        for prefix in &out {
            for v in values.iter() {
                let mut row = prefix.clone();
                row.push((*name, v.clone()));
                next.push(row);
            }
        }
        out = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks;

    const SAMPLE: &str = r#"{
        "block_properties": [
            { "name": "button_pressed_bit", "type": "bool", "values": [ {"value": false}, {"value": true} ] },
            { "name": "facing_direction", "type": "int", "values": [ {"value": 0}, {"value": 1} ] }
        ],
        "data_items": [
            { "name": "minecraft:acacia_button",
              "properties": [ {"name": "button_pressed_bit"}, {"name": "facing_direction"} ],
              "raw_id": 395, "serialization_id": "tile.acacia_button" },
            { "name": "minecraft:stone", "properties": [], "raw_id": 1, "serialization_id": "tile.stone" }
        ]
    }"#;

    #[test]
    fn enumerates_and_hashes() {
        let blocks_doc = blocks::parse(SAMPLE.as_bytes(), "1.26.50.4", "v1.26.50.4").unwrap();
        let doc = enumerate(&blocks_doc);
        assert_eq!(doc.payload.block_count, 2);
        // acacia_button: 2x2 = 4 states; stone: 1 empty state => 5 total.
        assert_eq!(doc.payload.state_count, 5);
        assert!(doc.payload.collisions.is_empty());
        assert!(doc.payload.unresolved.is_empty());

        // stone (empty states) matches the core golden vector.
        let stone = doc
            .payload
            .blocks
            .iter()
            .find(|b| b.name == "minecraft:stone")
            .unwrap();
        assert_eq!(stone.states.len(), 1);
        assert_eq!(stone.states[0].hash, 2_150_698_529);

        // acacia_button{button_pressed_bit=false,facing_direction=0} golden vector.
        let btn = doc
            .payload
            .blocks
            .iter()
            .find(|b| b.name == "minecraft:acacia_button")
            .unwrap();
        assert_eq!(btn.states.len(), 4);
        assert!(btn.states.iter().any(|s| s.hash == 152_215_376));
    }

    #[test]
    fn missing_property_is_unresolved() {
        let s = r#"{ "block_properties": [],
            "data_items": [ { "name": "minecraft:x", "properties": [ {"name": "ghost"} ],
                              "raw_id": 1, "serialization_id": "tile.x" } ] }"#;
        let blocks_doc = blocks::parse(s.as_bytes(), "1", "v1").unwrap();
        let doc = enumerate(&blocks_doc);
        assert_eq!(doc.payload.unresolved, vec!["minecraft:x".to_string()]);
    }
}

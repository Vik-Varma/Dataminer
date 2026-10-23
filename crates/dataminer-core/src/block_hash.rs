//! The Bedrock "network block hash": the runtime id a client computes for a
//! block state when `StartGame.UseBlockNetworkIDHashes` is set.
//!
//! The id is FNV-1a (32-bit) over a little-endian NBT compound describing the
//! state: an unnamed root compound holding a `name` string and a `states`
//! compound whose keys are sorted, with `minecraft:unknown` special-cased to
//! `0xFFFFFFFE`. Ferrock needs this to map chunk-palette entries to runtime ids.
//!
//! The byte layout is replicated verbatim from Dragonfly's
//! `server/world/network_block_hash.go` (MIT) — a server library that
//! interoperates with the real client, so the algorithm is validated by its
//! real-world use. The FNV-1a implementation here is checked against the
//! standard published FNV test vectors, and the whole function against golden
//! vectors, in the tests below. Confirming the result against a live BDS capture
//! is a separate, still-pending step.

use serde::{Deserialize, Serialize};

/// The sentinel id for `minecraft:unknown`.
pub const UNKNOWN_HASH: u32 = 0xffff_fffe;

const FNV1A32_OFFSET: u32 = 0x811c_9dc5;
const FNV1A32_PRIME: u32 = 0x0100_0193;

/// A block-state property value. Bedrock block states use exactly these three
/// value types: a byte-encoded bool, a 32-bit int, and a string (enum).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StateValue {
    /// A boolean, encoded as an NBT Byte (0/1).
    Bool(bool),
    /// A 32-bit integer, encoded as an NBT Int (little-endian).
    Int(i32),
    /// A string, encoded as an NBT String.
    Str(String),
}

/// FNV-1a 32-bit over a byte slice.
pub fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut h = FNV1A32_OFFSET;
    for &b in bytes {
        h ^= u32::from(b);
        h = h.wrapping_mul(FNV1A32_PRIME);
    }
    h
}

/// Compute the network block hash for a block `name` and its `states`.
///
/// `states` is given as `(key, value)` pairs; order does not matter, the keys
/// are sorted here to match the canonical encoding.
pub fn network_block_hash(name: &str, states: &[(String, StateValue)]) -> u32 {
    if name == "minecraft:unknown" {
        return UNKNOWN_HASH;
    }
    fnv1a32(&encode(name, states))
}

/// The exact little-endian NBT bytes that [`network_block_hash`] hashes. Exposed
/// so tests (and auditors) can inspect the encoding directly.
pub fn encode(name: &str, states: &[(String, StateValue)]) -> Vec<u8> {
    let mut keyed: Vec<&(String, StateValue)> = states.iter().collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));

    let mut data = Vec::new();
    let write_string = |data: &mut Vec<u8>, s: &str| {
        data.extend_from_slice(&(s.len() as u16).to_le_bytes());
        data.extend_from_slice(s.as_bytes());
    };

    // Unnamed root compound.
    data.push(10); // TAG_Compound
    data.push(0); // name length (u16 LE) = 0
    data.push(0);

    // name: <block name>
    data.push(8); // TAG_String
    write_string(&mut data, "name");
    write_string(&mut data, name);

    // states: { <sorted key>: <value> }
    data.push(10); // TAG_Compound
    write_string(&mut data, "states");
    for (k, v) in keyed {
        match v {
            StateValue::Str(s) => {
                data.push(8); // TAG_String
                write_string(&mut data, k);
                write_string(&mut data, s);
            }
            StateValue::Bool(b) => {
                data.push(1); // TAG_Byte
                write_string(&mut data, k);
                data.push(u8::from(*b));
            }
            StateValue::Int(i) => {
                data.push(3); // TAG_Int
                write_string(&mut data, k);
                data.extend_from_slice(&i.to_le_bytes());
            }
        }
    }
    data.push(0); // end states compound
    data.push(0); // end root compound
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a32_matches_standard_vectors() {
        // Published FNV-1a 32-bit test vectors.
        assert_eq!(fnv1a32(b""), 0x811c_9dc5);
        assert_eq!(fnv1a32(b"a"), 0xe40c_292c);
        assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
    }

    #[test]
    fn unknown_is_sentinel() {
        assert_eq!(network_block_hash("minecraft:unknown", &[]), UNKNOWN_HASH);
    }

    #[test]
    fn golden_vectors() {
        // Golden values computed from an independent replica of Dragonfly's
        // network_block_hash.go. They lock the byte layout + FNV together.
        assert_eq!(network_block_hash("minecraft:air", &[]), 3_690_217_760);
        assert_eq!(network_block_hash("minecraft:stone", &[]), 2_150_698_529);
        assert_eq!(network_block_hash("minecraft:dirt", &[]), 2_186_211_206);
        assert_eq!(
            network_block_hash(
                "minecraft:acacia_button",
                &[
                    ("button_pressed_bit".to_string(), StateValue::Bool(false)),
                    ("facing_direction".to_string(), StateValue::Int(0)),
                ]
            ),
            152_215_376
        );
    }

    #[test]
    fn key_order_does_not_change_hash() {
        let a = network_block_hash(
            "minecraft:x",
            &[
                ("b".to_string(), StateValue::Int(1)),
                ("a".to_string(), StateValue::Bool(true)),
            ],
        );
        let b = network_block_hash(
            "minecraft:x",
            &[
                ("a".to_string(), StateValue::Bool(true)),
                ("b".to_string(), StateValue::Int(1)),
            ],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn root_compound_prefix_is_exact() {
        // First bytes: TAG_Compound, u16 name len 0, TAG_String, "name".
        let bytes = encode("minecraft:air", &[]);
        assert_eq!(&bytes[0..3], &[10, 0, 0]);
        assert_eq!(bytes[3], 8); // TAG_String for "name"
        assert_eq!(&bytes[4..6], &4u16.to_le_bytes());
        assert_eq!(&bytes[6..10], b"name");
        assert_eq!(*bytes.last().unwrap(), 0); // final end tag
    }
}

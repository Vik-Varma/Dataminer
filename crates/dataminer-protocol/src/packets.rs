//! Resolve packets to their ordered wire fields, plus the protocol enums.

use std::collections::BTreeMap;

use dataminer_core::digest::Sha256Digest;
use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ProtocolError;

/// The spec `kind` for packet documents.
pub const KIND: &str = "packets";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// One field of a packet, as it appears on the wire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// The field name, e.g. `Target Actor Runtime ID`.
    pub name: String,
    /// The wire order (`x-ordinal-index`). Fields are emitted in this order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<i64>,
    /// The wire primitive (`x-underlying-type`), e.g. `uint8`, `float`, when given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying_type: Option<String>,
    /// The referenced schema type (basename of the `$ref`), e.g. `ActorRuntimeID`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_ref: Option<String>,
    /// Serialization options, e.g. `Compression` (varint), `Enum-as-Value`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serialization_options: Vec<String>,
    /// Whether the field is optional (not in the payload's `required` list).
    pub optional: bool,
}

/// A packet and its resolved fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Packet {
    /// The packet name, e.g. `AnimatePacket`.
    pub name: String,
    /// The packet id (`$metaProperties["[cereal:packet]"]`).
    pub id: i64,
    /// The payload schema name this packet resolves through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    /// The fields in wire order. Empty if the payload has none or could not be
    /// resolved (see [`PacketsPayload::unresolved`]).
    pub fields: Vec<Field>,
}

/// A protocol enum (a named set of symbolic values).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtoEnum {
    /// The enum name, e.g. `ActorEvent`.
    pub name: String,
    /// The JSON base type (`string`, `integer`, ...).
    pub value_type: String,
    /// The values, in the file's order.
    pub values: Vec<String>,
}

/// The normalized packet spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketsPayload {
    /// The wire protocol version (`x-protocol-version`).
    pub protocol_version: u32,
    /// Every packet, sorted by name, fields in wire order.
    pub packets: Vec<Packet>,
    /// Every protocol enum, sorted by name.
    pub enums: Vec<ProtoEnum>,
    /// Packets whose payload fields could not be resolved to the simple
    /// `properties` form (e.g. `allOf`/`oneOf` payloads). Listed for honest
    /// coverage rather than silently emitted as empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
}

/// Strip a `$ref` like `./AnimatePacketPayload.json` down to `AnimatePacketPayload`.
fn ref_basename(r: &str) -> String {
    r.trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches(".json")
        .to_string()
}

/// Parse a directory of protocol-docs `json/` files (keyed by basename, e.g.
/// `AnimatePacket.json` → bytes) into a packet spec document.
pub fn parse(
    files: &BTreeMap<String, Vec<u8>>,
    game_version: &str,
    repo_ref: &str,
) -> Result<SpecDocument<PacketsPayload>, ProtocolError> {
    // Parse every file once, keyed by basename without extension.
    let mut docs: BTreeMap<String, Value> = BTreeMap::new();
    for (name, bytes) in files {
        let v: Value = serde_json::from_slice(bytes).map_err(|source| ProtocolError::Parse {
            file: name.clone(),
            source,
        })?;
        docs.insert(ref_basename(name), v);
    }

    let mut packets = Vec::new();
    let mut unresolved = Vec::new();
    let mut protocol_version: Option<u32> = None;

    for (base, v) in &docs {
        let Some(id) = v
            .get("$metaProperties")
            .and_then(|m| m.get("[cereal:packet]"))
            .and_then(Value::as_i64)
        else {
            continue;
        };
        if let Some(p) = v.get("x-protocol-version").and_then(Value::as_u64) {
            protocol_version.get_or_insert(p as u32);
        }

        let name = v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(base)
            .to_string();

        // The packet $refs a payload; resolve it and read its properties.
        let payload_ref = v.get("$ref").and_then(Value::as_str).map(ref_basename);
        let mut fields = Vec::new();
        let mut resolved = true;
        if let Some(pref) = &payload_ref {
            match docs.get(pref) {
                Some(payload) => {
                    resolved = collect_fields(payload, &mut fields);
                }
                None => resolved = false,
            }
        }
        if !resolved {
            unresolved.push(name.clone());
        }
        fields.sort_by(|a, b| a.ordinal.cmp(&b.ordinal).then_with(|| a.name.cmp(&b.name)));

        packets.push(Packet {
            name,
            id,
            payload: payload_ref,
            fields,
        });
    }

    if packets.is_empty() {
        return Err(ProtocolError::Sanity(
            "no packets found (no files with $metaProperties[\"[cereal:packet]\"]); \
             the protocol-docs format may have changed"
                .to_string(),
        ));
    }
    packets.sort_by(|a, b| a.name.cmp(&b.name));

    // Enums: files with an `enum` array.
    let mut enums = Vec::new();
    for (base, v) in &docs {
        let Some(values) = v.get("enum").and_then(Value::as_array) else {
            continue;
        };
        let value_type = v
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("string")
            .to_string();
        let values = values
            .iter()
            .filter_map(|x| x.as_str().map(ToString::to_string))
            .collect();
        enums.push(ProtoEnum {
            name: v
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(base)
                .to_string(),
            value_type,
            values,
        });
    }
    enums.sort_by(|a, b| a.name.cmp(&b.name));

    let protocol_version = protocol_version.ok_or_else(|| {
        ProtocolError::Sanity("no x-protocol-version found on any packet".to_string())
    })?;

    // Pin the exact input: a digest over the sorted (name, bytes) pairs.
    let aggregate = aggregate_digest(files);
    let provenance = crate::provenance(game_version, repo_ref, aggregate);

    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        PacketsPayload {
            protocol_version,
            packets,
            enums,
            unresolved,
        },
    ))
}

/// Read a payload object's `properties` into `fields`. Returns whether the
/// payload was in the simple object-with-properties form we understand.
fn collect_fields(payload: &Value, fields: &mut Vec<Field>) -> bool {
    let Some(props) = payload.get("properties").and_then(Value::as_object) else {
        // An empty payload (object with no properties) is a resolved, zero-field
        // packet; anything else (allOf/oneOf/array) is not understood here.
        return payload.get("type").and_then(Value::as_str) == Some("object");
    };
    let required: Vec<&str> = payload
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    for (fname, fspec) in props {
        let serialization_options = fspec
            .get("x-serialization-options")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        fields.push(Field {
            name: fname.clone(),
            ordinal: fspec.get("x-ordinal-index").and_then(Value::as_i64),
            underlying_type: fspec
                .get("x-underlying-type")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            type_ref: fspec.get("$ref").and_then(Value::as_str).map(ref_basename),
            serialization_options,
            optional: !required.contains(&fname.as_str()),
        });
    }
    true
}

/// A SHA-256 over the sorted (filename, bytes) pairs, pinning the whole input.
fn aggregate_digest(files: &BTreeMap<String, Vec<u8>>) -> Sha256Digest {
    let mut buf = Vec::new();
    for (name, bytes) in files {
        buf.extend_from_slice(name.as_bytes());
        buf.push(0);
        buf.extend_from_slice(bytes);
        buf.push(0);
    }
    Sha256Digest::of_bytes(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> BTreeMap<String, Vec<u8>> {
        let mut m = BTreeMap::new();
        m.insert(
            "AnimatePacket.json".to_string(),
            br#"{"title":"AnimatePacket","x-protocol-version":2193,"$ref":"./AnimatePacketPayload.json",
                 "$metaProperties":{"[cereal:packet]":44}}"#
                .to_vec(),
        );
        m.insert(
            "AnimatePacketPayload.json".to_string(),
            br#"{"title":"AnimatePacketPayload","type":"object","properties":{
                 "Action":{"$ref":"./Act.json","x-underlying-type":"uint8",
                           "x-serialization-options":["Enum-as-Value"],"x-ordinal-index":0},
                 "Data":{"type":"number","x-underlying-type":"float","x-ordinal-index":2},
                 "Target Actor Runtime ID":{"$ref":"./ActorRuntimeID.json","x-ordinal-index":1}},
                 "required":["Action","Data","Target Actor Runtime ID"]}"#
                .to_vec(),
        );
        m.insert(
            "ActorEvent.json".to_string(),
            br#"{"title":"ActorEvent","type":"string","enum":["NONE","JUMP","HURT"]}"#.to_vec(),
        );
        m
    }

    #[test]
    fn resolves_packet_fields_in_wire_order() {
        let doc = parse(&files(), "1.26.51", "v1.26.51").unwrap();
        assert_eq!(doc.payload.protocol_version, 2193);
        assert_eq!(doc.payload.packets.len(), 1);
        let p = &doc.payload.packets[0];
        assert_eq!(p.name, "AnimatePacket");
        assert_eq!(p.id, 44);
        // Ordinal order: Action(0), Target Actor Runtime ID(1), Data(2).
        let names: Vec<&str> = p.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["Action", "Target Actor Runtime ID", "Data"]);
        assert_eq!(p.fields[0].underlying_type.as_deref(), Some("uint8"));
        assert_eq!(p.fields[0].serialization_options, vec!["Enum-as-Value"]);
        assert_eq!(p.fields[1].type_ref.as_deref(), Some("ActorRuntimeID"));
        assert!(doc.payload.unresolved.is_empty());
    }

    #[test]
    fn collects_enums() {
        let doc = parse(&files(), "1.26.51", "v1.26.51").unwrap();
        assert_eq!(doc.payload.enums.len(), 1);
        assert_eq!(doc.payload.enums[0].name, "ActorEvent");
        assert_eq!(doc.payload.enums[0].values, vec!["NONE", "JUMP", "HURT"]);
    }

    #[test]
    fn unresolved_payload_is_recorded_not_hidden() {
        let mut m = files();
        // A packet whose payload uses allOf (not the simple properties form).
        m.insert(
            "WeirdPacket.json".to_string(),
            br#"{"title":"WeirdPacket","$ref":"./WeirdPayload.json","$metaProperties":{"[cereal:packet]":200}}"#.to_vec(),
        );
        m.insert(
            "WeirdPayload.json".to_string(),
            br#"{"title":"WeirdPayload","allOf":[{"$ref":"./A.json"}]}"#.to_vec(),
        );
        let doc = parse(&m, "1.26.51", "v1.26.51").unwrap();
        assert!(doc.payload.unresolved.contains(&"WeirdPacket".to_string()));
    }
}

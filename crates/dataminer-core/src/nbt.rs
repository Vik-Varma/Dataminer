//! A minimal reader for Bedrock's *network little-endian* NBT.
//!
//! This is the encoding Bedrock uses on the wire and in gophertunnel/Dragonfly's
//! embedded data (e.g. `block_states.nbt`): string lengths and `Int`/`Long` are
//! unsigned/zig-zag varints, `Short` is 2-byte little-endian, `Byte` is one byte.
//! Dataminer reads it to cross-check its own facts against those MIT sources, and
//! (later) to read chunk-palette entries from captures.
//!
//! This reader covers the tags those inputs actually use — End, Byte, Short, Int,
//! String, Compound — and fails loudly on any other tag rather than guessing. It
//! is not a general NBT library; it grows a tag when a real input needs it.

use std::collections::BTreeMap;

/// Errors from decoding network-LE NBT.
#[derive(Debug, thiserror::Error)]
pub enum NbtError {
    /// Hit the end of input while more bytes were expected.
    #[error("unexpected end of NBT input at offset {0}")]
    Eof(usize),
    /// A tag we do not (yet) decode. The message names it so a reader knows what
    /// to add if a future input needs it.
    #[error("unsupported network-LE NBT tag {tag} at offset {offset}")]
    UnsupportedTag {
        /// The tag byte.
        tag: u8,
        /// Where it was.
        offset: usize,
    },
    /// A string was not valid UTF-8.
    #[error("invalid UTF-8 in NBT string at offset {0}")]
    Utf8(usize),
    /// A varint ran longer than its type allows.
    #[error("varint too long at offset {0}")]
    VarintTooLong(usize),
}

/// A decoded NBT value (only the tags this reader supports).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NbtValue {
    /// TAG_Byte.
    Byte(i8),
    /// TAG_Short (2-byte little-endian).
    Short(i16),
    /// TAG_Int (zig-zag varint).
    Int(i32),
    /// TAG_String (varint length + UTF-8).
    Str(String),
    /// TAG_Compound, entries keyed and ordered by name.
    Compound(BTreeMap<String, NbtValue>),
}

/// A cursor over a byte buffer.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, NbtError> {
        let b = *self.data.get(self.pos).ok_or(NbtError::Eof(self.pos))?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], NbtError> {
        let end = self.pos.checked_add(n).ok_or(NbtError::Eof(self.pos))?;
        let s = self
            .data
            .get(self.pos..end)
            .ok_or(NbtError::Eof(self.pos))?;
        self.pos = end;
        Ok(s)
    }

    /// Unsigned LEB128 varint, at most 5 bytes (u32).
    fn uvarint(&mut self) -> Result<u32, NbtError> {
        let mut result: u32 = 0;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            result |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 32 {
                return Err(NbtError::VarintTooLong(self.pos));
            }
        }
    }

    /// Zig-zag decoded signed varint (i32).
    fn ivarint(&mut self) -> Result<i32, NbtError> {
        let u = self.uvarint()?;
        Ok(((u >> 1) as i32) ^ -((u & 1) as i32))
    }

    fn string(&mut self) -> Result<String, NbtError> {
        let len = self.uvarint()? as usize;
        let at = self.pos;
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(ToString::to_string)
            .map_err(|_| NbtError::Utf8(at))
    }

    fn payload(&mut self, tag: u8) -> Result<NbtValue, NbtError> {
        match tag {
            1 => Ok(NbtValue::Byte(self.u8()? as i8)),
            2 => {
                let b = self.take(2)?;
                Ok(NbtValue::Short(i16::from_le_bytes([b[0], b[1]])))
            }
            3 => Ok(NbtValue::Int(self.ivarint()?)),
            8 => Ok(NbtValue::Str(self.string()?)),
            10 => {
                let mut map = BTreeMap::new();
                loop {
                    let t = self.u8()?;
                    if t == 0 {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(t)?;
                    map.insert(name, value);
                }
                Ok(NbtValue::Compound(map))
            }
            other => Err(NbtError::UnsupportedTag {
                tag: other,
                offset: self.pos - 1,
            }),
        }
    }
}

/// Decode a stream of consecutive root compounds (as in `block_states.nbt`),
/// returning each root's `(name, compound)`. The root name is usually empty.
pub fn decode_root_compounds(data: &[u8]) -> Result<Vec<(String, NbtValue)>, NbtError> {
    let mut r = Reader { data, pos: 0 };
    let mut out = Vec::new();
    while r.pos < data.len() {
        let tag = r.u8()?;
        if tag != 10 {
            return Err(NbtError::UnsupportedTag {
                tag,
                offset: r.pos - 1,
            });
        }
        let name = r.string()?;
        let value = r.payload(10)?;
        out.push((name, value));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one network-LE root record: `{name: <name>, states: {<k>: <byte/int/str>}}`.
    fn record(name: &str, states: &[(&str, NbtValue)]) -> Vec<u8> {
        fn ws(buf: &mut Vec<u8>, s: &str) {
            // unsigned varint length (short strings fit in one byte) + bytes
            let mut n = s.len() as u32;
            loop {
                let mut b = (n & 0x7f) as u8;
                n >>= 7;
                if n != 0 {
                    b |= 0x80;
                }
                buf.push(b);
                if n == 0 {
                    break;
                }
            }
            buf.extend_from_slice(s.as_bytes());
        }
        fn zig(buf: &mut Vec<u8>, v: i32) {
            let mut u = ((v << 1) ^ (v >> 31)) as u32;
            loop {
                let mut b = (u & 0x7f) as u8;
                u >>= 7;
                if u != 0 {
                    b |= 0x80;
                }
                buf.push(b);
                if u == 0 {
                    break;
                }
            }
        }
        let mut b = Vec::new();
        b.push(10); // root compound tag
        ws(&mut b, ""); // root name (empty)
        b.push(8);
        ws(&mut b, "name");
        ws(&mut b, name);
        b.push(10);
        ws(&mut b, "states");
        for (k, v) in states {
            match v {
                NbtValue::Byte(x) => {
                    b.push(1);
                    ws(&mut b, k);
                    b.push(*x as u8);
                }
                NbtValue::Int(x) => {
                    b.push(3);
                    ws(&mut b, k);
                    zig(&mut b, *x);
                }
                NbtValue::Str(s) => {
                    b.push(8);
                    ws(&mut b, k);
                    ws(&mut b, s);
                }
                _ => unreachable!(),
            }
        }
        b.push(0); // end states
        b.push(0); // end root
        b
    }

    #[test]
    fn decodes_a_record() {
        let mut bytes = record("minecraft:stone", &[]);
        bytes.extend(record(
            "minecraft:furnace",
            &[
                ("facing_direction", NbtValue::Int(3)),
                ("lit", NbtValue::Byte(1)),
            ],
        ));
        let roots = decode_root_compounds(&bytes).unwrap();
        assert_eq!(roots.len(), 2);

        let NbtValue::Compound(first) = &roots[0].1 else {
            panic!()
        };
        assert_eq!(
            first.get("name"),
            Some(&NbtValue::Str("minecraft:stone".into()))
        );
        assert_eq!(
            first.get("states"),
            Some(&NbtValue::Compound(Default::default()))
        );

        let NbtValue::Compound(second) = &roots[1].1 else {
            panic!()
        };
        let NbtValue::Compound(states) = second.get("states").unwrap() else {
            panic!()
        };
        assert_eq!(states.get("facing_direction"), Some(&NbtValue::Int(3)));
        assert_eq!(states.get("lit"), Some(&NbtValue::Byte(1)));
    }

    #[test]
    fn unsupported_tag_errs() {
        // Root compound, empty name, one entry named "x" with tag 6 (Double): unsupported.
        let bytes = [10u8, 0, 6, 1, b'x'];
        let err = decode_root_compounds(&bytes).unwrap_err();
        assert!(matches!(err, NbtError::UnsupportedTag { tag: 6, .. }));
    }
}

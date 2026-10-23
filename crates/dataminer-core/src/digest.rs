//! SHA-256 digests, used to pin exactly which bytes produced a spec.

use std::fmt;
use std::io::{self, Read};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A SHA-256 digest, serialized as lowercase hex (`sha256:<64 hex>`).
///
/// Every extracted spec records the digest of the artifact it was read from, so
/// a reviewer can confirm they are looking at the same bytes we were.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Hash a byte slice.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let out = hasher.finalize();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&out);
        Self(buf)
    }

    /// Hash everything a reader yields, without holding it all in memory.
    pub fn of_reader(mut reader: impl Read) -> io::Result<Self> {
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&hasher.finalize());
        Ok(Self(out))
    }

    /// The raw 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, without the `sha256:` prefix.
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", self.to_hex())
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256Digest({self})")
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        let hex = s.strip_prefix("sha256:").unwrap_or(&s);
        if hex.len() != 64 {
            return Err(D::Error::custom("sha256 digest must be 64 hex chars"));
        }
        let mut buf = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let byte = std::str::from_utf8(chunk)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| D::Error::custom("sha256 digest must be hex"))?;
            buf[i] = byte;
        }
        Ok(Self(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // SHA-256 of the empty string.
        let d = Sha256Digest::of_bytes(b"");
        assert_eq!(
            d.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(d.to_string(), format!("sha256:{}", d.to_hex()));
    }

    #[test]
    fn reader_matches_bytes() {
        let data = vec![7u8; 200_000];
        let d = Sha256Digest::of_reader(&data[..]).unwrap();
        assert_eq!(d, Sha256Digest::of_bytes(&data));
    }

    #[test]
    fn round_trips_through_json() {
        let d = Sha256Digest::of_bytes(b"ferrock");
        let json = serde_json::to_string(&d).unwrap();
        let back: Sha256Digest = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn rejects_malformed() {
        assert!(serde_json::from_str::<Sha256Digest>("\"sha256:zz\"").is_err());
        assert!(serde_json::from_str::<Sha256Digest>("\"nothex\"").is_err());
    }
}

//! Core vocabulary shared by every Dataminer extractor.
//!
//! Dataminer turns legally clean sources about Minecraft: Bedrock Edition into
//! versioned, diffable *specs* that a clean-room Rust reimplementation (Ferrock)
//! can build against. This crate defines what a spec is and, just as important,
//! where each fact came from.
//!
//! The guiding rule, borrowed from mature reverse-engineering practice: a fact
//! is only as trustworthy as its evidence. So every value Dataminer emits is
//! wrapped in enough provenance that a reader can decide whether to rely on it,
//! and a reviewer can reproduce it. We never emit a number we cannot point at a
//! source for.

pub mod block_hash;
pub mod digest;
pub mod provenance;
pub mod spec;

pub use block_hash::{StateValue, network_block_hash};
pub use digest::Sha256Digest;
pub use provenance::{Method, Provenance, SourceKind};
pub use spec::{SpecDocument, SpecError, SpecMeta};

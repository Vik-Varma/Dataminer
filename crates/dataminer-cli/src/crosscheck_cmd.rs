//! `dataminer crosscheck`: verify a dataset against an independent source and
//! report disagreements (the trust step Ferrock's requirements ask for).
//!
//! Today it checks the `blockstates` dataset against an authoritative
//! `block_states.nbt` (network little-endian NBT, as embedded by Dragonfly /
//! gophertunnel, MIT): it recomputes the network hash of every authoritative
//! state and compares the hash sets, attributing the differences by block.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use dataminer_core::block_hash::{StateValue, network_block_hash};
use dataminer_core::nbt::{self, NbtValue};
use serde_json::{Value, json};

/// Arguments for `dataminer crosscheck blockstates`.
#[derive(clap::Args)]
pub struct BlockStatesArgs {
    /// Our `blockstates.json` spec document.
    #[arg(long)]
    spec: PathBuf,
    /// An authoritative `block_states.nbt` (network little-endian NBT) to check
    /// against, e.g. Dragonfly's embedded file.
    #[arg(long)]
    against: PathBuf,
    /// Write the full JSON report here instead of only printing a summary.
    #[arg(long)]
    out: Option<PathBuf>,
}

/// Run `crosscheck blockstates`.
pub fn run_blockstates(args: BlockStatesArgs) -> Result<()> {
    // Authoritative side: decode and hash each state from its own NBT tags.
    let nbt_bytes = std::fs::read(&args.against)
        .with_context(|| format!("reading {}", args.against.display()))?;
    let roots = nbt::decode_root_compounds(&nbt_bytes).context("decoding block_states.nbt")?;

    let mut theirs: BTreeMap<u32, String> = BTreeMap::new();
    for (_root_name, value) in &roots {
        let NbtValue::Compound(map) = value else {
            anyhow::bail!("block_states.nbt root was not a compound");
        };
        let Some(NbtValue::Str(name)) = map.get("name") else {
            anyhow::bail!("block_states.nbt record missing a string `name`");
        };
        let states = match map.get("states") {
            Some(NbtValue::Compound(s)) => s,
            None => &BTreeMap::new(),
            _ => anyhow::bail!("block_states.nbt `states` was not a compound"),
        };
        let pairs = to_state_values(name, states)?;
        theirs.insert(network_block_hash(name, &pairs), name.clone());
    }

    // Our side: read the spec.
    let spec_bytes =
        std::fs::read(&args.spec).with_context(|| format!("reading {}", args.spec.display()))?;
    let spec: Value = serde_json::from_slice(&spec_bytes).context("parsing blockstates spec")?;
    let game_version = spec
        .pointer("/meta/provenance/game_version")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string();
    let blocks = spec
        .pointer("/payload/blocks")
        .and_then(Value::as_array)
        .context("spec has no payload.blocks (is this a blockstates document?)")?;

    let mut ours: BTreeMap<u32, String> = BTreeMap::new();
    for b in blocks {
        let name = b.get("name").and_then(Value::as_str).unwrap_or("?");
        if let Some(states) = b.get("states").and_then(Value::as_array) {
            for s in states {
                if let Some(h) = s.get("hash").and_then(Value::as_u64) {
                    ours.insert(h as u32, name.to_string());
                }
            }
        }
    }

    let our_set: std::collections::BTreeSet<u32> = ours.keys().copied().collect();
    let their_set: std::collections::BTreeSet<u32> = theirs.keys().copied().collect();
    let matched = our_set.intersection(&their_set).count();
    let only_ours = histogram(our_set.difference(&their_set).map(|h| ours[h].clone()));
    let only_theirs = histogram(their_set.difference(&our_set).map(|h| theirs[h].clone()));
    let only_ours_total: usize = our_set.difference(&their_set).count();
    let only_theirs_total: usize = their_set.difference(&our_set).count();

    println!(
        "crosscheck blockstates ({game_version}): ours={} theirs={} matched={matched}",
        our_set.len(),
        their_set.len()
    );
    println!("  only ours ({only_ours_total}): {}", fmt_hist(&only_ours));
    println!(
        "  only theirs ({only_theirs_total}): {}",
        fmt_hist(&only_theirs)
    );

    if let Some(path) = &args.out {
        let report = json!({
            "spec": args.spec.display().to_string(),
            "against": args.against.display().to_string(),
            "game_version": game_version,
            "our_hashes": our_set.len(),
            "authoritative_hashes": their_set.len(),
            "matched": matched,
            "only_ours_total": only_ours_total,
            "only_theirs_total": only_theirs_total,
            "only_ours": only_ours.iter().map(|(b, n)| json!({"block": b, "states": n})).collect::<Vec<_>>(),
            "only_theirs": only_theirs.iter().map(|(b, n)| json!({"block": b, "states": n})).collect::<Vec<_>>(),
        });
        std::fs::write(path, serde_json::to_string_pretty(&report)? + "\n")
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}

/// Convert an authoritative `states` compound into hashable `(key, value)` pairs.
/// Block-state values are Byte (0/1 bool), Int, or String; anything else is an
/// error so a surprising encoding is surfaced, not silently coerced.
fn to_state_values(
    block: &str,
    states: &BTreeMap<String, NbtValue>,
) -> Result<Vec<(String, StateValue)>> {
    let mut pairs = Vec::with_capacity(states.len());
    for (k, v) in states {
        let sv = match v {
            NbtValue::Byte(0) => StateValue::Bool(false),
            NbtValue::Byte(1) => StateValue::Bool(true),
            NbtValue::Int(i) => StateValue::Int(*i),
            NbtValue::Str(s) => StateValue::Str(s.clone()),
            other => anyhow::bail!(
                "block {block} property {k}: unexpected block-state NBT value {other:?}"
            ),
        };
        pairs.push((k.clone(), sv));
    }
    Ok(pairs)
}

fn histogram(names: impl Iterator<Item = String>) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for n in names {
        *counts.entry(n).or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

fn fmt_hist(h: &[(String, usize)]) -> String {
    if h.is_empty() {
        return "none".to_string();
    }
    h.iter()
        .take(12)
        .map(|(b, n)| format!("{b}×{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

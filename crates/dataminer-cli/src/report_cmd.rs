//! The `dataminer report` subcommand: a coverage summary for a spec document,
//! and a non-zero exit when it finds gaps — the CI hook Ferrock asked for.
//!
//! Today the gap signals are the protocol ingester's `unresolved` list and any
//! record marked `"method": "unknown"`. As datasets gain optional fields that
//! can be `unknown` (block collision boxes, physics constants), the same scan
//! reports field-level coverage without changing shape.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde_json::Value;

/// Arguments for `dataminer report`.
#[derive(clap::Args)]
pub struct ReportArgs {
    /// The spec JSON to summarize.
    spec: PathBuf,
    /// Exit non-zero if any gaps (unresolved entries or `unknown` methods) exist.
    #[arg(long)]
    strict: bool,
}

/// Run the report. Returns an exit code so `--strict` can fail CI.
pub fn run(args: ReportArgs) -> Result<ExitCode> {
    let bytes =
        std::fs::read(&args.spec).with_context(|| format!("reading {}", args.spec.display()))?;
    let doc: Value = serde_json::from_slice(&bytes).context("parsing spec JSON")?;

    if let Some(meta) = doc.get("meta") {
        let kind = meta.get("kind").and_then(Value::as_str).unwrap_or("?");
        let sv = meta
            .get("schema_version")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let prov = meta.get("provenance");
        let gv = prov
            .and_then(|p| p.get("game_version"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        let method = prov
            .and_then(|p| p.get("method"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        println!("{kind} (schema v{sv})  game_version={gv}  method={method}");
    }

    let payload = doc.get("payload").unwrap_or(&doc);

    // Per-collection record counts: every top-level payload field that is an array.
    if let Some(obj) = payload.as_object() {
        for (k, v) in obj {
            if let Some(arr) = v.as_array() {
                println!("  {}: {}", k, arr.len());
            }
        }
    }

    // Gaps: an `unresolved` list on the payload, plus any `"method":"unknown"`.
    let unresolved: Vec<String> = payload
        .get("unresolved")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let unknown_methods = count_unknown_methods(payload);
    let gaps = unresolved.len() + unknown_methods;

    if gaps == 0 {
        println!("coverage: no gaps");
    } else {
        println!("coverage: {gaps} gap(s)");
        if !unresolved.is_empty() {
            println!(
                "  unresolved ({}): {}",
                unresolved.len(),
                unresolved.join(", ")
            );
        }
        if unknown_methods > 0 {
            println!("  records with method=unknown: {unknown_methods}");
        }
    }

    if args.strict && gaps > 0 {
        eprintln!("report: {gaps} gap(s) with --strict; failing");
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Count occurrences of `"method": "unknown"` anywhere in the value.
fn count_unknown_methods(v: &Value) -> usize {
    match v {
        Value::Object(map) => {
            let here = usize::from(map.get("method").and_then(Value::as_str) == Some("unknown"));
            here + map.values().map(count_unknown_methods).sum::<usize>()
        }
        Value::Array(arr) => arr.iter().map(count_unknown_methods).sum(),
        _ => 0,
    }
}

//! Block-breaking speed and timing.
//!
//! When a survival player starts breaking a block, the server answers with
//! LevelEvent 3600 (start cracking) whose data is floor(65535 · r), where r is
//! the share of the block broken per input. The model here gives r from the
//! block, the held item and the player's state. [`derive`] requires it to
//! reproduce every measured 3600 value exactly, and requires the server's break
//! timing to follow N = ceil(1/r) inputs: the server breaks the block itself on
//! input N − 1 counted from the start-break input, and accepts a destroy
//! prediction from that input on.

use std::collections::BTreeMap;

use dataminer_core::provenance::Provenance;
use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::{MeasureError, evidence};

/// The spec kind.
pub const KIND: &str = "break-speed";
/// The payload schema version.
pub const SCHEMA_VERSION: u32 = 1;
/// LevelEvent 3600's data for one whole break.
pub const CRACK_SCALE: f64 = 65535.0;
/// Slack for float rounding when a product should land exactly on an integer.
const SLACK: f64 = 1e-7;

/// `PlayerAuthInput` block actions, as wire values for protocol 2193.
pub mod actions {
    /// Start breaking.
    pub const START_BREAK: i32 = 0;
    /// Stop breaking without finishing.
    pub const ABORT_BREAK: i32 = 1;
    /// Stop breaking.
    pub const STOP_BREAK: i32 = 2;
    /// Crack the block.
    pub const CRACK_BREAK: i32 = 18;
    /// The client predicts the block is destroyed.
    pub const PREDICT_DESTROY: i32 = 26;
    /// Carry on breaking.
    pub const CONTINUE_DESTROY: i32 = 27;
}

/// A tool's kind, from its item name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// `*_pickaxe`.
    Pickaxe,
    /// `*_shovel`.
    Shovel,
    /// `*_axe`.
    Axe,
    /// `*_hoe`.
    Hoe,
    /// `*_sword`.
    Sword,
    /// `shears`.
    Shears,
}

/// A tool tier: its speed on blocks it suits and its harvest level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tier {
    /// The item-name prefix, e.g. `wooden`.
    pub name: String,
    /// Speed s on a block the tool suits.
    pub speed: f64,
    /// Compared with a block's required level.
    pub harvest_level: u8,
}

const TIERS: [(&str, f64, u8); 6] = [
    ("wooden", 2.0, 0),
    ("stone", 4.0, 1),
    ("iron", 6.0, 2),
    ("diamond", 8.0, 3),
    ("netherite", 9.0, 4),
    ("golden", 12.0, 0),
];

/// The speed of shears, and of a sword, on a block they suit.
pub const FIXED_SPEED: f64 = 15.0;

/// What a block needs for the faster rate (D = 30 rather than 100).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "needs")]
pub enum Harvest {
    /// Anything, a bare hand included.
    Anything,
    /// A tool of this kind and at least this harvest level.
    Tool {
        /// The tool kind.
        kind: ToolKind,
        /// The lowest tier's harvest level that will do.
        min_level: u8,
    },
    /// Shears or a sword.
    ShearsOrSword,
}

/// One block's breaking parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockParams {
    /// The block name as given to `/setblock`.
    pub block: String,
    /// Hardness H; `None` for a block that never breaks in survival.
    pub hardness: Option<f64>,
    /// The interval `(lo, hi]` every measured 3600 value allows for H.
    pub hardness_bracket: Option<[f64; 2]>,
    /// The tools that speed it up.
    pub suited_tools: Vec<ToolKind>,
    /// What gives the faster rate.
    pub harvest: Harvest,
}

use Harvest::{Anything, ShearsOrSword};
use ToolKind::{Axe, Hoe, Pickaxe, Shears, Shovel, Sword};

const fn pick(min_level: u8) -> Harvest {
    Harvest::Tool {
        kind: Pickaxe,
        min_level,
    }
}

/// The measured blocks: name, hardness, suited tools, harvest rule.
const BLOCKS: [(&str, Option<f64>, &[ToolKind], Harvest); 16] = [
    ("bedrock", None, &[], Anything),
    ("cobblestone", Some(2.0), &[Pickaxe], pick(0)),
    ("deepslate", Some(3.0), &[Pickaxe], pick(0)),
    ("dirt", Some(0.5), &[Shovel], Anything),
    ("end_stone", Some(3.0), &[Pickaxe], pick(0)),
    ("glass", Some(0.3), &[], Anything),
    ("gravel", Some(0.6), &[Shovel], Anything),
    ("iron_ore", Some(3.0), &[Pickaxe], pick(1)),
    ("netherrack", Some(0.4), &[Pickaxe], pick(0)),
    ("oak_leaves", Some(0.2), &[Hoe, Shears], Anything),
    ("oak_log", Some(2.0), &[Axe], Anything),
    ("oak_planks", Some(2.0), &[Axe], Anything),
    ("obsidian", Some(35.0), &[Pickaxe], pick(3)),
    ("sand", Some(0.5), &[Shovel], Anything),
    ("stone", Some(1.5), &[Pickaxe], pick(0)),
    ("web", Some(4.0), &[Shears, Sword], ShearsOrSword),
];

/// The model, as mathematical definitions.
pub fn definition() -> Vec<String> {
    [
        "r = s / (H · D) · M, the share of the block broken per input",
        "s = the held tool's speed if the block lists the tool among suited_tools (tier speed, \
         or 15 for shears and swords), else 1; plus L² + 1 for Efficiency L on a suited tool",
        "D = 30 if the held item meets the block's harvest rule, else 100",
        "M = (1 + 0.2·h) · 1.2^h for Haste level h (amplifier + 1; Conduit Power counts as \
         Haste 1) × 0.21^f for Mining Fatigue level f × 1/5 with the eyes in water × 1/5 when \
         not on the ground",
        "LevelEvent 3600 data = floor(65535 · r)",
        "N = ceil(1 / r); counting the start-break input as 0, the server breaks the block \
         itself on input N − 1, and accepts a destroy prediction on input N − 1 but not before; \
         it answers a rejected prediction by re-sending the block as it was",
        "in creative, a start-break breaks the block on the same input, unless a sword is held",
        "a break that is not stopped goes on after the block is gone: when water flows into the \
         cell, crack updates for it follow (updates_after_break); a continue-destroy after \
         the break starts cracking whatever fills the cell (starts_after_break)",
    ]
    .map(String::from)
    .to_vec()
}

#[derive(Clone, Copy, Debug)]
struct Held {
    kind: ToolKind,
    speed: f64,
    level: u8,
}

fn held(item: &str) -> Result<Option<Held>, MeasureError> {
    if item.is_empty() {
        return Ok(None);
    }
    if item == "shears" {
        return Ok(Some(Held {
            kind: Shears,
            speed: FIXED_SPEED,
            level: 0,
        }));
    }
    let (tier, kind) = item
        .split_once('_')
        .ok_or_else(|| evidence(format!("unrecognised held item {item:?}")))?;
    let kind = match kind {
        "pickaxe" => Pickaxe,
        "shovel" => Shovel,
        "axe" => Axe,
        "hoe" => Hoe,
        "sword" => Sword,
        _ => return Err(evidence(format!("unrecognised held item {item:?}"))),
    };
    let &(_, speed, level) = TIERS
        .iter()
        .find(|t| t.0 == tier)
        .ok_or_else(|| evidence(format!("unrecognised tool tier in {item:?}")))?;
    let speed = if kind == Sword { FIXED_SPEED } else { speed };
    Ok(Some(Held { kind, speed, level }))
}

/// `effect` and `enchant` arguments as the console took them, e.g.
/// `haste 30 1` (amplifier 1, level 2) and `efficiency 3`.
fn level(arg: &str, name: &str, amplifier_field: usize) -> Result<u32, MeasureError> {
    let v: Vec<&str> = arg.split_whitespace().collect();
    if v.first() != Some(&name) {
        return Ok(0);
    }
    let n: u32 = v
        .get(amplifier_field)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| evidence(format!("can't read a level from {arg:?}")))?;
    Ok(if amplifier_field == 2 { n + 1 } else { n })
}

/// Haste's factor for level h.
pub fn haste_factor(h: u32) -> f64 {
    (1.0 + 0.2 * f64::from(h)) * 1.2f64.powi(h as i32)
}

/// Mining Fatigue's factor for level f.
pub fn fatigue_factor(f: u32) -> f64 {
    0.21f64.powi(f as i32)
}

/// The inputs a break takes: ceil(1/r).
pub fn inputs_to_break(r: f64) -> u32 {
    ((1.0 / r) - SLACK).ceil().max(1.0) as u32
}

/// The rate r for one case and the parts it was made of: `(s/D·M, r)`.
fn rate(rec: &RawRecord, block: &BlockParams) -> Result<(f64, Option<f64>), MeasureError> {
    let h = held(&rec.held)?;
    let suits = h.filter(|h| block.suited_tools.contains(&h.kind));
    let mut s = suits.map_or(1.0, |h| h.speed);
    let efficiency = level(&rec.enchant, "efficiency", 1)?;
    if efficiency > 0 {
        // Measured only on a tool that suits the block.
        if suits.is_none() {
            return Err(evidence(format!(
                "case {:?}: Efficiency on an item that doesn't suit {}, which the model doesn't \
                 cover",
                rec.name, block.block
            )));
        }
        s += f64::from(efficiency.pow(2) + 1);
    }
    let harvests = match &block.harvest {
        Anything => true,
        Harvest::Tool { kind, min_level } => {
            h.is_some_and(|h| h.kind == *kind && h.level >= *min_level)
        }
        ShearsOrSword => h.is_some_and(|h| matches!(h.kind, Shears | Sword)),
    };
    let d = if harvests { 30.0 } else { 100.0 };
    let haste = level(&rec.effect, "haste", 2)?.max(level(&rec.effect, "conduit_power", 2)?);
    let mut m = haste_factor(haste) * fatigue_factor(level(&rec.effect, "mining_fatigue", 2)?);
    let (wet, floating) = match rec.where_.as_str() {
        "ground" => (false, false),
        "water" => (true, false),
        "air" => (false, true),
        "water+air" => (true, true),
        w => return Err(evidence(format!("unknown place {w:?}"))),
    };
    if wet {
        m /= 5.0;
    }
    if floating {
        m /= 5.0;
    }
    let q = s / d * m;
    Ok((q, block.hardness.map(|hd| q / hd)))
}

#[derive(Clone, Debug, Deserialize)]
struct RawAction {
    k: i64,
    action: i32,
}

#[derive(Clone, Debug, Deserialize)]
struct RawKData {
    k: i64,
    data: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct RawUpdate {
    data: i64,
    after_break: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct RawPredict {
    k: i64,
    target: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RawRecord {
    name: String,
    block: String,
    mode: String,
    #[serde(default)]
    held: String,
    #[serde(default)]
    enchant: String,
    #[serde(default)]
    effect: String,
    #[serde(rename = "where")]
    where_: String,
    fresh_setup: bool,
    inputs: i64,
    actions: Option<Vec<RawAction>>,
    crack_start: Option<Vec<RawKData>>,
    crack_update: Option<Vec<RawUpdate>>,
    broke_k: Option<i64>,
    predict_replies: Option<Vec<RawPredict>>,
}

/// One action the client sent, `k` inputs after the first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SentAction {
    /// Input index.
    pub k: i64,
    /// The block action (see [`actions`]).
    pub action: i32,
}

/// A destroy prediction and how the server answered it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prediction {
    /// Input index.
    pub k: i64,
    /// `accepted` or `rejected`, from the model.
    pub expected: String,
    /// `restored` (the block re-sent as it was), `changed` (re-sent with its
    /// new value) or `none` (not re-sent).
    pub observed: String,
}

/// One measured case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// What the case does.
    pub name: String,
    /// The block.
    pub block: String,
    /// `s` (survival) or `c` (creative).
    pub mode: String,
    /// The held item; empty for a bare hand.
    pub held: String,
    /// The enchantment command's arguments, if any.
    pub enchant: String,
    /// The effect command's arguments, if any.
    pub effect: String,
    /// `ground`, `water`, `air` (not on the ground) or `water+air`.
    #[serde(rename = "where")]
    pub place: String,
    /// What the client sent.
    pub actions: Vec<SentAction>,
    /// Inputs the replies were watched for, from the first action.
    pub inputs_watched: i64,
    /// The model's r (survival only).
    pub rate: Option<f64>,
    /// The model's 3600 data.
    pub expected_crack_start: Option<i64>,
    /// The 3600 data the server sent.
    pub crack_start: Option<i64>,
    /// The model's N.
    pub inputs_to_break: Option<u32>,
    /// The input the model says the block breaks on.
    pub expected_break_input: Option<i64>,
    /// Crack-start data sent after the break input, distinct values: for what
    /// then fills the cell (0 for air).
    pub starts_after_break: Vec<i64>,
    /// Crack-update data the server went on sending for the cell after the
    /// break, distinct values (in water: for the water that flowed in).
    pub updates_after_break: Vec<i64>,
    /// The input during which the break reply arrived (the break input or the
    /// one after, by reply timing).
    pub broke_reply_input: Option<i64>,
    /// Destroy predictions sent, and the answers.
    pub predictions: Vec<Prediction>,
}

/// The `break-speed` payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BreakSpeed {
    /// The model.
    pub definition: Vec<String>,
    /// Tool tiers.
    pub tiers: Vec<Tier>,
    /// Shears' and swords' speed on blocks they suit.
    pub fixed_tool_speed: f64,
    /// Per-block parameters, sorted by block.
    pub blocks: Vec<BlockParams>,
    /// Every case, sorted by name.
    pub observations: Vec<Observation>,
    /// What the cases don't cover.
    pub not_covered: Vec<String>,
}

fn blocks() -> Vec<BlockParams> {
    BLOCKS
        .iter()
        .map(|(b, h, t, hv)| BlockParams {
            block: (*b).into(),
            hardness: *h,
            hardness_bracket: None,
            suited_tools: t.to_vec(),
            harvest: hv.clone(),
        })
        .collect()
}

/// The server's expected answers to a survival case's actions, given N.
fn simulate(acts: &[RawAction], n: Option<u32>) -> (Option<i64>, Vec<(i64, bool)>) {
    let mut start: Option<i64> = None;
    let mut broke: Option<i64> = None;
    let mut predictions = Vec::new();
    for a in acts {
        if broke.is_some_and(|b| a.k > b) {
            if a.action == actions::PREDICT_DESTROY {
                predictions.push((a.k, true));
            }
            continue;
        }
        match a.action {
            actions::START_BREAK if start.is_none() => {
                start = Some(a.k);
                broke = n.map(|n| a.k + i64::from(n) - 1);
            }
            actions::ABORT_BREAK => {
                start = None;
                broke = None;
            }
            actions::PREDICT_DESTROY => predictions.push((a.k, broke == Some(a.k))),
            _ => {}
        }
    }
    (broke, predictions)
}

fn simulate_creative(acts: &[RawAction], sword: bool) -> (Option<i64>, Vec<(i64, bool)>) {
    let mut broke = None;
    let mut predictions = Vec::new();
    for a in acts {
        match a.action {
            actions::START_BREAK if !sword && broke.is_none() => broke = Some(a.k),
            actions::PREDICT_DESTROY => predictions.push((a.k, broke.is_some())),
            _ => {}
        }
    }
    (broke, predictions)
}

fn check_break(rec: &RawRecord, expected: Option<i64>) -> Result<(), MeasureError> {
    let ok = match (expected, rec.broke_k) {
        (Some(e), Some(b)) => b == e || b == e + 1,
        // A break due on the last watched input may not have been seen.
        (Some(e), None) => e + 1 >= rec.inputs,
        (None, None) => true,
        (None, Some(_)) => false,
    };
    if ok {
        Ok(())
    } else {
        Err(evidence(format!(
            "case {:?}: expected the break on input {expected:?}, the reply came on {:?}",
            rec.name, rec.broke_k
        )))
    }
}

/// Parse the client's break records (one JSON object per line) into a spec.
pub fn derive(log: &str, provenance: Provenance) -> Result<SpecDocument<BreakSpeed>, MeasureError> {
    let mut params: BTreeMap<String, BlockParams> =
        blocks().into_iter().map(|b| (b.block.clone(), b)).collect();
    let mut brackets: BTreeMap<String, [f64; 2]> = BTreeMap::new();
    let mut observations = Vec::new();
    for rec in crate::parse_lines::<RawRecord>(log)? {
        if !rec.fresh_setup {
            return Err(evidence(format!(
                "case {:?}: the server never confirmed the target block was set, so the case \
                 may have run on a stale world",
                rec.name
            )));
        }
        if observations
            .iter()
            .any(|o: &Observation| o.name == rec.name)
        {
            return Err(evidence(format!("case {:?} appears twice", rec.name)));
        }
        let block = params
            .get(&rec.block)
            .ok_or_else(|| evidence(format!("no model for block {:?}", rec.block)))?
            .clone();
        let acts = rec.actions.clone().unwrap_or_default();
        // Crack starts after the break input are for whatever then fills the cell.
        // The split is at the input the break reply arrived on, not the model's:
        // a late reply delays every receipt alike, so the filler's first start
        // still comes after it.
        let before_break = |k: i64| rec.broke_k.is_none_or(|b| k <= b);
        let starts: Vec<i64> = rec
            .crack_start
            .iter()
            .flatten()
            .filter(|d| before_break(d.k))
            .map(|d| d.data)
            .collect();
        let mut starts_after: Vec<i64> = rec
            .crack_start
            .iter()
            .flatten()
            .filter(|d| !before_break(d.k))
            .map(|d| d.data)
            .collect();
        starts_after.sort_unstable();
        starts_after.dedup();
        let all_updates = rec.crack_update.clone().unwrap_or_default();
        let updates: Vec<i64> = all_updates
            .iter()
            .filter(|u| !u.after_break)
            .map(|u| u.data)
            .collect();
        let mut after_break: Vec<i64> = all_updates
            .iter()
            .filter(|u| u.after_break)
            .map(|u| u.data)
            .collect();
        after_break.sort_unstable();
        after_break.dedup();
        let (mut rate_v, mut want_start, mut n) = (None, None, None);
        let (expected_break, preds) = match rec.mode.as_str() {
            "s" => {
                let (q, r) = rate(&rec, &block)?;
                let r = r.unwrap_or(0.0);
                let d = (CRACK_SCALE * r + SLACK).floor() as i64;
                if !acts.iter().any(|a| a.action == actions::START_BREAK) {
                    if !starts.is_empty() || !updates.is_empty() {
                        return Err(evidence(format!(
                            "case {:?}: crack data {starts:?} {updates:?} with no start-break",
                            rec.name
                        )));
                    }
                } else {
                    let Some(&got) = starts.first() else {
                        return Err(evidence(format!("case {:?}: no crack start", rec.name)));
                    };
                    if starts.iter().any(|&s| s != d) || updates.iter().any(|&u| u != d) {
                        return Err(evidence(format!(
                            "case {:?}: the model gives crack data {d}, the server sent start \
                             {starts:?} and updates {updates:?}",
                            rec.name
                        )));
                    }
                    if let Some(h) = block.hardness.filter(|_| got > 0) {
                        let lo = CRACK_SCALE * q / (got + 1) as f64;
                        let hi = CRACK_SCALE * q / got as f64;
                        let b = brackets.entry(rec.block.clone()).or_insert([lo, hi]);
                        b[0] = b[0].max(lo);
                        b[1] = b[1].min(hi);
                        if !(h > b[0] && h <= b[1] * (1.0 + SLACK)) {
                            return Err(evidence(format!(
                                "block {:?}: hardness {h} is outside ({}, {}]",
                                rec.block, b[0], b[1]
                            )));
                        }
                    }
                }
                rate_v = Some(r);
                want_start = Some(d);
                n = (r > 0.0).then(|| inputs_to_break(r));
                simulate(&acts, n)
            }
            "c" => {
                if !starts.is_empty() {
                    return Err(evidence(format!(
                        "case {:?}: creative breaking sent a crack start",
                        rec.name
                    )));
                }
                let sword = held(&rec.held)?.is_some_and(|h| h.kind == Sword);
                simulate_creative(&acts, sword)
            }
            m => return Err(evidence(format!("case {:?}: unknown mode {m:?}", rec.name))),
        };
        check_break(&rec, expected_break)?;
        let replies = rec.predict_replies.clone().unwrap_or_default();
        if let Some(r) = replies
            .iter()
            .find(|r| !preds.iter().any(|(k, _)| *k == r.k))
        {
            return Err(evidence(format!(
                "case {:?}: an answer at input {} to no prediction",
                rec.name, r.k
            )));
        }
        let mut predictions = Vec::new();
        for (k, accepted) in &preds {
            // The server answers a rejected prediction by re-sending the block
            // as it was. An accepted one gets the block's new value, or no
            // re-send at all.
            let observed = replies
                .iter()
                .find(|r| r.k == *k)
                .map_or("none", |r| r.target.as_str());
            let fits = if *accepted {
                observed != "restored"
            } else {
                observed == "restored"
            };
            if !fits {
                return Err(evidence(format!(
                    "case {:?}: prediction at input {k} should be {}, the server answered {observed}",
                    rec.name,
                    if *accepted { "accepted" } else { "rejected" }
                )));
            }
            predictions.push(Prediction {
                k: *k,
                expected: if *accepted { "accepted" } else { "rejected" }.into(),
                observed: observed.into(),
            });
        }
        observations.push(Observation {
            name: rec.name,
            block: rec.block,
            mode: rec.mode,
            held: rec.held,
            enchant: rec.enchant,
            effect: rec.effect,
            place: rec.where_,
            actions: acts
                .iter()
                .map(|a| SentAction {
                    k: a.k,
                    action: a.action,
                })
                .collect(),
            inputs_watched: rec.inputs,
            rate: rate_v,
            expected_crack_start: want_start,
            crack_start: starts.first().copied(),
            inputs_to_break: n,
            expected_break_input: expected_break,
            starts_after_break: starts_after,
            updates_after_break: after_break,
            broke_reply_input: rec.broke_k,
            predictions,
        });
    }
    if let Some(b) = params
        .values()
        .find(|b| !observations.iter().any(|o| o.block == b.block))
    {
        return Err(evidence(format!("block {:?} has no case", b.block)));
    }
    for (b, br) in brackets {
        if let Some(p) = params.get_mut(&b) {
            p.hardness_bracket = Some(br);
        }
    }
    observations.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        BreakSpeed {
            definition: definition(),
            tiers: TIERS
                .iter()
                .map(|(n, s, l)| Tier {
                    name: (*n).into(),
                    speed: *s,
                    harvest_level: *l,
                })
                .collect(),
            fixed_tool_speed: FIXED_SPEED,
            blocks: params.into_values().collect(),
            observations,
            not_covered: [
                "Aqua Affinity",
                "Efficiency on a tool that doesn't suit the block, and Efficiency with Haste",
                "tool kinds on blocks other than those in observations",
                "breaking while looking or moving away from the block",
                "what the vanilla client sends",
            ]
            .map(String::from)
            .to_vec(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dataminer_core::provenance::{Method, SourceKind};

    fn prov() -> Provenance {
        Provenance {
            source: SourceKind::PacketCapture,
            method: Method::Measured,
            game_version: "test".into(),
            artifact: "synthetic".into(),
            artifact_sha256: None,
            tool: crate::TOOL.into(),
            notes: vec![],
        }
    }

    /// One record per block in the model, each a start-only break by hand
    /// with the right crack data and break input.
    fn baseline() -> Vec<String> {
        blocks()
            .iter()
            .map(|b| {
                let rec: RawRecord =
                    serde_json::from_str(&rec_json(&b.block, "", 0, "null", 60)).unwrap();
                let (d, brk) = match rate(&rec, b).unwrap().1 {
                    None => (0, "null".to_string()),
                    Some(r) => {
                        let n = inputs_to_break(r);
                        let brk = if n < 60 {
                            (n - 1).to_string()
                        } else {
                            "null".into()
                        };
                        ((CRACK_SCALE * r + SLACK).floor() as i64, brk)
                    }
                };
                rec_json(&b.block, "", d, &brk, 60)
            })
            .collect()
    }

    fn rec_json(block: &str, held: &str, data: i64, broke: &str, inputs: i64) -> String {
        format!(
            r#"{{"name":"{block} by {held}","block":"{block}","mode":"s","held":"{held}","enchant":"","effect":"","where":"ground","fresh_setup":true,"inputs":{inputs},"actions":[{{"k":0,"action":0}}],"crack_start":[{{"k":0,"data":{data}}}],"crack_update":[{{"data":{data},"after_break":false}}],"broke_k":{broke},"predict_replies":null}}"#
        )
    }

    fn with(extra: &str) -> String {
        let mut v = baseline();
        v.push(extra.to_string());
        v.join("\n")
    }

    #[test]
    fn the_model_reproduces_measured_values() {
        let doc = derive(&baseline().join("\n"), prov()).unwrap();
        let dirt = doc
            .payload
            .observations
            .iter()
            .find(|o| o.block == "dirt")
            .unwrap();
        assert_eq!(dirt.crack_start, Some(4369));
        assert_eq!(dirt.inputs_to_break, Some(15));
        let obs = doc
            .payload
            .blocks
            .iter()
            .find(|b| b.block == "obsidian")
            .unwrap();
        let [lo, hi] = obs.hardness_bracket.unwrap();
        assert!(lo < 35.0 && 35.0 <= hi);
        // A few measured values: stone by a wooden pickaxe, iron ore by a
        // wooden pickaxe (too low a tier), web by shears, leaves by shears.
        for (block, held, data) in [
            ("stone", "wooden_pickaxe", 2912),
            ("iron_ore", "wooden_pickaxe", 436),
            ("web", "shears", 8191),
            ("oak_leaves", "shears", 163837),
            ("obsidian", "diamond_pickaxe", 499),
        ] {
            let rec: RawRecord =
                serde_json::from_str(&rec_json(block, held, 0, "null", 60)).unwrap();
            let b = blocks().into_iter().find(|b| b.block == block).unwrap();
            let r = rate(&rec, &b).unwrap().1.unwrap();
            assert_eq!(
                (CRACK_SCALE * r + SLACK).floor() as i64,
                data,
                "{block} by {held}"
            );
        }
    }

    #[test]
    fn effects_and_enchantments_follow_the_formula() {
        assert!((haste_factor(1) - 1.44).abs() < 1e-12);
        assert!((fatigue_factor(2) - 0.0441).abs() < 1e-12);
        let b = blocks().into_iter().find(|b| b.block == "stone").unwrap();
        for (enchant, effect, place, data) in [
            ("efficiency 5", "", "ground", 40777),
            ("", "haste 30 4", "ground", 14495),
            ("", "mining_fatigue 30 0", "ground", 611),
            ("", "conduit_power 30 0", "water", 838),
            ("", "", "water+air", 116),
        ] {
            let line = rec_json("stone", "wooden_pickaxe", 0, "null", 60)
                .replace(r#""enchant":"""#, &format!(r#""enchant":"{enchant}""#))
                .replace(r#""effect":"""#, &format!(r#""effect":"{effect}""#))
                .replace(r#""where":"ground""#, &format!(r#""where":"{place}""#));
            let rec: RawRecord = serde_json::from_str(&line).unwrap();
            let r = rate(&rec, &b).unwrap().1.unwrap();
            assert_eq!(
                (CRACK_SCALE * r + SLACK).floor() as i64,
                data,
                "{enchant}{effect}{place}"
            );
        }
    }

    #[test]
    fn rejects_a_crack_value_off_by_one() {
        assert!(
            derive(
                &with(&rec_json("stone", "wooden_pickaxe", 2913, "22", 60)),
                prov()
            )
            .is_err()
        );
    }

    #[test]
    fn updates_after_the_break_are_reported_not_checked() {
        let line = rec_json("dirt", "", 4369, "14", 30).replace(
            r#""crack_update":[{"data":4369,"after_break":false}]"#,
            r#""crack_update":[{"data":4369,"after_break":false},{"data":4,"after_break":true}]"#,
        );
        let name = line.replace(r#""name":"dirt by ""#, r#""name":"dirt, water after""#);
        let doc = derive(&with(&name), prov()).unwrap();
        let o = doc
            .payload
            .observations
            .iter()
            .find(|o| o.name == "dirt, water after")
            .unwrap();
        assert_eq!(o.updates_after_break, vec![4]);
        let before = name.replace(
            r#""data":4,"after_break":true"#,
            r#""data":4,"after_break":false"#,
        );
        assert!(derive(&with(&before), prov()).is_err());
    }

    #[test]
    fn rejects_a_break_on_the_wrong_input() {
        assert!(
            derive(
                &with(&rec_json("stone", "wooden_pickaxe", 2912, "20", 60)),
                prov()
            )
            .is_err()
        );
        assert!(
            derive(
                &with(&rec_json("stone", "wooden_pickaxe", 2912, "null", 60)),
                prov()
            )
            .is_err()
        );
        assert!(
            derive(
                &with(&rec_json("stone", "wooden_pickaxe", 2912, "23", 60)),
                prov()
            )
            .is_ok()
        );
    }

    fn predict_case(k: i64, target: &str) -> String {
        rec_json("dirt", "", 4369, "14", 30)
            .replace(r#""name":"dirt by ""#, &format!(r#""name":"dirt predict {k}""#))
            .replace(
                r#""actions":[{"k":0,"action":0}]"#,
                &format!(r#""actions":[{{"k":0,"action":0}},{{"k":{k},"action":26}},{{"k":{k},"action":2}}]"#),
            )
            .replace(
                r#""predict_replies":null"#,
                &format!(r#""predict_replies":[{{"k":{k},"target":"{target}"}}]"#),
            )
    }

    #[test]
    fn a_prediction_is_accepted_from_input_n_minus_1() {
        assert!(derive(&with(&predict_case(13, "restored")), prov()).is_ok());
        assert!(derive(&with(&predict_case(14, "changed")), prov()).is_ok());
        assert!(derive(&with(&predict_case(13, "changed")), prov()).is_err());
        assert!(derive(&with(&predict_case(14, "restored")), prov()).is_err());
        let silent = |k| {
            let c = predict_case(k, "restored");
            let at = c.find(r#""predict_replies""#).unwrap();
            format!(r#"{}"predict_replies":null}}"#, &c[..at])
        };
        assert!(derive(&with(&silent(14)), prov()).is_ok());
        assert!(derive(&with(&silent(13)), prov()).is_err());
    }

    #[test]
    fn an_abort_cancels_the_break() {
        let abort = rec_json("dirt", "", 4369, "null", 40)
            .replace(r#""name":"dirt by ""#, r#""name":"dirt abort""#)
            .replace(
                r#""actions":[{"k":0,"action":0}]"#,
                r#""actions":[{"k":0,"action":0},{"k":5,"action":1}]"#,
            );
        assert!(derive(&with(&abort), prov()).is_ok());
        assert!(
            derive(
                &with(&abort.replace(r#""broke_k":null"#, r#""broke_k":14"#)),
                prov()
            )
            .is_err()
        );
    }

    #[test]
    fn creative_breaks_at_once_unless_a_sword_is_held() {
        let creative = |held: &str, broke: &str| {
            format!(
                r#"{{"name":"creative {held}","block":"dirt","mode":"c","held":"{held}","enchant":"","effect":"","where":"ground","fresh_setup":true,"inputs":30,"actions":[{{"k":0,"action":0}}],"crack_start":null,"crack_update":null,"broke_k":{broke},"predict_replies":null}}"#
            )
        };
        assert!(derive(&with(&creative("", "0")), prov()).is_ok());
        assert!(derive(&with(&creative("wooden_sword", "null")), prov()).is_ok());
        assert!(derive(&with(&creative("wooden_sword", "0")), prov()).is_err());
    }

    #[test]
    fn rejects_unknown_places_unmodelled_efficiency_and_unchecked_cracks() {
        let err = |extra: String| derive(&with(&extra), prov()).unwrap_err().to_string();
        let stairs = rec_json("stone", "wooden_pickaxe", 2912, "null", 60)
            .replace(r#""where":"ground""#, r#""where":"stairs""#);
        assert!(err(stairs).contains("unknown place"));
        let unsuited = rec_json("dirt", "wooden_pickaxe", 4369, "null", 60)
            .replace(r#""enchant":"""#, r#""enchant":"efficiency 5""#);
        assert!(err(unsuited).contains("Efficiency"));
        let no_start = rec_json("dirt", "", 4369, "null", 60)
            .replace("dirt by ", "dirt, continue only")
            .replace(r#""action":0"#, r#""action":27"#);
        assert!(err(no_start).contains("no start-break"));
    }

    #[test]
    fn rejects_a_stale_setup_an_unknown_block_and_a_missing_block() {
        let stale = rec_json("stone", "", 436, "null", 60).replace("true", "false");
        assert!(derive(&with(&stale.replace("stone by ", "stale")), prov()).is_err());
        assert!(derive(&with(&rec_json("granite", "", 1, "null", 60)), prov()).is_err());
        let mut v = baseline();
        v.retain(|l| !l.contains(r#""block":"web""#));
        assert!(derive(&v.join("\n"), prov()).is_err());
    }
}

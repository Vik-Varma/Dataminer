//! `minecraft:bamboo`: collision boxes per state, and the per-position offset.
//!
//! The boxes are measured: the player walks into a stalk along each horizontal
//! axis from both sides, and the server's stop position minus the player's
//! half-width is the face it hit. The offset model (which inputs, which
//! generator, how a draw becomes a step) was established on the dirty side by
//! static and dynamic analysis of BDS and is checked here against every run:
//! [`derive`] fails unless it predicts each stop to within [`TOLERANCE`].

use std::collections::BTreeMap;

use dataminer_core::provenance::{Method, Provenance};
use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::{MeasureError, Run};

/// The spec kind.
pub const KIND: &str = "bamboo";
/// The payload schema version.
pub const SCHEMA_VERSION: u32 = 1;
/// How far a prediction may be from a measured stop, in blocks.
pub const TOLERANCE: f64 = 0.001;

/// One axis of the offset: `steps` evenly spaced values from `min` to `max`,
/// chosen by the `draw`-th output of the position's generator (1-based).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisOffset {
    /// `x`, `y` or `z`.
    pub axis: char,
    /// Smallest offset, in blocks.
    pub min: f32,
    /// Largest offset, in blocks. Equal to `min` means the axis is not offset.
    pub max: f32,
    /// Number of distinct values.
    pub steps: u32,
    /// Which generator output this axis uses. Every axis consumes its draw,
    /// whether or not it is offset.
    pub draw: u32,
}

/// Bamboo's offset per axis, as read from our own running BDS (the offset
/// routine's parameters) and confirmed by every measured stop.
pub const AXES: [AxisOffset; 3] = [
    AxisOffset {
        axis: 'x',
        min: -0.25,
        max: 0.25,
        steps: 16,
        draw: 1,
    },
    AxisOffset {
        axis: 'y',
        min: 0.0,
        max: 0.0,
        steps: 0,
        draw: 2,
    },
    AxisOffset {
        axis: 'z',
        min: -0.25,
        max: 0.25,
        steps: 16,
        draw: 3,
    },
];

const SEED_XOR: u64 = 0x6a09_e667_f3bc_c909;
const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;

/// The 64-bit seed for block column `(x, z)`. `y` is not an input.
pub fn position_seed(x: i32, z: i32) -> u64 {
    let a = i64::from(x.wrapping_mul(3_129_871));
    let l = (a ^ i64::from(z).wrapping_mul(116_129_781)) as u64;
    let h = l
        .wrapping_mul(l)
        .wrapping_mul(42_317_861)
        .wrapping_add(l.wrapping_mul(11));
    i64::from((h >> 16) as u32 as i32) as u64
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The first three generator outputs for a seed, each as a float in [0, 1)
/// with 24 bits of precision.
pub fn draws(seed: u64) -> [f32; 3] {
    let lo = seed ^ SEED_XOR;
    let (mut s0, mut s1) = (mix(lo), mix(lo.wrapping_add(GOLDEN)));
    if s0 == 0 && s1 == 0 {
        (s0, s1) = (GOLDEN, SEED_XOR);
    }
    let mut out = [0.0; 3];
    for f in &mut out {
        let r = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);
        *f = (r >> 40) as f32 * (1.0 / 16_777_216.0);
        let t = s1 ^ s0;
        s0 = s0.rotate_left(49) ^ t ^ (t << 21);
        s1 = t.rotate_left(28);
    }
    out
}

/// The step index (0-based) an axis takes for a draw.
pub fn step_index(axis: &AxisOffset, draw: f32) -> u32 {
    (draw * axis.steps as f32).floor() as u32
}

/// The offset of one axis for a draw, in single precision as the server does it.
pub fn axis_offset(axis: &AxisOffset, draw: f32) -> f32 {
    if axis.min >= axis.max {
        return axis.min;
    }
    match axis.steps {
        0 => axis.min + (axis.max - axis.min) * draw,
        1 => (axis.min + axis.max) * 0.5,
        n => axis.min + step_index(axis, draw) as f32 * ((axis.max - axis.min) / (n - 1) as f32),
    }
}

/// Bamboo's offset at block column `(x, z)`, in blocks.
pub fn offset(x: i32, z: i32) -> [f32; 3] {
    let d = draws(position_seed(x, z));
    [0, 1, 2].map(|i| axis_offset(&AXES[i], d[AXES[i].draw as usize - 1]))
}

/// A block state of `minecraft:bamboo`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct State {
    /// `thin` or `thick`.
    pub bamboo_stalk_thickness: String,
    /// `no_leaves`, `small_leaves` or `large_leaves`.
    pub bamboo_leaf_size: String,
    /// `false` or `true`.
    pub age_bit: bool,
}

impl State {
    /// Every state, in metadata order.
    pub fn all() -> Vec<State> {
        let mut v = Vec::new();
        for t in ["thin", "thick"] {
            for l in ["no_leaves", "small_leaves", "large_leaves"] {
                for a in [false, true] {
                    v.push(State {
                        bamboo_stalk_thickness: t.into(),
                        bamboo_leaf_size: l.into(),
                        age_bit: a,
                    });
                }
            }
        }
        v
    }

    /// Parse the state from a `/setblock` block argument, filling defaults.
    pub fn from_block_arg(block: &str) -> Option<State> {
        let rest = block.trim().strip_prefix("bamboo")?.trim();
        let mut s = State {
            bamboo_stalk_thickness: "thin".into(),
            bamboo_leaf_size: "no_leaves".into(),
            age_bit: false,
        };
        let inner = match rest {
            "" => return Some(s),
            r => r.strip_prefix('[')?.strip_suffix(']')?,
        };
        for kv in inner.split(',') {
            let (k, v) = kv.split_once('=')?;
            let v = v.trim().trim_matches('"');
            match k.trim().trim_matches('"') {
                "bamboo_stalk_thickness" => s.bamboo_stalk_thickness = v.into(),
                "bamboo_leaf_size" => s.bamboo_leaf_size = v.into(),
                "age_bit" => s.age_bit = v.parse().ok()?,
                _ => return None,
            }
        }
        Some(s)
    }

    fn label(&self) -> String {
        format!(
            "{}/{}/{}",
            self.bamboo_stalk_thickness, self.bamboo_leaf_size, self.age_bit
        )
    }
}

/// An axis-aligned box in sixteenths of a block, relative to the block's
/// minimum corner, before the offset is added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Box16 {
    /// `[x, y, z]` minimum corner.
    pub min: [f64; 3],
    /// `[x, y, z]` maximum corner.
    pub max: [f64; 3],
}

/// The collision box of one state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateBox {
    /// The block state.
    pub state: State,
    /// The box before the offset.
    pub collision_box: Box16,
    /// How the box was obtained.
    pub method: Method,
    /// Distinct block columns walked into for this state.
    pub columns_measured: usize,
    /// Per-field caveats.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Which of the block's shapes take the offset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeUse {
    /// `collision`, `outline` or `rendering`.
    pub shape: String,
    /// Whether the offset is added; `None` when not established.
    pub offset_applied: Option<bool>,
    /// How that was established.
    pub method: Method,
    /// The evidence, or what is missing.
    pub note: String,
}

/// The offset model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OffsetModel {
    /// How the model was established.
    pub method: Method,
    /// What the offset depends on.
    pub inputs: Vec<String>,
    /// Per-axis parameters.
    pub axes: Vec<AxisOffset>,
    /// The model as a sequence of mathematical definitions.
    pub definition: Vec<String>,
}

/// One measured stop and what the model predicts for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// The state label, `thickness/leaf_size/age_bit`.
    pub state: String,
    /// The block position.
    pub pos: [i32; 3],
    /// The approach (`+x`, `-x`, `+z`, `-z`, or `drop`).
    pub dir: String,
    /// The coordinate the server stopped the player at (feet; x or z for a
    /// walk, y for a drop).
    pub measured: f32,
    /// What the model predicts.
    pub predicted: f32,
}

/// How well the model matches the measurements.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Validation {
    /// Largest allowed |measured − predicted|, in blocks.
    pub tolerance_blocks: f64,
    /// Largest |measured − predicted| seen, in blocks.
    pub max_abs_error_blocks: f64,
    /// The player's horizontal half-width the stops were converted with,
    /// measured by walking into full blocks.
    pub player_half_width: f64,
    /// Every stop used.
    pub observations: Vec<Observation>,
}

/// The `bamboo` payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bamboo {
    /// `minecraft:bamboo`.
    pub block: String,
    /// Per-state collision boxes, in sixteenths, before the offset.
    pub states: Vec<StateBox>,
    /// The per-position offset added to the box.
    pub offset: OffsetModel,
    /// Which shapes take the offset.
    pub shapes: Vec<ShapeUse>,
    /// The evidence.
    pub validation: Validation,
}

fn evidence(msg: impl Into<String>) -> MeasureError {
    MeasureError::Evidence(msg.into())
}

/// The player's half-width, from walks into full blocks of `stone`.
fn half_width(runs: &[&Run]) -> Result<f64, MeasureError> {
    let est: Vec<f64> = runs
        .iter()
        .map(|r| {
            let (bx, bz) = (f64::from(r.bx), f64::from(r.bz));
            let (x, z) = (f64::from(r.feet[0]), f64::from(r.feet[2]));
            match r.dir.as_str() {
                "+x" => Ok(bx - x),
                "-x" => Ok(x - bx - 1.0),
                "+z" => Ok(bz - z),
                "-z" => Ok(z - bz - 1.0),
                d => Err(evidence(format!("stone run with approach {d}"))),
            }
        })
        .collect::<Result<_, _>>()?;
    if est.len() < 4 {
        return Err(evidence("need stone runs from all four directions"));
    }
    let mean = est.iter().sum::<f64>() / est.len() as f64;
    if est.iter().any(|e| (e - mean).abs() > TOLERANCE / 10.0) {
        return Err(evidence(format!(
            "stone runs disagree on the half-width: {est:?}"
        )));
    }
    Ok((mean * 1e4).round() / 1e4)
}

/// Snap a value in sixteenths to the nearest sixteenth, if it is within tolerance.
fn snap16(v: f64) -> Option<f64> {
    let r = v.round();
    ((v - r).abs() <= TOLERANCE * 16.0).then_some(r)
}

/// The face of the stalk a stop implies, and which box field it measures:
/// `(axis, is_max, block-relative face in sixteenths)`.
fn face(r: &Run, hw: f64, off: [f32; 3]) -> Option<(usize, bool, f64)> {
    let (axis, is_max, sign) = match r.dir.as_str() {
        "+x" => (0, false, 1.0),
        "-x" => (0, true, -1.0),
        "+z" => (2, false, 1.0),
        "-z" => (2, true, -1.0),
        _ => return None,
    };
    let base = [r.bx, r.by, r.bz][axis];
    let world = f64::from(r.feet[axis]) + sign * hw;
    Some((
        axis,
        is_max,
        (world - f64::from(base) - f64::from(off[axis])) * 16.0,
    ))
}

fn predict(r: &Run, b: &Box16, hw: f64) -> f32 {
    let off = offset(r.bx, r.bz);
    let pos = [r.bx, r.by, r.bz].map(|v| v as f32);
    let corner = |axis: usize, max: bool| {
        let v = if max { b.max[axis] } else { b.min[axis] };
        f64::from(pos[axis] + off[axis] + (v / 16.0) as f32)
    };
    let p = match r.dir.as_str() {
        "+x" => corner(0, false) - hw,
        "-x" => corner(0, true) + hw,
        "+z" => corner(2, false) - hw,
        "-z" => corner(2, true) + hw,
        _ => corner(1, true),
    };
    p as f32
}

/// Derive the bamboo spec from a run log. `provenance` describes the server
/// and the log; it is attached as given.
pub fn derive(runs: &[Run], provenance: Provenance) -> Result<SpecDocument<Bamboo>, MeasureError> {
    if let Some(r) = runs.iter().find(|r| r.late_corrections > 0) {
        return Err(evidence(format!(
            "run at {:?} {} was still being corrected after it stopped",
            (r.bx, r.by, r.bz),
            r.dir
        )));
    }
    let stone: Vec<&Run> = runs.iter().filter(|r| r.block.trim() == "stone").collect();
    let hw = half_width(&stone)?;

    let mut bamboo: Vec<(State, &Run)> = Vec::new();
    for r in runs.iter().filter(|r| r.block.trim() != "stone") {
        let s = State::from_block_arg(&r.block)
            .ok_or_else(|| evidence(format!("unrecognised block {:?}", r.block)))?;
        bamboo.push((s, r));
    }

    // (state) -> field -> faces seen, in sixteenths
    let mut faces: BTreeMap<State, BTreeMap<(usize, bool), Vec<f64>>> = BTreeMap::new();
    let mut columns: BTreeMap<State, std::collections::BTreeSet<(i32, i32)>> = BTreeMap::new();
    for (s, r) in &bamboo {
        let f = if r.dir == "drop" {
            Some((1, true, (f64::from(r.feet[1]) - f64::from(r.by)) * 16.0))
        } else {
            face(r, hw, offset(r.bx, r.bz))
        };
        let (axis, is_max, v) = f.ok_or_else(|| evidence(format!("unknown approach {}", r.dir)))?;
        faces
            .entry(s.clone())
            .or_default()
            .entry((axis, is_max))
            .or_default()
            .push(v);
        if r.dir != "drop" {
            columns.entry(s.clone()).or_default().insert((r.bx, r.bz));
        }
    }

    let mut states = Vec::new();
    for s in State::all() {
        let Some(f) = faces.get(&s) else {
            return Err(evidence(format!("no runs for state {}", s.label())));
        };
        let mut b = Box16 {
            min: [0.0; 3],
            max: [16.0; 3],
        };
        for axis in [0, 2] {
            for is_max in [false, true] {
                let vs = f.get(&(axis, is_max)).ok_or_else(|| {
                    evidence(format!("state {} lacks a walk for axis {axis}", s.label()))
                })?;
                let snapped: Vec<Option<f64>> = vs.iter().map(|v| snap16(*v)).collect();
                let first = snapped[0];
                if first.is_none() || snapped.iter().any(|v| *v != first) {
                    return Err(evidence(format!(
                        "state {} faces don't agree on a sixteenth: {vs:?}",
                        s.label()
                    )));
                }
                let v = first.unwrap_or_default();
                if is_max {
                    b.max[axis] = v
                } else {
                    b.min[axis] = v
                }
            }
        }
        let mut notes = vec![
            "y min 0 is not measurable by walking or dropping; read from our running BDS's \
             base box for the thin state (single source)"
                .to_string(),
        ];
        match f.get(&(1, true)) {
            Some(vs) => {
                let tops: Vec<Option<f64>> = vs.iter().map(|v| snap16(*v)).collect();
                if tops.iter().any(|t| t.is_none() || *t != tops[0]) {
                    return Err(evidence(format!(
                        "state {} tops disagree: {vs:?}",
                        s.label()
                    )));
                }
                b.max[1] = tops[0].unwrap_or_default();
            }
            None => notes
                .push("y max 16 not measured for this state; measured 16 for other states".into()),
        }
        states.push(StateBox {
            columns_measured: columns.get(&s).map_or(0, |c| c.len()),
            state: s,
            collision_box: b,
            method: Method::Measured,
            notes,
        });
    }

    let by_state: BTreeMap<&State, &Box16> = states
        .iter()
        .map(|sb| (&sb.state, &sb.collision_box))
        .collect();
    let mut observations = Vec::new();
    let mut worst = 0.0f64;
    for (s, r) in &bamboo {
        let predicted = predict(r, by_state[s], hw);
        let axis = match r.dir.as_str() {
            "+x" | "-x" => 0,
            "+z" | "-z" => 2,
            _ => 1,
        };
        let measured = r.feet[axis];
        worst = worst.max((f64::from(measured) - f64::from(predicted)).abs());
        observations.push(Observation {
            state: s.label(),
            pos: [r.bx, r.by, r.bz],
            dir: r.dir.clone(),
            measured,
            predicted,
        });
    }
    if worst > TOLERANCE {
        return Err(evidence(format!(
            "model misses a measured stop by {worst} blocks (tolerance {TOLERANCE})"
        )));
    }
    observations.sort_by(|a, b| {
        (&a.state, a.pos, &a.dir, a.measured.to_bits()).cmp(&(
            &b.state,
            b.pos,
            &b.dir,
            b.measured.to_bits(),
        ))
    });

    let payload = Bamboo {
        block: "minecraft:bamboo".into(),
        states,
        offset: offset_model(),
        shapes: shape_use(),
        validation: Validation {
            tolerance_blocks: TOLERANCE,
            max_abs_error_blocks: (worst * 1e7).round() / 1e7,
            player_half_width: hw,
            observations,
        },
    };
    Ok(SpecDocument::new(KIND, SCHEMA_VERSION, provenance, payload))
}

fn offset_model() -> OffsetModel {
    let defs = [
        "All integer arithmetic is two's complement; 64-bit operations are modulo 2^64; >> on 64-bit values is a logical shift; rotl is a 64-bit left rotation.",
        "Inputs: the block's integer x and z. y is not an input: the same column gives the same offset at every height.",
        "a = x * 3129871 computed in 32 bits (wrapping), then sign-extended to 64 bits.",
        "b = z * 116129781 computed in 64 bits, with z sign-extended.",
        "l = a XOR b.",
        "h = l*l*42317861 + l*11 (mod 2^64).",
        "seed = the low 32 bits of (h >> 16), read as a signed 32-bit integer and sign-extended to 64 bits.",
        "lo = seed XOR 0x6a09e667f3bcc909; hi = lo + 0x9e3779b97f4a7c15.",
        "M(v): v = (v XOR (v >> 30)) * 0xbf58476d1ce4e5b9; v = (v XOR (v >> 27)) * 0x94d049bb133111eb; result v XOR (v >> 31).",
        "s0 = M(lo), s1 = M(hi). If both are 0, s0 = 0x9e3779b97f4a7c15 and s1 = 0x6a09e667f3bcc909.",
        "Output k (k = 1, 2, 3, in order): r_k = rotl(s0 + s1, 17) + s0; then t = s1 XOR s0, s0 = rotl(s0, 49) XOR t XOR (t << 21), s1 = rotl(t, 28).",
        "f_k = (r_k >> 40) / 2^24, a value in [0, 1) with 24 bits.",
        "For an axis with min < max and steps n >= 2: index = floor(f * n) (0 .. n-1), offset = min + index * ((max - min) / (n - 1)). For bamboo, x uses f_1 and z uses f_3: offset = -0.25 + index/30, index 0..15. y uses f_2 but has min = max = 0, so its offset is 0.",
        "The server evaluates the offset and the box in IEEE-754 single precision: step = 0.5f/15f, offset = -0.25f + index*step, then box corner = (float(block coordinate) + offset) + corner/16.",
    ];
    OffsetModel {
        method: Method::Measured,
        inputs: vec!["block x".into(), "block z".into()],
        axes: AXES.to_vec(),
        definition: defs.iter().map(|d| (*d).to_string()).collect(),
    }
}

fn shape_use() -> Vec<ShapeUse> {
    vec![
        ShapeUse {
            shape: "collision".into(),
            offset_applied: Some(true),
            method: Method::Measured,
            note: "every walk and drop in validation.observations matches box + offset".into(),
        },
        ShapeUse {
            shape: "outline".into(),
            offset_applied: Some(true),
            method: Method::Inferred,
            note: "static analysis of BDS only: the selection-shape path calls the same \
                   per-position offset routine with the same base box as collision. Not \
                   measured; needs a second source before Ferrock relies on it"
                .into(),
        },
        ShapeUse {
            shape: "rendering".into(),
            offset_applied: None,
            method: Method::Unknown,
            note: "BDS has no renderer, so the client's use is not observable here. BDS does \
                   carry a shared accessor that returns the same per-position offset, which \
                   suggests but does not establish that rendering uses it"
                .into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Step indices (x, z) measured on BDS 1.26.52.3: the stalk's measured
    // centre, less the base box centre, as a multiple of 1/30 from -0.25.
    const MEASURED_STEPS: [((i32, i32), (u32, u32)); 12] = [
        ((0, 0), (2, 4)),
        ((-1, -1), (5, 2)),
        ((0, -1), (13, 3)),
        ((-1, 0), (5, 15)),
        ((3, 3), (12, 13)),
        ((-14, -18), (13, 7)),
        ((19, -4), (14, 2)),
        ((-2001, 1500), (0, 11)),
        ((1234, -987), (12, 1)),
        ((-777, -3333), (12, 1)),
        ((3000, 2999), (9, 9)),
        ((-4095, -4097), (9, 3)),
    ];

    #[test]
    fn matches_measured_steps() {
        for ((x, z), (nx, nz)) in MEASURED_STEPS {
            let d = draws(position_seed(x, z));
            assert_eq!(step_index(&AXES[0], d[0]), nx, "x at {x},{z}");
            assert_eq!(step_index(&AXES[2], d[2]), nz, "z at {x},{z}");
        }
    }

    #[test]
    fn offsets_stay_in_range_and_y_is_never_offset() {
        for x in -40..40 {
            for z in -40..40 {
                let o = offset(x, z);
                assert_eq!(o[1], 0.0);
                for v in [o[0], o[2]] {
                    assert!((-0.25..=0.25).contains(&v), "{v} at {x},{z}");
                }
            }
        }
    }

    #[test]
    fn every_step_is_reachable() {
        let mut seen = [false; 16];
        for x in -64..64 {
            for z in -64..64 {
                seen[step_index(&AXES[0], draws(position_seed(x, z))[0]) as usize] = true;
            }
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn parses_block_arguments() {
        let s = State::from_block_arg(
            r#"bamboo ["bamboo_stalk_thickness"="thick","bamboo_leaf_size"="small_leaves","age_bit"=true]"#,
        )
        .unwrap();
        assert_eq!(s.label(), "thick/small_leaves/true");
        assert_eq!(
            State::from_block_arg("bamboo").unwrap().label(),
            "thin/no_leaves/false"
        );
        assert!(State::from_block_arg("stone").is_none());
        assert_eq!(State::all().len(), 12);
    }

    fn run(block: &str, pos: (i32, i32, i32), dir: &str, feet: [f32; 3]) -> Run {
        Run {
            block: block.into(),
            bx: pos.0,
            by: pos.1,
            bz: pos.2,
            dir: dir.into(),
            feet,
            late_corrections: 0,
        }
    }

    fn synthetic(width16: f64) -> Vec<Run> {
        let mut v = vec![
            run("stone", (3, 200, 3), "+x", [2.7, 200.0, 3.5]),
            run("stone", (3, 200, 3), "-x", [4.3, 200.0, 3.5]),
            run("stone", (3, 200, 3), "+z", [3.5, 200.0, 2.7]),
            run("stone", (3, 200, 3), "-z", [3.5, 200.0, 4.3]),
        ];
        for s in State::all() {
            let w = if s.bamboo_stalk_thickness == "thick" {
                width16 + 1.0
            } else {
                width16
            };
            let block = format!(
                r#"bamboo ["bamboo_stalk_thickness"="{}","bamboo_leaf_size"="{}","age_bit"={}]"#,
                s.bamboo_stalk_thickness, s.bamboo_leaf_size, s.age_bit
            );
            let (x, y, z) = (5, 200, -7);
            let o = offset(x, z);
            let lo = |p: i32, a: usize| p as f32 + o[a] + 0.5;
            let hi = |p: i32, a: usize| lo(p, a) + (w / 16.0) as f32;
            v.push(run(&block, (x, y, z), "+x", [lo(x, 0) - 0.3, 200.0, 0.0]));
            v.push(run(&block, (x, y, z), "-x", [hi(x, 0) + 0.3, 200.0, 0.0]));
            v.push(run(&block, (x, y, z), "+z", [0.0, 200.0, lo(z, 2) - 0.3]));
            v.push(run(&block, (x, y, z), "-z", [0.0, 200.0, hi(z, 2) + 0.3]));
            v.push(run(&block, (x, y, z), "drop", [0.0, 201.0, 0.0]));
        }
        v
    }

    fn prov() -> Provenance {
        Provenance {
            source: dataminer_core::provenance::SourceKind::PacketCapture,
            method: Method::Measured,
            game_version: "test".into(),
            artifact: "synthetic".into(),
            artifact_sha256: None,
            tool: crate::TOOL.into(),
            notes: vec![],
        }
    }

    #[test]
    fn derives_boxes_from_consistent_runs() {
        let doc = derive(&synthetic(2.0), prov()).unwrap();
        let p = &doc.payload;
        assert_eq!(p.validation.player_half_width, 0.3);
        assert_eq!(p.states.len(), 12);
        let thin = &p.states[0].collision_box;
        assert_eq!((thin.min, thin.max), ([8.0, 0.0, 8.0], [10.0, 16.0, 10.0]));
        let thick = &p.states[6].collision_box;
        assert_eq!(
            (thick.min, thick.max),
            ([8.0, 0.0, 8.0], [11.0, 16.0, 11.0])
        );
        assert!(p.validation.max_abs_error_blocks <= TOLERANCE);
    }

    #[test]
    fn rejects_a_stop_the_model_cannot_explain() {
        let mut runs = synthetic(2.0);
        let r = runs.iter_mut().rfind(|r| r.dir == "-x").unwrap();
        r.feet[0] += 0.01;
        assert!(derive(&runs, prov()).is_err());
    }

    #[test]
    fn rejects_a_run_still_being_corrected() {
        let mut runs = synthetic(2.0);
        runs[5].late_corrections = 1;
        assert!(derive(&runs, prov()).is_err());
    }
}

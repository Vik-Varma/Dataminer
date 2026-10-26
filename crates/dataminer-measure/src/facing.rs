//! Which way placed blocks face: the states the server stores for a block a
//! player places, by the player's yaw and position.
//!
//! Horizontal facing comes from yaw alone. Observers, dispensers and pistons
//! can also face up or down; whether they do depends on where the player
//! stands relative to the placed cell. [`derive`] finds the limits of that rule
//! from the cases as brackets, and refuses cases that contradict each other.

use std::collections::BTreeMap;

use dataminer_core::provenance::Provenance;
use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{MeasureError, evidence};

/// The spec kind.
pub const KIND: &str = "block-facing";
/// The payload schema version.
pub const SCHEMA_VERSION: u32 = 1;
/// The player's eye height above the feet.
pub const EYE_HEIGHT: f64 = 1.62;
/// Positions are rounded to 1 / this many blocks before comparing.
const PER_BLOCK: f64 = 1e4;

/// Stored states, by state name.
pub type States = BTreeMap<String, Value>;

#[derive(Clone, Debug, Deserialize)]
struct RawRecord {
    item: String,
    yaw: f32,
    pitch: f32,
    eye: [f32; 3],
    clicked: [i32; 3],
    face: i32,
    click_pos: [f32; 3],
    placed: [i32; 3],
    result: String,
    states: Option<States>,
    setup: Option<Vec<String>>,
}

/// One placement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// The item placed (also the block name).
    pub item: String,
    /// Player yaw, degrees (0 = +z, 90 = −x, 180 = −z, −90 = +x).
    pub yaw: f32,
    /// Player pitch, degrees, positive looking down.
    pub pitch: f32,
    /// The player's feet, from the server's position (eye − 1.62).
    pub feet: [f64; 3],
    /// The clicked block.
    pub clicked: [i32; 3],
    /// The clicked face (0 down, 1 up, 2 north, 3 south, 4 west, 5 east).
    pub face: i32,
    /// The click position relative to the clicked block.
    pub click_pos: [f32; 3],
    /// The cell the block was placed in.
    pub placed: [i32; 3],
    /// Console setup before the placement, besides clearing the area.
    pub setup: Vec<String>,
    /// feet y − the placed cell's y.
    pub feet_minus_y: f64,
    /// The feet's offsets from the cell's centre along x and z.
    pub offset_xz: [f64; 2],
    /// The stored states.
    pub states: States,
}

/// The states for one item and yaw, placed on the floor two blocks away.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YawStates {
    /// The item.
    pub item: String,
    /// Player yaw, degrees.
    pub yaw: f32,
    /// The stored states.
    pub states: States,
}

/// The states one item takes when it faces up or down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerticalStates {
    /// The item.
    pub item: String,
    /// States when the player's feet are above the cell.
    pub feet_above: States,
    /// States when the cell is above the player.
    pub cell_above: States,
}

/// Where vertical facing applies, as measured brackets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerticalRule {
    /// The rule in words.
    pub definition: Vec<String>,
    /// `[lo, hi)`: a lies in it.
    pub feet_above_a: [f64; 2],
    /// `(lo, hi]`: b lies in it.
    pub cell_above_b: [f64; 2],
    /// `[lo, hi)`: L lies in it.
    pub axis_limit_l: [f64; 2],
    /// Whether a straight-line distance limit could also explain the cases.
    /// `false` means only the per-axis limit fits.
    pub straight_line_limit_fits: bool,
    /// Per item, the states for each direction.
    pub items: Vec<VerticalStates>,
}

/// The `block-facing` payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockFacing {
    /// States by yaw, sorted by item then yaw.
    pub horizontal: Vec<YawStates>,
    /// Up and down.
    pub vertical: VerticalRule,
    /// Every placement, sorted.
    pub observations: Vec<Observation>,
    /// What the cases don't cover.
    pub not_covered: Vec<String>,
}

fn round(v: f64) -> f64 {
    (v * PER_BLOCK).round() / PER_BLOCK
}

/// `[max of lower, min of upper]`, or an error naming the contradiction.
fn bracket(what: &str, lower: &[f64], upper: &[f64]) -> Result<[f64; 2], MeasureError> {
    let lo = lower.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let hi = upper.iter().copied().fold(f64::INFINITY, f64::min);
    if lower.is_empty() || upper.is_empty() {
        return Err(evidence(format!("{what}: needs cases on both sides")));
    }
    if lo >= hi {
        return Err(evidence(format!(
            "{what}: the cases contradict each other ({lo} on one side, {hi} on the other)"
        )));
    }
    Ok([lo, hi])
}

/// The four tabled yaws.
const YAWS: [i32; 4] = [-90, 0, 90, 180];

/// A yaw rounded and brought into (−180, 180], which must be one of [`YAWS`].
fn yaw_key(yaw: f32) -> Result<i32, MeasureError> {
    let y = (yaw.round() as i32).rem_euclid(360);
    let y = if y > 180 { y - 360 } else { y };
    if YAWS.contains(&y) {
        Ok(y)
    } else {
        Err(evidence(format!(
            "yaw {yaw}: only 0, 90, 180 and −90 are tabled"
        )))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Dir {
    Flat,
    FeetAbove,
    CellAbove,
}

/// Parse the client's placement records (one JSON object per line) into a spec.
pub fn derive(
    log: &str,
    provenance: Provenance,
) -> Result<SpecDocument<BlockFacing>, MeasureError> {
    let mut obs = Vec::new();
    for r in crate::parse_lines::<RawRecord>(log)? {
        if r.result != format!("minecraft:{}", r.item) {
            return Err(evidence(format!(
                "placing {} at {:?} left {}: the placement failed",
                r.item, r.placed, r.result
            )));
        }
        let feet = [
            f64::from(r.eye[0]),
            round(f64::from(r.eye[1]) - EYE_HEIGHT),
            f64::from(r.eye[2]),
        ];
        obs.push(Observation {
            item: r.item,
            yaw: r.yaw,
            pitch: r.pitch,
            feet,
            clicked: r.clicked,
            face: r.face,
            click_pos: r.click_pos,
            placed: r.placed,
            setup: r.setup.unwrap_or_default(),
            feet_minus_y: round(feet[1] - f64::from(r.placed[1])),
            offset_xz: [
                round(feet[0] - (f64::from(r.placed[0]) + 0.5)),
                round(feet[2] - (f64::from(r.placed[2]) + 0.5)),
            ],
            states: r.states.unwrap_or_default(),
        });
    }

    // The yaw table: on the floor, feet level with the cell, two blocks away.
    let mut horizontal: BTreeMap<(String, i32), States> = BTreeMap::new();
    for o in &obs {
        let far = o.offset_xz.iter().any(|d| d.abs() >= 2.0);
        if o.face == 1 && o.feet_minus_y == 0.0 && far {
            let key = (o.item.clone(), yaw_key(o.yaw)?);
            if let Some(prev) = horizontal.insert(key.clone(), o.states.clone())
                && prev != o.states
            {
                return Err(evidence(format!(
                    "{} at yaw {}: two floor placements disagree",
                    key.0, key.1
                )));
            }
        }
    }
    let items: Vec<String> = {
        let mut v: Vec<String> = obs.iter().map(|o| o.item.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    for it in &items {
        for yaw in YAWS {
            if !horizontal.contains_key(&(it.clone(), yaw)) {
                return Err(evidence(format!("{it}: no floor placement at yaw {yaw}")));
            }
        }
    }

    // Classify every other placement against the yaw table.
    let mut dirs = Vec::new();
    let mut vertical: BTreeMap<String, (Option<States>, Option<States>)> = BTreeMap::new();
    for o in &obs {
        let flat = horizontal.get(&(o.item.clone(), yaw_key(o.yaw)?));
        let base = |s: &States| -> States {
            s.iter()
                .filter(|(k, _)| k.as_str() != "upside_down_bit")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let dir = if flat.map(base) == Some(base(&o.states)) {
            Dir::Flat
        } else if o.feet_minus_y > 0.0 {
            Dir::FeetAbove
        } else if o.feet_minus_y < 0.0 {
            Dir::CellAbove
        } else {
            return Err(evidence(format!(
                "{} level with the player at {:?} did not face by yaw",
                o.item, o.feet
            )));
        };
        if dir != Dir::Flat {
            let e = vertical.entry(o.item.clone()).or_default();
            let slot = if dir == Dir::FeetAbove {
                &mut e.0
            } else {
                &mut e.1
            };
            if let Some(prev) = slot.replace(o.states.clone())
                && prev != o.states
            {
                return Err(evidence(format!(
                    "{}: two vertical placements disagree",
                    o.item
                )));
            }
        }
        dirs.push(dir);
    }
    let vert_items: Vec<VerticalStates> = vertical
        .into_iter()
        .map(|(item, (up, down))| {
            Ok(VerticalStates {
                feet_above: up
                    .ok_or_else(|| evidence(format!("{item}: never faced with feet above")))?,
                cell_above: down
                    .ok_or_else(|| evidence(format!("{item}: never faced with the cell above")))?,
                item,
            })
        })
        .collect::<Result<_, MeasureError>>()?;
    let vert: Vec<(&Observation, Dir)> = obs
        .iter()
        .zip(dirs.iter().copied())
        .filter(|(o, _)| vert_items.iter().any(|v| v.item == o.item))
        .collect();

    // The distance limit, from cases a whole block above or two below: well
    // clear of the height limits.
    let axis = |o: &Observation| o.offset_xz[0].abs().max(o.offset_xz[1].abs());
    let line = |o: &Observation| o.offset_xz[0].hypot(o.offset_xz[1]);
    let clear: Vec<&(&Observation, Dir)> = vert
        .iter()
        .filter(|(o, _)| o.feet_minus_y >= 1.0 || o.feet_minus_y <= -2.0)
        .collect();
    let near = |f: &dyn Fn(&Observation) -> f64, flat: bool| -> Vec<f64> {
        clear
            .iter()
            .filter(|(_, d)| (*d == Dir::Flat) == flat)
            .map(|(o, _)| f(o))
            .collect()
    };
    let axis_limit = bracket("axis limit", &near(&axis, false), &near(&axis, true))?;
    let straight_line_limit_fits =
        bracket("line limit", &near(&line, false), &near(&line, true)).is_ok();

    // The height limits, from cases within the distance limit.
    let close: Vec<&(&Observation, Dir)> = vert
        .iter()
        .filter(|(o, _)| axis(o) <= axis_limit[0])
        .collect();
    let pick = |want: Dir, pred: &dyn Fn(f64) -> bool| -> Vec<f64> {
        close
            .iter()
            .filter(|(o, d)| *d == want && pred(o.feet_minus_y))
            .map(|(o, _)| o.feet_minus_y)
            .collect()
    };
    let a = bracket(
        "feet-above limit",
        &pick(Dir::Flat, &|v| v >= 0.0),
        &pick(Dir::FeetAbove, &|_| true),
    )?;
    let b = bracket(
        "cell-above limit",
        &pick(Dir::CellAbove, &|_| true),
        &pick(Dir::Flat, &|v| v < 0.0),
    )?;

    // Every case must fit the rule for some a, b and L inside their brackets.
    for (o, d) in &vert {
        let (h, ax) = (o.feet_minus_y, axis(o));
        let fits = match d {
            Dir::FeetAbove => h > a[0] && ax < axis_limit[1],
            Dir::CellAbove => h < b[1] && ax < axis_limit[1],
            Dir::Flat => !(ax <= axis_limit[0] && (h >= a[1] || h <= b[0])),
        };
        if !fits {
            return Err(evidence(format!(
                "{} at feet {:?} faced {d:?}, which no rule within a {a:?}, b {b:?} and L \
                 {axis_limit:?} allows",
                o.item, o.feet
            )));
        }
    }

    let mut horizontal_v: Vec<YawStates> = horizontal
        .into_iter()
        .map(|((item, yaw), states)| YawStates {
            item,
            yaw: yaw as f32,
            states,
        })
        .collect();
    horizontal_v.sort_by(|x, y| (&x.item, x.yaw as i32).cmp(&(&y.item, y.yaw as i32)));
    // Every field breaks ties, so the order never depends on the log's.
    obs.sort_by_cached_key(|o| {
        (
            format!(
                "{} {:?} {:?} {:+09.4} {:+09.4} {:+09.4} {:+08.3} {:+08.3}",
                o.item, o.placed, o.face, o.feet[0], o.feet[1], o.feet[2], o.yaw, o.pitch
            ),
            o.setup.clone(),
            format!("{:?} {:?}", o.clicked, o.click_pos),
            serde_json::to_string(&o.states).unwrap_or_default(),
        )
    });
    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        BlockFacing {
            horizontal: horizontal_v,
            vertical: VerticalRule {
                definition: [
                    "with feet = the player's feet y, Y = the placed cell's y, and dx, dz = the \
                     feet's offsets from the cell's centre:",
                    "faces by feet_above when feet − Y > a and max(|dx|, |dz|) ≤ L",
                    "faces by cell_above when feet − Y < b and max(|dx|, |dz|) ≤ L",
                    "otherwise faces by yaw, as in horizontal; pitch does not enter",
                ]
                .map(String::from)
                .to_vec(),
                feet_above_a: a,
                cell_above_b: b,
                axis_limit_l: axis_limit,
                straight_line_limit_fits,
                items: vert_items,
            },
            observations: obs,
            not_covered: [
                "the click height where stairs switch to upside down (only 0.25 and 0.75 tried)",
                "the yaw boundaries between the four horizontal directions (only 0, 90, 180, −90)",
                "the horizontal-only blocks placed with the player above the cell",
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

    fn rec(yaw: f32, feet: [f64; 3], placed: [i32; 3], fd: i32) -> String {
        format!(
            r#"{{"item":"piston","yaw":{yaw},"pitch":39,"eye":[{},{},{}],"clicked":[0,0,0],"face":1,"click_pos":[0.5,1,0.5],"placed":[{},{},{}],"result":"minecraft:piston","states":{{"facing_direction":{fd}}},"setup":null}}"#,
            feet[0],
            feet[1] + EYE_HEIGHT,
            feet[2],
            placed[0],
            placed[1],
            placed[2]
        )
    }

    /// The four yaws, then a hole and a ceiling at several distances and
    /// heights, shaped like the measured cases.
    fn cases() -> Vec<String> {
        let c = [60, 200, 2];
        let mut v = vec![
            rec(180.0, [60.5, 200.0, 4.5], c, 2),
            rec(-90.0, [58.5, 200.0, 2.5], c, 5),
            rec(0.0, [60.5, 200.0, 0.5], c, 3),
            rec(90.0, [62.5, 200.0, 2.5], c, 4),
            rec(0.0, [60.5, 200.0, 1.2], c, 3),
            rec(0.0, [60.5, 200.0625, 1.2], c, 1),
            rec(0.0, [60.5, 200.25, 1.2], [60, 202, 2], 3),
            rec(0.0, [60.5, 200.1875, 1.2], [60, 202, 2], 0),
            rec(0.0, [59.1, 200.0, 1.1], [60, 199, 2], 1),
        ];
        for (z, up) in [(1.0, true), (0.97, false), (0.5, false)] {
            v.push(rec(
                0.0,
                [60.5, 200.0, z],
                [60, 199, 2],
                if up { 1 } else { 3 },
            ));
            v.push(rec(
                0.0,
                [60.5, 200.0, z],
                [60, 202, 2],
                if up { 0 } else { 3 },
            ));
        }
        v
    }

    #[test]
    fn derives_the_brackets() {
        let doc = derive(&cases().join("\n"), prov()).unwrap();
        let v = &doc.payload.vertical;
        assert_eq!(v.feet_above_a, [0.0, 0.0625]);
        assert_eq!(v.cell_above_b, [-1.8125, -1.75]);
        let [lo, hi] = v.axis_limit_l;
        assert!(
            (lo - 1.5).abs() < 1e-9 && (hi - 1.53).abs() < 1e-9,
            "{lo} {hi}"
        );
        assert!(!v.straight_line_limit_fits);
        assert_eq!(v.items[0].feet_above["facing_direction"], 1);
        assert_eq!(doc.payload.horizontal.len(), 4);
    }

    #[test]
    fn rejects_contradicting_cases() {
        let mut v = cases();
        v.push(rec(0.0, [60.5, 200.5, 1.2], [60, 200, 2], 3));
        assert!(derive(&v.join("\n"), prov()).is_err());
    }

    #[test]
    fn rejects_a_case_only_the_final_check_sees() {
        // Feet half a block above the cell, 1.6 out: in neither the bracket
        // subsets, but past the distance limit, so it must face by yaw.
        let mut v = cases();
        v.push(rec(0.0, [60.5, 200.5, 4.1], [60, 200, 2], 1));
        let err = derive(&v.join("\n"), prov()).unwrap_err();
        assert!(err.to_string().contains("no rule"), "{err}");
    }

    #[test]
    fn normalises_yaws_and_refuses_untabled_ones() {
        let mut v = cases();
        v[0] = rec(-180.0, [60.5, 200.0, 4.5], [60, 200, 2], 2);
        assert!(derive(&v.join("\n"), prov()).is_ok());
        let mut v = cases();
        v.push(rec(45.0, [60.5, 200.0, 1.2], [60, 200, 2], 3));
        let err = derive(&v.join("\n"), prov()).unwrap_err();
        assert!(err.to_string().contains("tabled"), "{err}");
    }

    #[test]
    fn rejects_a_failed_placement() {
        let mut v = cases();
        v.push(rec(0.0, [60.5, 200.0, 1.2], [60, 200, 2], 3).replace(
            "\"result\":\"minecraft:piston\"",
            "\"result\":\"minecraft:air\"",
        ));
        assert!(derive(&v.join("\n"), prov()).is_err());
    }

    #[test]
    fn rejects_a_missing_yaw() {
        let v: Vec<String> = cases().into_iter().skip(1).collect();
        assert!(derive(&v.join("\n"), prov()).is_err());
    }
}

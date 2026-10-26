//! Per-tick movement traces: the server's position and velocity after every
//! input tick of a scripted run.
//!
//! A strict server corrects a wrong claim at most once per six inputs, so a
//! single run only shows every sixth tick. The client therefore replays the
//! same inputs from the same start several times, claiming the ticks it already
//! knows exactly and the rest wrongly, until the server has corrected every
//! tick. Because the merged trace comes from several replays, it is checked by
//! replaying it once more while claiming every tick: a trace counts as
//! `measured` only when every tick is known and those verification replays drew
//! no correction at all. Otherwise it is `unknown`; a tick the replays never
//! revealed is kept as `None`, never filled in.

use std::collections::BTreeMap;

use dataminer_core::provenance::{Method, Provenance};
use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::MeasureError;

/// The spec kind.
pub const KIND: &str = "movement-traces";
/// The payload schema version.
pub const SCHEMA_VERSION: u32 = 3;
/// How far tick 0 may be from the start position, in blocks. Every trace
/// starts from rest, so one tick moves the player far less than this.
pub const MAX_FIRST_TICK_DISTANCE: f64 = 1.0;

/// `PlayerAuthInput` input-flag bits the client set, counted from 0 at the
/// least significant bit, as measured on the wire for protocol 2193.
pub mod bits {
    /// Forward key (sent with move vector y = 1).
    pub const UP: u32 = 10;
    /// Backward key (move vector y = -1).
    pub const DOWN: u32 = 11;
    /// Left strafe key (move vector x = 1).
    pub const LEFT: u32 = 12;
    /// Right strafe key (move vector x = -1).
    pub const RIGHT: u32 = 13;
    /// Held while jumping: jump down, jumping, want up, raw jump current.
    pub const JUMP_HELD: [u32; 4] = [3, 6, 16, 61];
    /// Added on the first tick of a jump press: start jumping, raw jump pressed.
    pub const JUMP_PRESS: [u32; 2] = [31, 60];
    /// Held while sneaking: sneaking, sneak down.
    pub const SNEAK_HELD: [u32; 2] = [8, 9];
}

/// The client's raw input span, as logged.
#[derive(Clone, Debug, Default, Deserialize)]
struct RawSpan {
    from: usize,
    to: usize,
    #[serde(default)]
    forward: f32,
    #[serde(default)]
    strafe: f32,
    #[serde(default)]
    jump: bool,
    #[serde(default)]
    sneak: bool,
    #[serde(default)]
    yaw: Option<f32>,
    #[serde(default)]
    pitch: f32,
    #[serde(default)]
    bits: Option<Vec<u32>>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawCase {
    name: String,
    #[serde(default)]
    setup: Option<Vec<String>>,
    #[serde(default)]
    pass_setup: Option<Vec<String>>,
    start: [f64; 3],
    #[serde(default)]
    yaw: f32,
    ticks: usize,
    #[serde(default)]
    spans: Option<Vec<RawSpan>>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawTick {
    known: bool,
    feet: [f32; 3],
    delta: [f32; 3],
    on_ground: bool,
    server_sprinting: bool,
    #[serde(default)]
    server_swimming: bool,
    #[serde(default)]
    server_gliding: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawTrace {
    case: RawCase,
    trace: Vec<RawTick>,
}

/// Inputs held over ticks `from..to`; outside every span the client sent no
/// movement input at the trace's yaw. Where spans overlap, the later one in
/// the list is what was sent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputSpan {
    /// First tick (inclusive).
    pub from: usize,
    /// Last tick (exclusive).
    pub to: usize,
    /// `[x, y]` move vector: x is strafe (left positive), y is forward.
    pub move_vector: [f32; 2],
    /// Yaw in degrees (0 = +z, −90 = +x).
    pub yaw: f32,
    /// Pitch in degrees, positive looking down.
    pub pitch: f32,
    /// Input-flag bits set on every tick of the span, ascending.
    pub bits: Vec<u32>,
    /// Bits added only on the span's first tick, ascending.
    pub first_tick_bits: Vec<u32>,
}

/// The server's state after one input tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    /// Feet position (the packet's eye position minus 1.62).
    pub feet: [f32; 3],
    /// The packet's `Delta`: the velocity carried into the next tick.
    pub velocity: [f32; 3],
    /// The packet's on-ground flag.
    pub on_ground: bool,
    /// The SPRINTING actor flag (bit 3) the server reported for this tick.
    pub sprinting: bool,
    /// The SWIMMING actor flag (bit 57) the server reported for this tick.
    pub swimming: bool,
    /// The GLIDING actor flag (bit 32) the server reported for this tick.
    /// Written only when set (schema 2). It means something only in a trace
    /// whose `gliding_recorded` is true; elsewhere the flag wasn't logged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gliding: bool,
}

/// One traced run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    /// What the run does.
    pub name: String,
    /// Server console commands run before it (effects, terrain).
    pub setup: Vec<String>,
    /// Commands run again before every replay, for state a run uses up
    /// (food, an elytra's durability). Written only when present (schema 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pass_setup: Vec<String>,
    /// Whether the client logged the GLIDING flag on every tick of this run.
    /// When false, a tick's `gliding` is false only because nothing was
    /// recorded. Written only when true (schema 3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gliding_recorded: bool,
    /// Feet position the player was teleported to before the run.
    pub start: [f64; 3],
    /// The inputs.
    pub inputs: Vec<InputSpan>,
    /// `measured` when every tick is known and every verification replay drew
    /// no correction, else `unknown`.
    pub method: Method,
    /// The verification replays, if any were run.
    pub verification: Option<Verification>,
    /// The server's state after input 0, 1, …; `None` where not revealed.
    pub ticks: Vec<Option<Tick>>,
}

/// Replays of a finished trace that claimed every known tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Verification {
    /// How many replays were run.
    pub replays: usize,
    /// Ticks any replay was corrected at, ascending; empty means the server
    /// agreed with every tick of the trace.
    pub corrected_ticks: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawVerification {
    case: String,
    corrections_at: Vec<Option<Vec<usize>>>,
}

/// The `movement-traces` payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Traces {
    /// The traces, sorted by name.
    pub traces: Vec<Trace>,
}

fn span(raw: &RawSpan, trace_yaw: f32) -> InputSpan {
    let mut held: Vec<u32> = raw.bits.clone().unwrap_or_default();
    let mut first = Vec::new();
    if raw.forward > 0.0 {
        held.push(bits::UP);
    }
    if raw.forward < 0.0 {
        held.push(bits::DOWN);
    }
    if raw.strafe > 0.0 {
        held.push(bits::LEFT);
    }
    if raw.strafe < 0.0 {
        held.push(bits::RIGHT);
    }
    if raw.jump {
        held.extend(bits::JUMP_HELD);
        first.extend(bits::JUMP_PRESS);
    }
    if raw.sneak {
        held.extend(bits::SNEAK_HELD);
    }
    held.sort_unstable();
    held.dedup();
    first.sort_unstable();
    InputSpan {
        from: raw.from,
        to: raw.to,
        move_vector: [raw.strafe, raw.forward],
        yaw: raw.yaw.unwrap_or(trace_yaw),
        pitch: raw.pitch,
        bits: held,
        first_tick_bits: first,
    }
}

fn parse_verification(log: &str) -> Result<BTreeMap<String, Verification>, MeasureError> {
    let mut out = BTreeMap::new();
    for raw in crate::parse_lines::<RawVerification>(log)? {
        let ticks: Vec<usize> = raw
            .corrections_at
            .iter()
            .flatten()
            .flatten()
            .copied()
            .collect();
        // A case verified more than once keeps every replay, so one failed
        // replay can't be hidden by a later clean one.
        let v = out.entry(raw.case).or_insert(Verification {
            replays: 0,
            corrected_ticks: Vec::new(),
        });
        v.replays += raw.corrections_at.len();
        v.corrected_ticks.extend(ticks);
        v.corrected_ticks.sort_unstable();
        v.corrected_ticks.dedup();
    }
    Ok(out)
}

/// Parse the client's trace log (one JSON object per line), and optionally its
/// verification log, into a spec.
pub fn derive(
    log: &str,
    verification: Option<&str>,
    provenance: Provenance,
) -> Result<SpecDocument<Traces>, MeasureError> {
    let verified = verification
        .map(parse_verification)
        .transpose()?
        .unwrap_or_default();
    let mut traces = Vec::new();
    for raw in crate::parse_lines::<RawTrace>(log)? {
        if raw.trace.len() != raw.case.ticks {
            return Err(MeasureError::Evidence(format!(
                "trace {:?} has {} ticks, expected {}",
                raw.case.name,
                raw.trace.len(),
                raw.case.ticks
            )));
        }
        if traces.iter().any(|t: &Trace| t.name == raw.case.name) {
            return Err(MeasureError::Evidence(format!(
                "trace {:?} appears twice in the log",
                raw.case.name
            )));
        }
        for sp in raw.case.spans.iter().flatten() {
            if sp.from >= sp.to || sp.to > raw.case.ticks {
                return Err(MeasureError::Evidence(format!(
                    "trace {:?} has an input span {}..{} outside its {} ticks",
                    raw.case.name, sp.from, sp.to, raw.case.ticks
                )));
            }
        }
        let Some(t0) = raw.trace.first().filter(|t| t.known) else {
            return Err(MeasureError::Evidence(format!(
                "trace {:?} has no known tick 0, so it can't be checked against its start",
                raw.case.name
            )));
        };
        {
            let d = (0..3)
                .map(|a| (f64::from(t0.feet[a]) - raw.case.start[a]).powi(2))
                .sum::<f64>()
                .sqrt();
            if d > MAX_FIRST_TICK_DISTANCE {
                return Err(MeasureError::Evidence(format!(
                    "trace {:?} starts {d:.3} blocks from its start position, so the player was \
                     not where the run assumed (a failed teleport or a dead player)",
                    raw.case.name
                )));
            }
        }
        let logged = raw
            .trace
            .iter()
            .filter(|t| t.server_gliding.is_some())
            .count();
        if logged != 0 && logged != raw.trace.len() {
            return Err(MeasureError::Evidence(format!(
                "trace {:?} logs the gliding flag on {logged} of its {} ticks",
                raw.case.name,
                raw.trace.len()
            )));
        }
        let ticks: Vec<Option<Tick>> = raw
            .trace
            .iter()
            .map(|t| {
                t.known.then_some(Tick {
                    feet: t.feet,
                    velocity: t.delta,
                    on_ground: t.on_ground,
                    sprinting: t.server_sprinting,
                    swimming: t.server_swimming,
                    gliding: t.server_gliding.unwrap_or(false),
                })
            })
            .collect();
        let verification = verified.get(&raw.case.name).cloned();
        let agreed = verification
            .as_ref()
            .is_some_and(|v| v.replays > 0 && v.corrected_ticks.is_empty());
        let method = if agreed && ticks.iter().all(Option::is_some) {
            Method::Measured
        } else {
            Method::Unknown
        };
        traces.push(Trace {
            name: raw.case.name,
            setup: raw.case.setup.unwrap_or_default(),
            pass_setup: raw.case.pass_setup.unwrap_or_default(),
            gliding_recorded: logged != 0,
            start: raw.case.start,
            inputs: raw
                .case
                .spans
                .unwrap_or_default()
                .iter()
                .map(|s| span(s, raw.case.yaw))
                .collect(),
            method,
            verification,
            ticks,
        });
    }
    if traces.is_empty() {
        return Err(MeasureError::Evidence("no traces in the log".into()));
    }
    traces.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        Traces { traces },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dataminer_core::provenance::SourceKind;

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

    const TICK: &str = r#"{"known":true,"feet":[0.5,200.42,-20.5],"delta":[0,0.3332,0],"on_ground":false,"server_sprinting":false}"#;
    const GAP: &str = r#"{"known":false,"feet":[0,0,0],"delta":[0,0,0],"on_ground":false,"server_sprinting":false}"#;

    fn line(ticks: &[&str], n: usize) -> String {
        format!(
            r#"{{"case":{{"name":"jump","setup":["effect @a clear"],"start":[0.5,200,-20.5],"yaw":0,"ticks":{n},"spans":[{{"from":0,"to":1,"jump":true,"forward":1,"bits":[20]}}]}},"passes":6,"trace":[{}]}}"#,
            ticks.join(",")
        )
    }

    const AGREED: &str = r#"{"case":"jump","corrections_at":[null,null]}"#;
    const DISAGREED: &str = r#"{"case":"jump","corrections_at":[null,[1]]}"#;

    #[test]
    fn complete_verified_trace_is_measured_with_wire_bits() {
        let doc = derive(&line(&[TICK, TICK], 2), Some(AGREED), prov()).unwrap();
        let t = &doc.payload.traces[0];
        assert_eq!(t.method, Method::Measured);
        assert_eq!(t.inputs[0].bits, vec![3, 6, 10, 16, 20, 61]);
        assert_eq!(t.inputs[0].first_tick_bits, vec![31, 60]);
        assert_eq!(t.inputs[0].move_vector, [0.0, 1.0]);
        assert_eq!(t.ticks[0].as_ref().unwrap().velocity, [0.0, 0.3332, 0.0]);
        assert_eq!(t.verification.as_ref().unwrap().replays, 2);
    }

    #[test]
    fn a_trace_the_server_disagrees_with_is_unknown() {
        let doc = derive(&line(&[TICK, TICK], 2), Some(DISAGREED), prov()).unwrap();
        let t = &doc.payload.traces[0];
        assert_eq!(t.method, Method::Unknown);
        assert_eq!(t.verification.as_ref().unwrap().corrected_ticks, vec![1]);
    }

    #[test]
    fn an_unverified_trace_is_unknown() {
        let doc = derive(&line(&[TICK, TICK], 2), None, prov()).unwrap();
        assert_eq!(doc.payload.traces[0].method, Method::Unknown);
    }

    #[test]
    fn an_unrevealed_tick_stays_unknown() {
        let doc = derive(&line(&[TICK, GAP], 2), Some(AGREED), prov()).unwrap();
        let t = &doc.payload.traces[0];
        assert_eq!(t.method, Method::Unknown);
        assert!(t.ticks[1].is_none());
    }

    #[test]
    fn rejects_a_trace_that_did_not_start_at_its_start() {
        let elsewhere = TICK.replace("200.42", "63.0");
        assert!(derive(&line(&[&elsewhere, TICK], 2), Some(AGREED), prov()).is_err());
    }

    #[test]
    fn a_failed_replay_is_not_hidden_by_a_later_clean_one() {
        let log = format!("{DISAGREED}\n{AGREED}\n");
        let doc = derive(&line(&[TICK, TICK], 2), Some(&log), prov()).unwrap();
        let t = &doc.payload.traces[0];
        assert_eq!(t.method, Method::Unknown);
        assert_eq!(t.verification.as_ref().unwrap().replays, 4);
    }

    #[test]
    fn rejects_a_trace_name_given_twice() {
        let one = line(&[TICK, TICK], 2);
        assert!(derive(&format!("{one}\n{one}\n"), Some(AGREED), prov()).is_err());
    }

    #[test]
    fn rejects_an_unknown_tick_zero() {
        assert!(derive(&line(&[GAP, TICK], 2), Some(AGREED), prov()).is_err());
    }

    #[test]
    fn rejects_a_span_beyond_the_trace() {
        let long = line(&[TICK, TICK], 2).replace(r#""to":1"#, r#""to":3"#);
        assert!(derive(&long, Some(AGREED), prov()).is_err());
    }

    #[test]
    fn gliding_is_kept_and_only_written_when_set() {
        let with = |v: &str| {
            TICK.replace(
                r#""server_sprinting":false"#,
                &format!(r#""server_sprinting":false,"server_gliding":{v}"#),
            )
        };
        let doc = derive(
            &line(&[&with("false"), &with("true")], 2),
            Some(AGREED),
            prov(),
        )
        .unwrap();
        let t = &doc.payload.traces[0];
        assert!(!t.ticks[0].as_ref().unwrap().gliding);
        assert!(t.ticks[1].as_ref().unwrap().gliding);
        let json = doc.to_pretty_json().unwrap();
        assert_eq!(json.matches("\"gliding\"").count(), 1);
    }

    #[test]
    fn records_whether_the_gliding_flag_was_logged() {
        let off = TICK.replace(
            r#""server_sprinting":false"#,
            r#""server_sprinting":false,"server_gliding":false"#,
        );
        let doc = derive(&line(&[TICK, TICK], 2), Some(AGREED), prov()).unwrap();
        assert!(!doc.payload.traces[0].gliding_recorded);
        assert!(!doc.to_pretty_json().unwrap().contains("gliding_recorded"));
        let doc = derive(&line(&[&off, &off], 2), Some(AGREED), prov()).unwrap();
        assert!(doc.payload.traces[0].gliding_recorded);
        let err = derive(&line(&[TICK, &off], 2), Some(AGREED), prov()).unwrap_err();
        assert!(err.to_string().contains("gliding flag on 1 of its 2 ticks"));
    }

    #[test]
    fn rejects_a_short_trace() {
        assert!(derive(&line(&[TICK], 2), Some(AGREED), prov()).is_err());
    }
}

//! Per-tick movement traces: the server's position and velocity after every
//! input tick of a scripted run.
//!
//! A strict server corrects a wrong claim at most once per six inputs, so a
//! single run only shows every sixth tick. The client therefore replays the
//! same inputs from the same start several times, claiming the ticks it already
//! knows exactly and the rest wrongly, until the server has corrected every
//! tick. This module turns the client's log of such traces into a spec. A tick
//! the replays never revealed is kept as `None` and the trace is marked
//! `unknown`, never filled in.

use dataminer_core::provenance::{Method, Provenance};
use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::MeasureError;

/// The spec kind.
pub const KIND: &str = "movement-traces";
/// The payload schema version.
pub const SCHEMA_VERSION: u32 = 1;

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
}

#[derive(Clone, Debug, Deserialize)]
struct RawTrace {
    case: RawCase,
    trace: Vec<RawTick>,
}

/// Inputs held over ticks `from..to`; outside every span the client sent no
/// movement input at the trace's yaw.
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
}

/// One traced run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    /// What the run does.
    pub name: String,
    /// Server console commands run before it (effects, terrain).
    pub setup: Vec<String>,
    /// Feet position the player was teleported to before the run.
    pub start: [f64; 3],
    /// The inputs.
    pub inputs: Vec<InputSpan>,
    /// `measured` when every tick is known, else `unknown`.
    pub method: Method,
    /// The server's state after input 0, 1, …; `None` where not revealed.
    pub ticks: Vec<Option<Tick>>,
}

/// The `movement-traces` payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Traces {
    /// The traces, in run order.
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

/// Parse the client's trace log (one JSON object per line) into a spec.
pub fn derive(log: &str, provenance: Provenance) -> Result<SpecDocument<Traces>, MeasureError> {
    let mut traces = Vec::new();
    for (i, line) in log.lines().enumerate() {
        if !line.trim_start().starts_with('{') {
            continue;
        }
        let raw: RawTrace = serde_json::from_str(line).map_err(|source| MeasureError::Parse {
            line: i + 1,
            source,
        })?;
        if raw.trace.len() != raw.case.ticks {
            return Err(MeasureError::Evidence(format!(
                "trace {:?} has {} ticks, expected {}",
                raw.case.name,
                raw.trace.len(),
                raw.case.ticks
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
                })
            })
            .collect();
        let method = if ticks.iter().all(Option::is_some) {
            Method::Measured
        } else {
            Method::Unknown
        };
        traces.push(Trace {
            name: raw.case.name,
            setup: raw.case.setup.unwrap_or_default(),
            start: raw.case.start,
            inputs: raw
                .case
                .spans
                .unwrap_or_default()
                .iter()
                .map(|s| span(s, raw.case.yaw))
                .collect(),
            method,
            ticks,
        });
    }
    if traces.is_empty() {
        return Err(MeasureError::Evidence("no traces in the log".into()));
    }
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

    #[test]
    fn complete_trace_is_measured_with_wire_bits() {
        let doc = derive(&line(&[TICK, TICK], 2), prov()).unwrap();
        let t = &doc.payload.traces[0];
        assert_eq!(t.method, Method::Measured);
        assert_eq!(t.inputs[0].bits, vec![3, 6, 10, 16, 20, 61]);
        assert_eq!(t.inputs[0].first_tick_bits, vec![31, 60]);
        assert_eq!(t.inputs[0].move_vector, [0.0, 1.0]);
        assert_eq!(t.ticks[0].as_ref().unwrap().velocity, [0.0, 0.3332, 0.0]);
    }

    #[test]
    fn an_unrevealed_tick_stays_unknown() {
        let doc = derive(&line(&[TICK, GAP], 2), prov()).unwrap();
        let t = &doc.payload.traces[0];
        assert_eq!(t.method, Method::Unknown);
        assert!(t.ticks[1].is_none());
    }

    #[test]
    fn rejects_a_short_trace() {
        assert!(derive(&line(&[TICK], 2), prov()).is_err());
    }
}

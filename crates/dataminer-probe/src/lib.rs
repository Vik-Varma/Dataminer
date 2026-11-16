//! The headless client that measures our own strict Bedrock Dedicated Server.
//!
//! A strict server simulates every `PlayerAuthInput` itself and answers a
//! wrong position claim with a `CorrectPlayerMovePrediction` carrying its own
//! position and velocity for that tick, at most about once per six inputs. The
//! probe replays one scripted case from the same start again and again,
//! claiming the ticks it already knows exactly and the others wrongly, until
//! the server has told it every tick. It then replays the case claiming every
//! tick, and logs which ticks (if any) were still corrected.
//!
//! This module is the part with no network in it: the case format, the input
//! flags for a tick, and the merge of corrections into a trace. The run log it
//! produces is what `dataminer measure traces` reads.

use serde::{Deserialize, Serialize};

/// Eye height above the feet, as the server reads `PlayerAuthInput.position`.
pub const EYE_HEIGHT: f32 = 1.62;

/// Input-flag bits, counted from the least significant, for protocol 2193.
/// The same numbers as `dataminer_measure::traces::bits`.
pub mod bits {
    pub const UP: u32 = 10;
    pub const DOWN: u32 = 11;
    pub const LEFT: u32 = 12;
    pub const RIGHT: u32 = 13;
    pub const JUMP_HELD: [u32; 4] = [3, 6, 16, 61];
    pub const JUMP_PRESS: [u32; 2] = [31, 60];
    pub const SNEAK_HELD: [u32; 2] = [8, 9];
    /// Sent once after every teleport, or the server drops later inputs.
    pub const HANDLED_TELEPORT: u32 = 37;
}

/// Inputs held over ticks `from..to`. `forward` and `strafe` are the move
/// vector exactly as sent (strafe left positive); they are not limited or
/// scaled by the probe.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub from: usize,
    pub to: usize,
    #[serde(default)]
    pub forward: f32,
    #[serde(default)]
    pub strafe: f32,
    #[serde(default)]
    pub jump: bool,
    #[serde(default)]
    pub sneak: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw: Option<f32>,
    #[serde(default)]
    pub pitch: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bits: Option<Vec<u32>>,
}

/// One scripted case. `setup` runs on the server console once before the
/// case, `pass_setup` before every replay; `{name}` in a command is replaced
/// by the probe's player name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_setup: Option<Vec<String>>,
    /// Feet position the player is teleported to before every replay.
    pub start: [f64; 3],
    #[serde(default)]
    pub yaw: f32,
    pub ticks: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spans: Option<Vec<Span>>,
}

/// What the probe sends on one tick of a case.
#[derive(Clone, Debug, PartialEq)]
pub struct TickInput {
    /// `[strafe, forward]`.
    pub move_vector: [f32; 2],
    pub yaw: f32,
    pub pitch: f32,
    /// Flag bits, ascending, without duplicates.
    pub bits: Vec<u32>,
}

impl Case {
    /// The input for `tick`. Where spans overlap, the later one in the list
    /// wins; outside every span the player stands still at the case's yaw.
    pub fn input(&self, tick: usize) -> TickInput {
        let span = self
            .spans
            .iter()
            .flatten()
            .rev()
            .find(|s| s.from <= tick && tick < s.to);
        let Some(s) = span else {
            return TickInput {
                move_vector: [0.0, 0.0],
                yaw: self.yaw,
                pitch: 0.0,
                bits: Vec::new(),
            };
        };
        let mut b: Vec<u32> = s.bits.clone().unwrap_or_default();
        if s.forward > 0.0 {
            b.push(bits::UP);
        }
        if s.forward < 0.0 {
            b.push(bits::DOWN);
        }
        if s.strafe > 0.0 {
            b.push(bits::LEFT);
        }
        if s.strafe < 0.0 {
            b.push(bits::RIGHT);
        }
        if s.jump {
            b.extend(bits::JUMP_HELD);
            if tick == s.from {
                b.extend(bits::JUMP_PRESS);
            }
        }
        if s.sneak {
            b.extend(bits::SNEAK_HELD);
        }
        b.sort_unstable();
        b.dedup();
        TickInput {
            move_vector: [s.strafe, s.forward],
            yaw: s.yaw.unwrap_or(self.yaw),
            pitch: s.pitch,
            bits: b,
        }
    }
}

/// The server's state after one input tick, as a correction reported it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub known: bool,
    pub feet: [f32; 3],
    pub delta: [f32; 3],
    pub on_ground: bool,
    pub server_sprinting: bool,
    pub server_swimming: bool,
}

/// One line of the run log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceLine {
    pub case: Case,
    pub trace: Vec<Tick>,
}

/// One line of the verification log: for each verifying replay, the ticks the
/// server still corrected.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerificationLine {
    pub case: String,
    pub corrections_at: Vec<Option<Vec<usize>>>,
}

/// A correction, placed on the case's own tick numbering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Correction {
    pub tick: usize,
    /// Eye position, as the packet carries it.
    pub position: [f32; 3],
    pub delta: [f32; 3],
    pub on_ground: bool,
    pub sprinting: bool,
    pub swimming: bool,
}

/// Feet from the packet's eye position.
pub fn feet(eye: [f32; 3]) -> [f32; 3] {
    [eye[0], eye[1] - EYE_HEIGHT, eye[2]]
}

/// Merge one replay's corrections into the trace. A tick that two replays
/// reported differently is an error: the replays did not start alike.
pub fn merge(trace: &mut [Tick], corrections: &[Correction]) -> Result<usize, String> {
    let mut new = 0;
    for c in corrections {
        let Some(slot) = trace.get_mut(c.tick) else {
            continue;
        };
        let t = Tick {
            known: true,
            feet: feet(c.position),
            delta: c.delta,
            on_ground: c.on_ground,
            server_sprinting: c.sprinting,
            server_swimming: c.swimming,
        };
        if slot.known {
            if slot.feet != t.feet || slot.delta != t.delta {
                return Err(format!(
                    "tick {} was {:?} {:?} in an earlier replay and {:?} {:?} in this one",
                    c.tick, slot.feet, slot.delta, t.feet, t.delta
                ));
            }
        } else {
            *slot = t;
            new += 1;
        }
    }
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> Case {
        Case {
            name: "c".into(),
            setup: None,
            pass_setup: None,
            start: [0.5, 0.0, 0.5],
            yaw: 10.0,
            ticks: 6,
            spans: Some(vec![
                Span {
                    from: 1,
                    to: 5,
                    forward: 1.0,
                    strafe: 1.0,
                    bits: Some(vec![20]),
                    ..Span::default()
                },
                Span {
                    from: 3,
                    to: 4,
                    forward: -1.0,
                    jump: true,
                    yaw: Some(90.0),
                    ..Span::default()
                },
            ]),
        }
    }

    #[test]
    fn a_tick_outside_every_span_is_idle_at_the_case_yaw() {
        let i = case().input(0);
        assert_eq!((i.move_vector, i.yaw, i.bits.len()), ([0.0, 0.0], 10.0, 0));
    }

    #[test]
    fn a_span_sets_direction_bits_and_keeps_its_own() {
        let i = case().input(1);
        assert_eq!(i.move_vector, [1.0, 1.0]);
        assert_eq!(i.bits, vec![bits::UP, bits::LEFT, 20]);
    }

    #[test]
    fn the_later_span_wins_and_a_jump_is_pressed_on_its_first_tick_only() {
        let i = case().input(3);
        assert_eq!((i.move_vector, i.yaw), ([0.0, -1.0], 90.0));
        assert_eq!(i.bits, vec![3, 6, bits::DOWN, 16, 31, 60, 61]);
        assert_eq!(case().input(4).bits, vec![bits::UP, bits::LEFT, 20]);
    }

    fn correction(tick: usize, x: f32) -> Correction {
        Correction {
            tick,
            position: [x, 1.62, 0.0],
            delta: [0.0; 3],
            on_ground: true,
            sprinting: false,
            swimming: false,
        }
    }

    #[test]
    fn merge_fills_unknown_ticks_and_ignores_ticks_outside_the_case() {
        let mut trace = vec![Tick::default(); 3];
        assert_eq!(
            merge(&mut trace, &[correction(1, 2.0), correction(7, 9.0)]),
            Ok(1)
        );
        assert!(trace[1].known && !trace[0].known);
        assert_eq!(trace[1].feet, [2.0, 0.0, 0.0]);
    }

    #[test]
    fn merge_refuses_a_tick_two_replays_disagree_on() {
        let mut trace = vec![Tick::default(); 3];
        merge(&mut trace, &[correction(1, 2.0)]).unwrap();
        assert_eq!(merge(&mut trace, &[correction(1, 2.0)]), Ok(0));
        assert!(merge(&mut trace, &[correction(1, 2.5)]).is_err());
    }

    #[test]
    fn a_case_round_trips_through_the_run_log_shape() {
        let line = TraceLine {
            case: case(),
            trace: vec![Tick::default(); 6],
        };
        let text = serde_json::to_string(&line).unwrap();
        assert_eq!(serde_json::from_str::<TraceLine>(&text).unwrap(), line);
    }
}

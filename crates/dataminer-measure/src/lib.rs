//! Turn measurements taken on our own Bedrock Dedicated Server into specs.
//!
//! The method (Dataminer's preferred one for physics): run BDS with
//! `server-authoritative-movement-strict=true` and
//! `player-position-acceptance-threshold=0.0001`, drive a headless client
//! against it, and record where the server's `CorrectPlayerMovePrediction`
//! packets put the player. A strict server corrects any claimed position more
//! than 0.0001 blocks from its own simulation, so once the client stops getting
//! corrections its position *is* the server's.
//!
//! This crate reads the client's run log (one JSON object per line) and derives
//! facts from it. It never runs the server itself.

pub mod bamboo;
pub mod breaking;
pub mod facing;
pub mod traces;

use serde::Deserialize;

/// Errors from reading a run log or deriving facts from it.
#[derive(Debug, thiserror::Error)]
pub enum MeasureError {
    /// A line of the run log was not a run record.
    #[error("run log line {line}: {source}")]
    Parse {
        /// 1-based line number.
        line: usize,
        /// The underlying serde error.
        source: serde_json::Error,
    },
    /// The runs don't support the fact being derived, or contradict it.
    #[error("{0}")]
    Evidence(String),
}

/// One scripted run of the headless client: a block was placed at
/// `(bx, by, bz)`, the player started on one side of it and walked (or, for
/// `drop`, fell) until the server stopped moving it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Run {
    /// The block as given to `/setblock`, e.g. `bamboo ["age_bit"=true]`.
    pub block: String,
    /// Block position.
    pub bx: i32,
    /// Block position.
    pub by: i32,
    /// Block position.
    pub bz: i32,
    /// Approach: `+x`, `-x`, `+z`, `-z` (walking along that axis), or `drop`.
    pub dir: String,
    /// The player's feet position where the server stopped it.
    pub feet: [f32; 3],
    /// Corrections received after the player had stopped. Non-zero means the
    /// client and server still disagreed, so the run is not a valid stop.
    #[serde(default)]
    pub late_corrections: u32,
}

/// Parse a JSON-lines run log. Lines that don't start with `{` (the client's
/// own status output) are skipped.
pub fn parse_runs(log: &str) -> Result<Vec<Run>, MeasureError> {
    log.lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with('{'))
        .map(|(i, l)| {
            serde_json::from_str(l).map_err(|source| MeasureError::Parse {
                line: i + 1,
                source,
            })
        })
        .collect()
}

/// The tool identifier stamped into every spec's provenance.
pub const TOOL: &str = concat!("dataminer-measure ", env!("CARGO_PKG_VERSION"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runs_and_skips_status_lines() {
        let log = "{\"block\":\"stone\",\"bx\":3,\"by\":200,\"bz\":3,\"dir\":\"+x\",\
                   \"feet\":[2.7,200.0,3.5],\"extra\":1}\nseen: map[]\n";
        let runs = parse_runs(log).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].dir, "+x");
        assert_eq!(runs[0].late_corrections, 0);
    }

    #[test]
    fn reports_the_bad_line() {
        let err = parse_runs("\n{\"block\":1}\n").unwrap_err();
        assert!(matches!(err, MeasureError::Parse { line: 2, .. }));
    }
}

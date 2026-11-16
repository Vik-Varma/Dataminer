//! `dataminer-probe`: replay scripted cases against our own strict Bedrock
//! Dedicated Server and log what the server said. See the library docs.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use acacia_client::proto::packets::{
    CorrectPlayerMovePrediction, MovePlayer, PlayerAuthInput, PlayerAuthInputInputMode,
    PlayerAuthInputInteractionModel, PlayerAuthInputPlayMode, ServerboundLoadingScreen,
    SetEntityData,
};
use acacia_client::proto::types::{
    InputData, MetadataDictionaryItemValue, MetadataFlags1, Vec2f, Vec3f,
};
use acacia_client::{Client, Event, TransportKind};
use anyhow::{Context, Result, bail};
use clap::Parser;
use dataminer_probe::{
    Case, Correction, EYE_HEIGHT, Tick, TraceLine, VerificationLine, bits, merge,
};

/// `ServerboundLoadingScreen` type "end": the server simulates nobody before it.
const LOADING_SCREEN_END: i32 = 2;
/// A wrong claim is this far from the best guess, well over the 0.0001 threshold.
const WRONG_BY: f32 = 0.05;
/// Idle inputs after the teleport before a replay starts; the last `REST_QUIET` must be uncorrected.
const REST_TICKS: usize = 40;
const REST_QUIET: usize = 15;
/// Idle inputs after a replay, to collect corrections still on their way.
const TAIL_TICKS: usize = 12;

#[derive(Parser)]
#[command(about = "Measure movement on our own strict Bedrock Dedicated Server")]
struct Args {
    /// Server address, e.g. 127.0.0.1:19132.
    #[arg(long)]
    server: String,
    /// File the server reads console commands from (a FIFO feeding its stdin).
    #[arg(long)]
    console: PathBuf,
    /// JSON array of cases.
    #[arg(long)]
    cases: PathBuf,
    /// Run log to append to (read by `dataminer measure traces --runs`).
    #[arg(long)]
    out: PathBuf,
    /// Verification log to append to (`--verify`).
    #[arg(long)]
    verify_out: PathBuf,
    /// Player name.
    #[arg(long, default_value = "Probe")]
    name: String,
    /// Give up on a case after this many replays without learning every tick.
    #[arg(long, default_value_t = 14)]
    max_replays: usize,
    /// Replays that claim every tick, after the trace is complete.
    #[arg(long, default_value_t = 2)]
    verify_replays: usize,
}

struct Probe {
    client: Client,
    runtime_id: u64,
    name: String,
    console: PathBuf,
    tick: u64,
    /// Eye position last claimed or last told by the server.
    eye: [f32; 3],
    flags: i64,
    teleports_to_handle: u32,
    corrections: Vec<(u64, CorrectPlayerMovePrediction, i64)>,
}

fn v3(a: [f32; 3]) -> Vec3f {
    Vec3f {
        x: a[0],
        y: a[1],
        z: a[2],
    }
}

impl Probe {
    fn command(&self, line: &str) -> Result<()> {
        let line = line.replace("{name}", &self.name);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.console)
            .with_context(|| format!("opening {}", self.console.display()))?;
        writeln!(f, "{line}")?;
        Ok(())
    }

    fn handle(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Packet(p) => {
                if let Ok(c) = p.decode::<CorrectPlayerMovePrediction>() {
                    self.eye = [c.position.x, c.position.y, c.position.z];
                    self.corrections.push((c.tick, c, self.flags));
                } else if let Ok(m) = p.decode::<MovePlayer>()
                    && u64::from(m.runtime_id) == self.runtime_id
                {
                    self.eye = [m.position.x, m.position.y, m.position.z];
                    self.teleports_to_handle += 1;
                } else if let Ok(d) = p.decode::<SetEntityData>()
                    && d.runtime_entity_id == self.runtime_id
                {
                    for item in &d.metadata {
                        if let MetadataDictionaryItemValue::Flags(MetadataFlags1(f)) = item.value {
                            self.flags = f;
                        }
                    }
                }
            }
            Event::Violation(_) => {}
            Event::Disconnected(reason) => bail!("disconnected: {reason:?}"),
        }
        Ok(())
    }

    /// Wait one tick (50 ms), handling whatever arrives.
    async fn wait_tick(&mut self) -> Result<()> {
        let end = tokio::time::Instant::now() + Duration::from_millis(50);
        loop {
            match tokio::time::timeout_at(end, self.client.recv()).await {
                Err(_) => return Ok(()),
                Ok(None) => bail!("the connection closed"),
                Ok(Some(event)) => self.handle(event)?,
            }
        }
    }

    /// Send one input claiming `eye`, then wait a tick. Returns the tick number sent.
    async fn step(&mut self, eye: [f32; 3], input: &dataminer_probe::TickInput) -> Result<u64> {
        let mut flags: Vec<InputData> = input
            .bits
            .iter()
            .map(|b| InputData::from_raw(i64::from(*b)))
            .collect();
        if self.teleports_to_handle > 0 {
            self.teleports_to_handle = 0;
            flags.push(InputData::from_raw(i64::from(bits::HANDLED_TELEPORT)));
        }
        let (ys, yc) = input.yaw.to_radians().sin_cos();
        let (ps, pc) = input.pitch.to_radians().sin_cos();
        let mv = Vec2f {
            x: input.move_vector[0],
            z: input.move_vector[1],
        };
        self.tick += 1;
        let packet = PlayerAuthInput {
            pitch: input.pitch,
            yaw: input.yaw,
            position: v3(eye),
            move_vector: mv.clone(),
            head_yaw: input.yaw,
            input_data: flags,
            input_mode: PlayerAuthInputInputMode::Mouse,
            play_mode: PlayerAuthInputPlayMode::Normal,
            interaction_model: PlayerAuthInputInteractionModel::Crosshair,
            interact_rotation: Vec2f {
                x: input.pitch,
                z: input.yaw,
            },
            tick: self.tick,
            delta: v3([0.0; 3]),
            transaction: None,
            item_stack_request: None,
            block_action: None,
            vehicle_rotation: None,
            predicted_vehicle: None,
            analogue_move_vector: mv.clone(),
            camera_orientation: Vec3f {
                x: -ys * pc,
                y: -ps,
                z: yc * pc,
            },
            raw_move_vector: mv,
        };
        if !self.client.send(&packet) {
            bail!("the connection closed");
        }
        self.eye = eye;
        let sent = self.tick;
        self.wait_tick().await?;
        Ok(sent)
    }

    /// Teleport to the case's start and stand until the server stops correcting.
    async fn go_to_start(&mut self, case: &Case) -> Result<()> {
        let [x, y, z] = case.start;
        self.command(&format!("tp {{name}} {x} {y} {z} {} 0", case.yaw))?;
        let idle = dataminer_probe::TickInput {
            move_vector: [0.0, 0.0],
            yaw: case.yaw,
            pitch: 0.0,
            bits: Vec::new(),
        };
        let mut quiet = 0;
        for _ in 0..REST_TICKS * 4 {
            let before = self.corrections.len() + self.teleports_to_handle as usize;
            self.step(self.eye, &idle).await?;
            let after = self.corrections.len() + self.teleports_to_handle as usize;
            quiet = if after == before { quiet + 1 } else { 0 };
            if quiet >= REST_TICKS.max(REST_QUIET) {
                break;
            }
        }
        if quiet < REST_QUIET {
            bail!(
                "case {:?}: the player never came to rest at its start",
                case.name
            );
        }
        let feet = [self.eye[0], self.eye[1] - EYE_HEIGHT, self.eye[2]];
        let off = (0..3)
            .map(|a| (f64::from(feet[a]) - case.start[a]).powi(2))
            .sum::<f64>()
            .sqrt();
        if off > 1.0 {
            bail!(
                "case {:?}: the player rests at {feet:?}, {off:.2} blocks from its start",
                case.name
            );
        }
        self.corrections.clear();
        Ok(())
    }

    /// One replay. Claims the known ticks exactly and the others wrongly; returns the corrections
    /// that fall on the case's ticks.
    async fn replay(&mut self, case: &Case, trace: &[Tick]) -> Result<Vec<Correction>> {
        for line in case.pass_setup.iter().flatten() {
            self.command(line)?;
        }
        self.go_to_start(case).await?;
        let base = self.tick + 1;
        let mut guess = self.eye;
        for i in 0..case.ticks {
            let claim = match trace.get(i).filter(|t| t.known) {
                Some(t) => {
                    guess = [t.feet[0], t.feet[1] + EYE_HEIGHT, t.feet[2]];
                    guess
                }
                None => [guess[0] + WRONG_BY, guess[1], guess[2]],
            };
            self.step(claim, &case.input(i)).await?;
            if let Some((_, c, _)) = self.corrections.last() {
                guess = [c.position.x, c.position.y, c.position.z];
            }
        }
        let idle = dataminer_probe::TickInput {
            move_vector: [0.0, 0.0],
            yaw: case.yaw,
            pitch: 0.0,
            bits: Vec::new(),
        };
        for _ in 0..TAIL_TICKS {
            self.step(self.eye, &idle).await?;
        }
        let out = self
            .corrections
            .drain(..)
            .filter(|(tick, _, _)| *tick >= base && ((*tick - base) as usize) < case.ticks)
            .map(|(tick, c, flags)| Correction {
                tick: (tick - base) as usize,
                position: [c.position.x, c.position.y, c.position.z],
                delta: [c.delta.x, c.delta.y, c.delta.z],
                on_ground: c.on_ground,
                sprinting: flags & MetadataFlags1::SPRINTING.0 != 0,
                swimming: flags & MetadataFlags1::SWIMMING.0 != 0,
            })
            .collect();
        Ok(out)
    }
}

fn append(path: &Path, line: &impl serde::Serialize) -> Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    writeln!(f, "{}", serde_json::to_string(line)?)?;
    Ok(())
}

async fn run(args: Args) -> Result<()> {
    let cases: Vec<Case> = serde_json::from_slice(
        &std::fs::read(&args.cases).with_context(|| format!("reading {}", args.cases.display()))?,
    )
    .context("the cases file is not a JSON array of cases")?;
    let client = Client::builder(&args.server)
        .offline(&args.name)
        .transport(TransportKind::RakNet)
        .connect()
        .await
        .context("joining the server")?;
    let runtime_id = client.runtime_entity_id();
    client.send(&ServerboundLoadingScreen {
        r#type: LOADING_SCREEN_END,
        loading_screen_id: None,
    });
    let mut probe = Probe {
        client,
        runtime_id,
        name: args.name.clone(),
        console: args.console.clone(),
        tick: 3,
        eye: [0.0, 0.0, 0.0],
        flags: 0,
        teleports_to_handle: 0,
        corrections: Vec::new(),
    };
    for case in &cases {
        for line in case.setup.iter().flatten() {
            probe.command(line)?;
        }
        let mut trace = vec![Tick::default(); case.ticks];
        let mut replays = 0;
        while trace.iter().any(|t| !t.known) {
            if replays == args.max_replays {
                break;
            }
            replays += 1;
            let got = probe.replay(case, &trace).await?;
            let new = merge(&mut trace, &got)
                .map_err(|e| anyhow::anyhow!("case {:?}: {e}", case.name))?;
            eprintln!("{}: replay {replays} gave {new} new ticks", case.name);
            if new == 0 && trace.iter().any(|t| !t.known) {
                eprintln!(
                    "{}: a replay taught nothing; giving up on the unknown ticks",
                    case.name
                );
                break;
            }
        }
        let mut corrections_at = Vec::new();
        if trace.iter().all(|t| t.known) {
            for _ in 0..args.verify_replays {
                let got = probe.replay(case, &trace).await?;
                corrections_at.push(Some(got.iter().map(|c| c.tick).collect::<Vec<_>>()));
            }
        }
        let unknown = trace.iter().filter(|t| !t.known).count();
        eprintln!(
            "{}: {} replays, {unknown} unknown ticks, verification {corrections_at:?}",
            case.name, replays
        );
        append(
            &args.out,
            &TraceLine {
                case: case.clone(),
                trace,
            },
        )?;
        append(
            &args.verify_out,
            &VerificationLine {
                case: case.name.clone(),
                corrections_at,
            },
        )?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("dataminer-probe: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match rt.block_on(run(args)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dataminer-probe: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

//! All device and filesystem I/O lives here. No hardware opens in dry-run mode.
use crate::{
    protocol::{self as p, Report},
    state::State,
};
use anyhow::{Context, Result, bail, ensure};
use hidapi::{HidApi, HidDevice};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
pub const SEND_WAIT: Duration = Duration::from_millis(550);
pub const GET_WAIT: Duration = Duration::from_millis(150);
pub const KEYSTROKE_WAIT: Duration = Duration::from_millis(650);
pub const COLOR_START_WAIT: Duration = Duration::from_millis(750);
/// Per-page waits the original passes for per-key colour (0x0C) and mode (0x51) pages.
pub const COLOR_PAGE_WAIT: Duration = Duration::from_millis(150);
pub const MODE_PAGE_WAIT: Duration = Duration::from_millis(100);
pub enum Transport {
    DryRun,
    /// `cfg`: vendor feature port (usage 0xFFFF/0x02). `events`: input report 5 channel (0xFFFF/0x01).
    Hardware { cfg: HidDevice, events: HidDevice },
}
impl Transport {
    pub fn open(dry: bool) -> Result<Self> {
        if dry {
            return Ok(Self::DryRun);
        }
        let api = HidApi::new()?;
        let open = |usage: u16| -> Result<HidDevice> {
            let matches: Vec<_> = api
                .device_list()
                .filter(|d| {
                    d.vendor_id() == 0x342d
                        && d.product_id() == 0xe3de
                        && d.usage_page() == 0xffff
                        && d.usage() == usage
                })
                .collect();
            match matches.len() {
                0 => bail!("keyboard not found (no 342d:e3de 0xffff/{usage:#04x} interface); is it plugged in?"),
                1 => {}
                n => bail!("{n} 342d:e3de 0xffff/{usage:#04x} interfaces found; unplug all but one keyboard"),
            }
            let path = matches[0].path().to_string_lossy().into_owned();
            matches[0].open_device(&api).with_context(|| {
                format!("cannot open {path}; if permission was denied, install the 70-glorious.rules udev rule")
            })
        };
        let cfg = open(0x02)?;
        let events = open(0x01)?;
        events.set_blocking_mode(false)?;
        Ok(Self::Hardware { cfg, events })
    }
    pub fn send(&self, r: &Report) -> Result<()> {
        let mut wire = [0; 65];
        wire[1..].copy_from_slice(r);
        match self {
            Self::DryRun => println!(
                "TX {}",
                wire.iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::Hardware { cfg, .. } => {
                cfg.send_feature_report(&wire)?;
                thread::sleep(match r[0] {
                    19 => KEYSTROKE_WAIT,
                    12 => COLOR_PAGE_WAIT,
                    81 => MODE_PAGE_WAIT,
                    _ => SEND_WAIT,
                });
            }
        }
        Ok(())
    }
    pub fn query(&self, r: &Report) -> Result<Report> {
        self.send(r)?;
        match self {
            Self::DryRun => {
                let mut mock = [0; 64];
                mock[0] = r[0];
                Ok(mock)
            }
            Self::Hardware { cfg, .. } => {
                let mut wire = [0; 65];
                let n = cfg.get_feature_report(&mut wire)?;
                thread::sleep(GET_WAIT);
                ensure!(n == 65, "short feature response: {n}");
                ensure!(
                    wire[1] == r[0],
                    "unexpected response opcode {:02x}",
                    wire[1]
                );
                let mut out = [0; 64];
                out.copy_from_slice(&wire[1..]);
                Ok(out)
            }
        }
    }
    pub fn live(&self) -> Result<Option<f32>> {
        match self {
            Self::DryRun => Ok(None),
            Self::Hardware { events, .. } => latest_travel(|r| Ok(events.read_timeout(r, 0)?)),
        }
    }
}
fn latest_travel(mut read: impl FnMut(&mut [u8]) -> Result<usize>) -> Result<Option<f32>> {
    let mut r = [0; 65];
    let mut latest = None;
    // Catch up to current travel without letting continuous input starve commands.
    for _ in 0..256 {
        let n = read(&mut r)?;
        if n == 0 {
            break;
        }
        if let Some(value) = p::parse_visualization(&r[..n]) {
            latest = Some(value);
        }
    }
    Ok(latest)
}
pub fn state_path() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("cannot determine configuration directory")?
        .join("glorious-core-rs/state.json"))
}
pub fn load_state() -> Result<State> {
    load_state_from(&state_path()?)
}
pub fn load_state_from(path: &std::path::Path) -> Result<State> {
    let s: State = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid state {} (left untouched)", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
        Err(e) => return Err(e.into()),
    };
    s.validate()?;
    Ok(s)
}
fn save_state(s: &State) -> Result<()> {
    save_state_to(s, &state_path()?)
}
pub fn save_state_to(s: &State, path: &std::path::Path) -> Result<()> {
    s.validate()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    if path.exists() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        std::fs::copy(path, path.with_extension(format!("json.{stamp}.bak")))?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(s)?)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
/// Native (GTK) file chooser via zenity. Blocks, so call it off the UI thread.
/// `Ok(None)` when the dialog was cancelled.
pub fn pick_file(save: bool) -> Result<Option<PathBuf>> {
    let mut c = std::process::Command::new("zenity");
    c.args(["--file-selection", "--filename=inglorious-core.json"]);
    if save {
        c.args(["--save", "--confirm-overwrite"]);
    }
    let out = c.output().context("file dialogs need zenity (pacman -S zenity)")?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((out.status.success() && !path.is_empty()).then(|| path.into()))
}
#[derive(Debug)]
pub enum Command {
    Shutdown,
    Info,
    Write {
        reports: Vec<Report>,
        state: Option<Box<State>>,
    },
    Live(bool),
}
#[derive(Debug)]
pub enum Event {
    Status(String),
    Progress(usize, usize),
    Info {
        fw: String,
        profile: u8,
        layer: u8,
        sleep: Option<u16>,
        /// hidraw nodes of the feature and event ports.
        ports: [String; 2],
    },
    Saved(Box<State>),
    Live(f32),
    Done,
    Error(String),
}
pub fn info(t: &Transport) -> Result<Event> {
    let mut fw = Vec::new();
    for op in [0xf0, 0x80, 0xae] {
        fw.push(p::parse_fw(&t.query(&p::get_fw(op)?)?)?);
    }
    let (profile, layer) = p::parse_profile(&t.query(&p::get_profile())?)?;
    let sleep = p::parse_sleep(&t.query(&p::get_sleep())?)?;
    let ports = match t {
        Transport::DryRun => ["dry-run".into(), "dry-run".into()],
        Transport::Hardware { cfg, events } => [cfg, events].map(|d| {
            d.get_device_info()
                .map(|i| i.path().to_string_lossy().trim_start_matches("/dev/").into())
                .unwrap_or_else(|_| "?".into())
        }),
    };
    Ok(Event::Info {
        fw: fw.join(" / "),
        profile,
        layer,
        sleep,
        ports,
    })
}
pub fn worker(dry: bool) -> (Sender<Command>, Receiver<Event>, thread::JoinHandle<()>) {
    worker_with_open(dry, move || Transport::open(dry))
}
fn worker_with_open(
    dry: bool,
    mut open: impl FnMut() -> Result<Transport> + Send + 'static,
) -> (Sender<Command>, Receiver<Event>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let (et, er) = mpsc::channel();
    let join = thread::spawn(move || {
        let mut transport: Option<Transport> = None;
        let mut live = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(cmd) => {
                    if matches!(cmd, Command::Shutdown) {
                        if live && let Some(t) = transport.as_ref() {
                            let _ = t.send(&p::visualization(false));
                        }
                        break;
                    }
                    let writing = matches!(cmd, Command::Write { .. });
                    let result = (|| -> Result<()> {
                        if transport.is_none() {
                            transport = Some(open()?);
                        }
                        let t = transport.as_ref().unwrap();
                        match cmd {
                            Command::Shutdown => unreachable!("handled before opening hardware"),
                            Command::Info => {
                                let _ = et.send(info(t)?);
                            }
                            Command::Live(enabled) => {
                                if enabled {
                                    t.live()?; // Discard packets left over from a previous live session.
                                }
                                t.send(&p::visualization(enabled))?;
                                live = enabled;
                            }
                            Command::Write { mut reports, state } => {
                                ensure!(!reports.is_empty(), "empty write");
                                if let Some(s) = state.as_ref() {
                                    s.validate()?;
                                    // Un-indexed lighting/performance/mode writes must target the selected profile.
                                    if reports[0][0] != 5 {
                                        reports.insert(0, p::set_profile(s.profile, s.layer)?);
                                    }
                                }
                                if reports.iter().any(|r| r[0] == 12) && !dry {
                                    thread::sleep(COLOR_START_WAIT);
                                }
                                let n = reports.len();
                                for (i, r) in reports.iter().enumerate() {
                                    t.send(r)?;
                                    let _ = et.send(Event::Progress(i + 1, n));
                                }
                                if let Some(s) = state {
                                    if !dry {
                                        save_state(&s)?;
                                    }
                                    let _ = et.send(Event::Saved(s));
                                }
                            }
                        }
                        Ok(())
                    })();
                    match result {
                        Ok(()) => {
                            let _ = et.send(Event::Done);
                        }
                        Err(e) => {
                            // Reopen on the next explicit action; never replay a failed batch.
                            transport = None;
                            live = false;
                            let note = if writing {
                                ". A partial batch may have reached the board; saved state was not advanced."
                            } else {
                                ""
                            };
                            let _ = et.send(Event::Error(format!("{e:#}{note}")));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if live && let Some(t) = transport.as_ref() {
                        let _ = t.send(&p::visualization(false));
                    }
                    break;
                }
            }
            if live && let Some(t) = transport.as_ref() {
                match t.live() {
                    Ok(Some(v)) => {
                        let _ = et.send(Event::Live(v));
                    }
                    Ok(None) => {}
                    Err(e) => {
                        transport = None;
                        live = false;
                        let _ = et.send(Event::Error(format!("Live read stopped: {e}")));
                    }
                }
            }
        }
    });
    (tx, er, join)
}
pub fn cli(dry: bool, args: &[String]) -> Result<()> {
    let t = Transport::open(dry)?;
    match args.first().map(String::as_str) {
        Some("info") if args.len() == 1 => println!("{:?}", info(&t)?),
        Some("profile") if args.len() == 2 => {
            let n: u8 = args[1].parse()?;
            ensure!((1..=3).contains(&n), "profile must be 1..3");
            let mut s = load_state()?;
            s.profile = n - 1;
            s.layer = 0;
            t.send(&p::set_profile(s.profile, s.layer)?)?;
            if !dry {
                save_state(&s)?;
            }
        }
        Some("effect") if (2..=5).contains(&args.len()) => {
            let mut s = load_state()?;
            let (profile, layer) = p::parse_profile(&t.query(&p::get_profile())?)?;
            s.profile = profile;
            s.layer = layer;
            let a = &mut s.current_mut().lighting;
            a.effect = args[1].parse()?;
            if let Some(v) = args.get(2) {
                a.speed = v.parse()?;
            }
            if let Some(v) = args.get(3) {
                a.brightness = v.parse()?;
            }
            if let Some(v) = args.get(4) {
                ensure!(v.len() == 6 && v.is_ascii(), "RGB must be six hex digits");
                a.rgb = [
                    u8::from_str_radix(&v[0..2], 16)?,
                    u8::from_str_radix(&v[2..4], 16)?,
                    u8::from_str_radix(&v[4..6], 16)?,
                ];
            }
            t.send(&p::lighting(a, false)?)?;
            if !dry {
                save_state(&s)?;
            }
        }
        Some("perkey") if args.len() >= 2 => {
            let mut s = load_state()?;
            let (profile, layer) = p::parse_profile(&t.query(&p::get_profile())?)?;
            s.profile = profile;
            s.layer = layer;
            let colors = &mut s.current_mut().colors;
            colors.fill(p::KeyLight::default());
            for spec in &args[1..] {
                let (key, hex) = spec.split_once('=').context("expected KEY=RRGGBB")?;
                let idx = crate::tables::PHYSICAL_KEYS
                    .iter()
                    .find(|(n, _)| *n == key)
                    .with_context(|| format!("unknown key {key}"))?
                    .1;
                ensure!(hex.len() == 6 && hex.is_ascii(), "RGB must be six hex digits");
                let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
                colors[idx as usize] = p::KeyLight {
                    rgb: [byte(0)?, byte(2)?, byte(4)?],
                    mode: 1,
                };
            }
            if !dry {
                thread::sleep(COLOR_START_WAIT);
            }
            for r in p::per_key_lighting(colors)? {
                t.send(&r)?;
            }
            if !dry {
                save_state(&s)?;
            }
        }
        Some("actuation") if matches!(args.len(), 2 | 4) => {
            let mut s = load_state()?;
            let (profile, layer) = p::parse_profile(&t.query(&p::get_profile())?)?;
            s.profile = profile;
            s.layer = layer;
            let a = &mut s.current_mut().actuation;
            a.press = args[1].parse()?;
            a.rapid = args.len() == 4;
            if a.rapid {
                a.rapid_press = args[2].parse()?;
                a.rapid_release = args[3].parse()?;
            }
            for r in crate::state::actuation_reports(&s)? {
                t.send(&r)?;
            }
            if !dry {
                save_state(&s)?;
            }
        }
        _ => bail!(
            "usage: inglorious-core [--dry-run] [info | profile <1..3> | effect <0..18> [speed%] [brightness%] [RRGGBB] | perkey KEY=RRGGBB... | actuation <mm> [rt_press_mm rt_release_mm]]"
        ),
    };
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_read_uses_newest_sample_and_skips_other_reports() {
        use std::collections::VecDeque;
        let mut reports = VecDeque::from([
            vec![5, 27, 0], vec![5, 27, 40], vec![5, 1, 2, 0],
            vec![5, 27, 0], vec![5, 27, 30], vec![5, 208, 9],
        ]);
        let read = |out: &mut [u8]| -> Result<usize> {
            let Some(r) = reports.pop_front() else { return Ok(0); };
            out[..r.len()].copy_from_slice(&r);
            Ok(r.len())
        };
        assert_eq!(latest_travel(read).unwrap(), Some(0.75));
        assert!(reports.is_empty());
        assert_eq!(latest_travel(|_| Ok(0)).unwrap(), None);
    }
    #[test]
    fn live_read_is_bounded_and_keeps_read_errors() {
        let mut reads = 0;
        let value = latest_travel(|out| {
            reads += 1;
            out[..3].copy_from_slice(&[5, 27, 20]);
            Ok(3)
        }).unwrap();
        assert_eq!(value, Some(0.5));
        assert_eq!(reads, 256, "continuous input must not starve worker commands");
        assert!(latest_travel(|_| anyhow::bail!("device disconnected")).is_err());
    }
    #[test]
    fn worker_reopens_after_error_without_replaying_write() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        let opens = Arc::new(AtomicUsize::new(0));
        let count = opens.clone();
        let (tx, rx, join) = worker_with_open(true, move || {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(Transport::DryRun)
        });
        tx.send(Command::Write { reports: vec![], state: None }).unwrap();
        assert!(matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Event::Error(_)));
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err(), "no automatic retry");
        tx.send(Command::Info).unwrap();
        assert!(matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Event::Info { .. }));
        assert!(matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Event::Done));
        let reopened = opens.load(Ordering::SeqCst);
        tx.send(Command::Shutdown).unwrap();
        join.join().unwrap();
        assert_eq!(reopened, 2, "next action must reopen cached handles");
    }
    #[test]
    fn persistence_backup_and_corruption() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "state-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let path = root.join("state.json");
        assert_eq!(load_state_from(&path).unwrap().profile, 0);
        let mut s = State::default();
        save_state_to(&s, &path).unwrap();
        let original = std::fs::read(&path).unwrap();
        s.profile = 2;
        save_state_to(&s, &path).unwrap();
        assert_eq!(load_state_from(&path).unwrap().profile, 2);
        let backup = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "bak"))
            .unwrap();
        assert_eq!(std::fs::read(backup).unwrap(), original);
        let good = std::fs::read(&path).unwrap();
        s.layer = 3;
        assert!(save_state_to(&s, &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), good);
        std::fs::write(&path, b"{broken").unwrap();
        assert!(load_state_from(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        std::fs::remove_dir_all(root).unwrap();
    }
}

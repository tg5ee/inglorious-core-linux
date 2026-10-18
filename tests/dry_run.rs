use std::{
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
fn run(args: &[&str], env_dry: bool) -> std::process::Output {
    let config = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "dry-run-config-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_inglorious-core"))
        .args(args)
        .env("GLORIOUS_CORE_DRY_RUN", if env_dry { "1" } else { "0" })
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(!config.exists(), "dry-run must not create persistent state");
    output
}
#[test]
fn info_logs_five_65_byte_reports() {
    let o = run(&["--dry-run", "info"], false);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8(o.stdout).unwrap();
    let reports: Vec<_> = text.lines().filter(|l| l.starts_with("TX ")).collect();
    assert_eq!(reports.len(), 5);
    for r in reports {
        assert_eq!(r.split_whitespace().count(), 66);
        assert!(r.starts_with("TX 00 "));
    }
    assert!(text.contains("KBv0 / RFv0 / LEDv0"));
}
#[test]
fn environment_dry_run_and_profile_sequence() {
    let o = run(&["profile", "3"], true);
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    let opcodes: Vec<_> = text
        .lines()
        .filter(|l| l.starts_with("TX"))
        .map(|l| l.split_whitespace().nth(2).unwrap())
        .collect();
    assert_eq!(opcodes, ["05"]);
}
#[test]
fn effect_and_invalid_input() {
    let o = run(&["--dry-run", "effect", "6", "50", "50", "aabbcc"], false);
    assert!(o.status.success());
    assert!(
        String::from_utf8(o.stdout)
            .unwrap()
            .contains("TX 00 07 07 0a 0a 09 aa bb cc")
    );
    for args in [
        &["--dry-run", "profile", "0"][..],
        &["--dry-run", "effect", "19"][..],
        &["--dry-run", "effect", "6", "101"][..],
        &["--dry-run", "effect", "6", "50", "50", "é1234"][..],
    ] {
        let o = run(args, false);
        assert!(!o.status.success());
        assert!(!String::from_utf8_lossy(&o.stdout).contains("TX 00 07"));
    }
}
#[test]
fn dry_worker_progress_and_state() {
    use glorious_core_rs::{
        device::{Command as Work, Event, worker},
        protocol,
        state::State,
    };
    let (tx, rx, join) = worker(true);
    tx.send(Work::Write {
        reports: vec![protocol::set_sleep(60)],
        state: Some(Box::new(State::default())),
    })
    .unwrap();
    let mut progress = Vec::new();
    let mut saved = false;
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap() {
            Event::Progress(i, n) => progress.push((i, n)),
            Event::Saved(_) => saved = true,
            Event::Done => break,
            Event::Error(e) => panic!("{e}"),
            _ => {}
        }
    }
    assert_eq!(progress, [(1, 2), (2, 2)]);
    assert!(saved);
    tx.send(Work::Shutdown).unwrap();
    join.join().unwrap();
}

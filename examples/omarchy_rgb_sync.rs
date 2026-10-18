//! Omarchy RGB Sync (prototype, kept out of the GUI and the packaged binary).
//!
//! Puts the active Omarchy theme's `accent` into the keyboard's own base effect: one 0x07 preset
//! write per theme switch, through the same packet the GUI's Apply sends. The board runs the
//! animation itself; nothing is streamed. Effects with firmware colours are left alone.
//!
//!   omarchy-rgb-sync enable    install the theme-set hook and apply the current accent
//!   omarchy-rgb-sync apply     what the hook runs on every theme switch
//!   omarchy-rgb-sync disable   remove the hook and restore the lighting saved by the app
//!
//! It never writes state.json, so `disable` (or a sidebar slot click in the GUI) restores the app's
//! saved lighting. GLORIOUS_CORE_DRY_RUN=1 skips the hardware.
use anyhow::{Context, Result, bail};
use glorious_core_rs::{
    device::{self, Transport},
    protocol::{self as p, Lighting},
    tables::{MULTICOLOUR_ONLY, SINGLE_COLOUR},
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

/// Hook body; `{exe}` is this binary's path at `enable` time, so PATH in the hook doesn't matter.
const HOOK: &str = "#!/bin/bash
# Inglorious Core: Omarchy RGB Sync prototype. `omarchy-rgb-sync disable` removes this file.
# Backgrounded so the theme switch never waits on the keyboard.
[ -x '{exe}' ] && setsid -f '{exe}' apply >/dev/null 2>&1 </dev/null
exit 0
";

fn hook_path() -> Result<PathBuf> {
    Ok(dirs::config_dir().context("no config dir")?.join("omarchy/hooks/theme-set.d/inglorious-rgb-sync"))
}

/// `accent = "#rrggbb"` from the active theme's colors.toml (Omarchy v4 flat format).
fn accent(colors: &str) -> Option<[u8; 3]> {
    let v = colors.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "accent").then(|| v.trim().trim_matches('"').trim_start_matches('#').to_owned())
    })?;
    if v.len() != 6 || !v.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&v[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// The saved preset recoloured to `rgb` in single-colour mode, or None when the firmware owns its colours.
fn themed(l: &Lighting, rgb: [u8; 3]) -> Option<Lighting> {
    (!MULTICOLOUR_ONLY.contains(&l.effect)).then(|| Lighting { color_mode: SINGLE_COLOUR, rgb, ..l.clone() })
}

/// App state with the slot the board is actually on (lighting writes are unindexed).
fn open(dry: bool) -> Result<(Transport, glorious_core_rs::state::State)> {
    let t = Transport::open(dry)?;
    let mut s = device::load_state()?;
    if !dry {
        (s.profile, s.layer) = p::parse_profile(&t.query(&p::get_profile())?)?;
    }
    Ok((t, s))
}

fn apply(dry: bool) -> Result<()> {
    let path = dirs::state_dir().context("no state dir")?.join("omarchy/current/theme/colors.toml");
    let rgb = accent(&fs::read_to_string(&path).with_context(|| path.display().to_string())?)
        .with_context(|| format!("no accent in {}", path.display()))?;
    // Theme switches can come in bursts; one keyboard writer at a time, each using the newest theme.
    let lock = fs::File::create(std::env::temp_dir().join("inglorious-rgb-sync.lock"))?;
    lock.lock()?;
    let (t, s) = open(dry)?;
    let l = s.lighting_state();
    let Some(keys) = themed(&l.lighting, rgb) else {
        println!("effect {} uses firmware colours; left unchanged", l.lighting.effect);
        return Ok(());
    };
    t.send(&p::lighting(&keys, false)?)?;
    // Side lights follow only when the app has them following the keyboard.
    if l.side_lighting == l.lighting {
        t.send(&p::lighting(&keys, true)?)?;
    }
    println!("accent {rgb:02x?} -> effect {}", keys.effect);
    Ok(())
}

fn main() -> Result<()> {
    let dry = std::env::var("GLORIOUS_CORE_DRY_RUN").is_ok_and(|v| v == "1" || v == "true");
    match std::env::args().nth(1).as_deref() {
        Some("apply") => apply(dry),
        Some("enable") => {
            let h = hook_path()?;
            let exe = std::env::current_exe()?;
            fs::write(&h, HOOK.replace("{exe}", &exe.to_string_lossy()))?;
            fs::set_permissions(&h, fs::Permissions::from_mode(0o755))?;
            println!("hook installed: {}", h.display());
            apply(dry)
        }
        Some("disable") => {
            match fs::remove_file(hook_path()?) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
            let (t, s) = open(dry)?;
            let l = s.lighting_state();
            t.send(&p::lighting(&l.lighting, false)?)?;
            t.send(&p::lighting(&l.side_lighting, true)?)?;
            println!("hook removed; saved lighting restored");
            Ok(())
        }
        _ => bail!("omarchy-rgb-sync enable | apply | disable"),
    }
}

#[test]
fn accent_and_fallback() {
    let colors = "mode = \"dark\"\naccent = \"#458f82\"\nmuted = \"#5b5f5a\"\n";
    assert_eq!(accent(colors), Some([0x45, 0x8f, 0x82]));
    assert_eq!(accent("accent = \"#45\""), None);
    let wave = Lighting { effect: 1, color_mode: 8, ..Lighting::default() };
    let t = themed(&wave, [1, 2, 3]).unwrap();
    assert_eq!((t.effect, t.color_mode, t.rgb, t.speed), (1, SINGLE_COLOUR, [1, 2, 3], wave.speed));
    assert!(themed(&Lighting { effect: 0, ..Lighting::default() }, [1, 2, 3]).is_none());
}

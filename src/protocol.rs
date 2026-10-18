//! Magneton command bodies, excluding the HID report-ID prefix. No I/O.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
pub type Report = [u8; 64];
pub fn checksum(bytes: &[u8]) -> u8 {
    255u8.wrapping_sub(bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)))
}
fn header(bytes: &[u8]) -> Report {
    let mut r = [0; 64];
    r[..bytes.len()].copy_from_slice(bytes);
    r[7] = checksum(&r[..7]);
    r
}
fn packed(p: u8, l: u8) -> Result<u8> {
    ensure!(p < 3 && l < 3, "profile and layer must be 0..2");
    Ok(l << 4 | p)
}
pub fn get_fw(command: u8) -> Result<Report> {
    ensure!(
        [0xf0, 0x80, 0xae].contains(&command),
        "invalid firmware request"
    );
    Ok(header(&[command]))
}
pub fn parse_fw(r: &[u8]) -> Result<String> {
    ensure!(r.len() >= 3, "short firmware response");
    let prefix = match r[0] {
        0xf0 => "KBv",
        0x80 => "RFv",
        0xae => "LEDv",
        _ => bail!("unexpected firmware response"),
    };
    Ok(format!("{prefix}{:x}", u16::from_le_bytes([r[1], r[2]])))
}
pub fn get_profile() -> Report {
    // JS sums the leading zero plus command bytes 0..6; all omitted bytes are zero.
    header(&[0x85])
}
pub fn parse_profile(r: &[u8]) -> Result<(u8, u8)> {
    ensure!(r.len() >= 3 && r[0] == 0x85, "invalid profile response");
    packed(r[1], r[2])?;
    Ok((r[1], r[2]))
}
pub fn set_profile(p: u8, l: u8) -> Result<Report> {
    packed(p, l)?;
    Ok(header(&[5, p, l]))
}
pub fn get_sleep() -> Report {
    header(&[0x92])
}
pub fn set_sleep(seconds: u16) -> Report {
    let mut r = header(&[0x12]);
    r[16..18].copy_from_slice(&seconds.to_le_bytes());
    r[18..20].copy_from_slice(&[255, 255]);
    r
}
pub fn parse_sleep(r: &[u8]) -> Result<Option<u16>> {
    ensure!(r.len() >= 13 && r[0] == 0x92, "invalid sleep response");
    let s = u16::from_le_bytes([r[9], r[10]]);
    Ok(if s == 0 || s == 65535 { None } else { Some(s) })
}
pub fn report_rate(p: u8, hz: u16) -> Result<Report> {
    packed(p, 0)?;
    let rate = match hz {
        8000 => 0,
        4000 => 1,
        2000 => 2,
        1000 => 3,
        500 => 4,
        250 => 5,
        125 => 6,
        _ => bail!("unsupported polling rate"),
    };
    Ok(header(&[4, p, rate]))
}
pub fn debounce(ms: u16) -> Result<Report> {
    ensure!(ms <= 1275, "debounce exceeds firmware range");
    Ok(header(&[0x11, 0, ((ms as f32 / 5.).round()) as u8]))
}
pub fn reset() -> Report {
    header(&[2])
}
pub fn calibration(stage: u8) -> Result<Report> {
    ensure!((1..=4).contains(&stage), "calibration stage must be 1..4");
    Ok(header(&[
        if stage <= 2 { 0x1c } else { 0x1e },
        u8::from(stage == 1 || stage == 3),
    ]))
}
pub fn visualization(enable: bool) -> Report {
    header(&[0x1b, u8::from(enable)])
}
pub fn parse_visualization(r: &[u8]) -> Option<f32> {
    (r.len() >= 3 && r[0] == 5 && r[1] == 27).then(|| (r[2] as f32 / 40.).min(1.))
}
pub fn custom_actuation() -> Report {
    header(&[0x1d, 3])
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Actuation {
    pub press: f32,
    pub rapid: bool,
    pub rapid_press: f32,
    pub rapid_release: f32,
    pub deadzone: Option<f32>,
}
impl Default for Actuation {
    fn default() -> Self {
        // DEFAULT_ACTUATION_* / DEFAULT_RAPID_TRIGGER_* / DEFAULT_ACTUATION_DEAD_ZONE_VALUE in index.js.
        Self {
            press: 2.,
            rapid: false,
            rapid_press: 0.2,
            rapid_release: 0.3,
            deadzone: Some(3.7),
        }
    }
}
fn travel(v: f32) -> Result<u8> {
    ensure!(v.is_finite() && (0.0..=50.).contains(&v), "invalid travel");
    Ok(((v * 10.).round() as i32 - 1).clamp(1, 40) as u8)
}
pub fn actuation(p: u8, l: u8, key: Option<u8>, a: &Actuation) -> Result<Report> {
    let t = travel(a.press)?;
    let mut r = header(&[
        26,
        packed(p, l)?,
        0,
        if a.rapid { 128 } else { 0 },
        t,
        u8::from(key.is_none()),
        key.unwrap_or(0),
    ]);
    r[8] = t;
    if a.rapid {
        r[9] = travel(a.rapid_press)?;
        r[10] = travel(a.rapid_release)?;
    }
    r[25] = a.deadzone.map(travel).transpose()?.unwrap_or(40);
    Ok(r)
}
pub fn keystroke(key: u8, p: u8, l: u8, slot: u8, code: [u8; 4]) -> Result<Report> {
    packed(p, l)?;
    ensure!(slot < 4, "slot must be 0..3");
    let mut r = header(&[19, p, key, l, slot]);
    r[8..12].copy_from_slice(&code);
    Ok(r)
}
pub fn keystroke_clear(p: u8, l: u8, key: u8) -> Result<Report> {
    Ok(header(&[26, packed(p, l)?, 0, 0, 0, 0, key]))
}
pub fn complex_keystroke(p: u8, l: u8, key: u8, codes: [[u8; 4]; 4]) -> Result<Vec<Report>> {
    codes
        .into_iter()
        .enumerate()
        .map(|(i, c)| keystroke(key, p, l, i as u8, c))
        .collect()
}
pub fn toggle(p: u8, l: u8, key: u8, t: u8) -> Result<Report> {
    ensure!(t <= 1, "toggle type must be 0 or 1");
    Ok(header(&[26, packed(p, l)?, 0, 4 + t, 0, 0, key]))
}
pub fn mod_tap(p: u8, l: u8, key: u8, timeout: u16) -> Result<Report> {
    let mut r = header(&[26, packed(p, l)?, 0, 3, 0, 0, key]);
    r[16] = (timeout.clamp(10, 1000) as f32 / 10.).ceil() as u8;
    Ok(r)
}
/// Modes correspond to first press, second press, second release, first release: 0 absent, 1 single, 2 continuous.
pub fn dynamic_keystroke(
    p: u8,
    l: u8,
    key: u8,
    first: Option<f32>,
    second: Option<f32>,
    modes: [u8; 4],
) -> Result<Report> {
    ensure!(modes.iter().all(|m| *m <= 2), "invalid DKS modes");
    let mut r = header(&[26, packed(p, l)?, 0, 2, 0, 0, key]);
    let v = first
        .filter(|v| *v != 0.)
        .or(second.filter(|v| *v != 0.))
        .unwrap_or(4.);
    r[11] = travel(v)?.clamp(4, 24);
    for (i, m) in modes.iter().enumerate() {
        r[12 + i] = m << (i * 2);
    }
    Ok(r)
}
pub fn map_effect(value: u8) -> u8 {
    [
        1, 2, 6, 3, 4, 5, 7, 8, 0, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
    ]
    .get(value as usize)
    .copied()
    .unwrap_or(0)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lighting {
    pub effect: u8,
    pub speed: u8,
    pub brightness: u8,
    pub color_mode: u8,
    pub kind: u8,
    pub rgb: [u8; 3],
}
impl Default for Lighting {
    fn default() -> Self {
        Self {
            effect: 6,
            speed: 50,
            brightness: 50,
            color_mode: 9,
            kind: 0,
            rgb: [180, 130, 40],
        }
    }
}
pub fn lighting(a: &Lighting, side: bool) -> Result<Report> {
    ensure!(
        a.effect <= 18
            && a.speed <= 100
            && a.brightness <= 100
            && a.color_mode <= 9
            && a.kind <= 15,
        "invalid lighting settings"
    );
    let mut r = [0; 64];
    r[..8].copy_from_slice(&[
        if side { 8 } else { 7 },
        map_effect(a.effect),
        20 - (a.speed as u16 * 20).div_ceil(100) as u8,
        (a.brightness as u16 * 20).div_ceil(100) as u8,
        a.color_mode | (a.kind << 4),
        a.rgb[0],
        a.rgb[1],
        a.rgb[2],
    ]);
    r[8] = checksum(&r[..8]);
    Ok(r)
}
pub fn per_key_color(page: u8, data: &[u8]) -> Result<Report> {
    ensure!(page < 9 && data.len() <= 56, "invalid color page");
    let mut r = header(&[12, 0, 128, 1, page]);
    r[8..8 + data.len()].copy_from_slice(data);
    Ok(r)
}
pub fn per_key_mode(page: u8, data: &[u8]) -> Result<Report> {
    ensure!(
        page < 3 && data.len() <= 56 && data.iter().all(|v| *v <= 2),
        "invalid mode page"
    );
    let mut r = header(&[81, page]);
    r[8..8 + data.len()].copy_from_slice(data);
    Ok(r)
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KeyLight {
    pub rgb: [u8; 3],
    pub mode: u8,
}
pub fn per_key_lighting(keys: &[KeyLight]) -> Result<Vec<Report>> {
    ensure!(keys.len() == 168, "expected 168 hardware colors");
    let colors: Vec<u8> = keys.iter().flat_map(|k| k.rgb).collect();
    let modes: Vec<u8> = keys.iter().map(|k| k.mode).collect();
    let mut out = Vec::new();
    for (i, c) in colors.chunks(56).enumerate() {
        out.push(per_key_color(i as u8, c)?);
    }
    for (i, m) in modes.chunks(56).enumerate() {
        out.push(per_key_mode(i as u8, m)?);
    }
    Ok(out)
}
pub fn macro_page(id: u8, page: u8, last: bool, data: &[u8]) -> Result<Report> {
    ensure!(page < 5 && data.len() <= 56, "invalid macro page");
    let mut r = header(&[22, id, page, 56, u8::from(last)]);
    r[8..8 + data.len()].copy_from_slice(data);
    Ok(r)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MacroEvent {
    pub key: u8,
    pub press: bool,
    pub delay: u16,
}
pub fn macro_reports(id: u8, repeat: u8, events: &[MacroEvent]) -> Result<Vec<Report>> {
    let mut s = vec![repeat, 0];
    for e in events {
        ensure!(e.delay > 0, "macro delay must be positive");
        s.push(e.key);
        if e.delay <= 127 {
            s.push(if e.press { 128 } else { 0 } | e.delay as u8);
        } else {
            // Original uses >>4, not >>8; preserve firmware encoding.
            s.extend([
                if e.press { 128 } else { 0 },
                e.delay as u8,
                (e.delay >> 4) as u8,
            ]);
        }
    }
    s.extend([0, 0]);
    ensure!(s.len() <= 280, "macro exceeds five pages");
    let n = s.len().div_ceil(56);
    s.chunks(56)
        .enumerate()
        .map(|(i, d)| macro_page(id, i as u8, i + 1 == n, d))
        .collect()
}
pub fn macro_keycode(id: u8, repeat_mode: u8) -> Result<[u8; 4]> {
    ensure!(repeat_mode <= 2, "invalid repeat mode");
    Ok([9, repeat_mode, id, 0])
}
/// Wired USB branch of PrepareGetBatteryStateBuffer (receiver/Bluetooth intentionally excluded).
pub fn get_battery() -> Report {
    header(&[0x83])
}
pub fn parse_battery(r: &[u8]) -> Result<(u8, u8)> {
    ensure!(
        r.len() >= 3 && r[0] == 0x83 && r[1] <= 100 && r[2] <= 3,
        "invalid battery response"
    );
    Ok((r[1], r[2]))
}
#[derive(Clone, Debug)]
pub struct MacroAction {
    pub key: u8,
    pub start: u32,
    pub duration: u32,
}
/// PrepareMacroData: stable timeline ordering, ignoring initial silence like the original.
pub fn macro_data(id: u8, actions: &[MacroAction]) -> Result<Vec<Report>> {
    if actions.is_empty() {
        return Ok(Vec::new());
    }
    let mut ordered: Vec<_> = actions.iter().collect();
    ordered.sort_by_key(|a| a.start);
    let mut timeline = Vec::new();
    for a in ordered {
        let end = a
            .start
            .checked_add(a.duration)
            .ok_or_else(|| anyhow::anyhow!("macro timestamp overflow"))?;
        timeline.push((a.start, a.key, true));
        timeline.push((end, a.key, false));
    }
    timeline.sort_by_key(|e| e.0);
    let mut events = Vec::new();
    for (i, e) in timeline.iter().enumerate() {
        let delay = timeline
            .get(i + 1)
            .map(|next| next.0 - e.0)
            .unwrap_or(1)
            .max(1);
        ensure!(delay <= 65535, "macro delay exceeds 16 bits");
        events.push(MacroEvent {
            key: e.1,
            press: e.2,
            delay: delay as u16,
        });
    }
    macro_reports(id, 1, &events)
}

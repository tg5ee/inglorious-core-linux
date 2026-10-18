//! Six-screen GUI from the Figma redesign (file j70ge8ICVlsLKIjCpWPGy2, page "Screens"),
//! coloured by the active Omarchy theme and laid out for Hyprland tiles of any size.
mod effects;
mod theme;
mod widgets;
use eframe::egui::{self, Align, Color32, Layout, Sense, Ui, pos2, vec2};
use glorious_core_rs::{
    device::{self, Command, Event},
    protocol::{self as p, KeyLight, MacroEvent},
    state::{self, Advanced, State},
    tables::{EFFECTS, KEYCODES, MULTICOLOUR, MULTICOLOUR_ONLY, SINGLE_COLOUR},
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};
use theme::{Colours, Theme, pal};
use widgets::{self as w, KeyLook, Kind};

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Lighting,
    Actuation,
    Keymap,
    Advanced,
    Performance,
    Device,
}
const SCREENS: [(Screen, &str, &str); 6] = {
    use egui_phosphor::regular as i;
    [
        (Screen::Lighting, "Lighting", i::PALETTE),
        (Screen::Actuation, "Actuation", i::SLIDERS_HORIZONTAL),
        (Screen::Keymap, "Keymap", i::KEYBOARD),
        (Screen::Advanced, "Advanced keys", i::MAGIC_WAND),
        (Screen::Performance, "Performance", i::GAUGE),
        (Screen::Device, "Device", i::CPU),
    ]
};
/// Independently applied slices of the state; a screen is "unsaved" when its parts differ.
#[derive(Clone, Copy)]
enum Part {
    Preset,
    PerKey,
    Actuation,
    Keys,
    Performance,
}
impl Screen {
    fn parts(self) -> &'static [Part] {
        match self {
            Screen::Lighting => &[Part::Preset, Part::PerKey],
            Screen::Actuation => &[Part::Actuation],
            Screen::Keymap | Screen::Advanced => &[Part::Keys],
            Screen::Performance => &[Part::Performance],
            Screen::Device => &[],
        }
    }
}
fn copy(part: Part, pi: usize, li: usize, from: &State, to: &mut State) {
    let (a, b) = (&from.profiles[pi], &mut to.profiles[pi]);
    let li = if matches!(part, Part::Preset | Part::PerKey) { 0 } else { li };
    let (la, lb) = (&a.layers[li], &mut b.layers[li]);
    match part {
        Part::Preset => {
            lb.lighting = la.lighting.clone();
            lb.side_lighting = la.side_lighting.clone();
        }
        Part::PerKey => (lb.colors, lb.per_key_on) = (la.colors.clone(), la.per_key_on),
        Part::Actuation => {
            lb.actuation = la.actuation.clone();
            lb.per_key = la.per_key.clone();
        }
        Part::Keys => {
            lb.bindings = la.bindings.clone();
            lb.advanced = la.advanced.clone();
        }
        Part::Performance => (b.polling, b.debounce, b.sleep) = (a.polling, a.debounce, a.sleep),
    }
}
fn same(part: Part, pi: usize, li: usize, x: &State, y: &State) -> bool {
    let (a, b) = (&x.profiles[pi], &y.profiles[pi]);
    let li = if matches!(part, Part::Preset | Part::PerKey) { 0 } else { li };
    let (la, lb) = (&a.layers[li], &b.layers[li]);
    match part {
        Part::Preset => la.lighting == lb.lighting && la.side_lighting == lb.side_lighting,
        Part::PerKey => la.colors == lb.colors && la.per_key_on == lb.per_key_on,
        Part::Actuation => la.actuation == lb.actuation && la.per_key == lb.per_key,
        Part::Keys => la.bindings == lb.bindings && la.advanced == lb.advanced,
        Part::Performance => (a.polling, a.debounce, a.sleep) == (b.polling, b.debounce, b.sleep),
    }
}

const LAYERS: [&str; 3] = ["Base", "Fn", "Fn2"];
const SWATCHES: [[u8; 3]; 7] = [
    [0x1f, 0x4f, 0xff],
    [0x7b, 0x2f, 0xf7],
    [0x00, 0xc8, 0x53],
    [0xe5, 0x39, 0x35],
    [0xf5, 0xa4, 0x3a],
    [0xff, 0xff, 0xff],
    [0x00, 0xe5, 0xff],
];
const RATES: [u16; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];
const STAGES: [&str; 4] = ["1st press", "2nd press", "2nd release", "1st release"];
const ADV_KINDS: [&str; 4] = ["Dynamic keystroke", "Mod-tap", "Toggle", "Macro"];
const CATEGORIES: [(&str, &str); 5] = [
    ("Keyboard", "Keyboard keys"),
    ("Media", "Media keys"),
    ("Mouse", "Mouse buttons"),
    ("Macro", "Macros"),
    ("Disabled", "Turn the key off"),
];

fn code_name(code: u8) -> Option<&'static str> {
    KEYCODES.iter().find(|(_, c)| *c == code).map(|(n, _)| *n)
}
/// "CapsLock" -> "Caps Lock", "KeyA" -> "A", "mouse_left" -> "Mouse left".
fn pretty(name: &str) -> String {
    let n = name.strip_prefix("Key").or_else(|| name.strip_prefix("Digit")).unwrap_or(name).replace('_', " ");
    let mut out = String::new();
    let mut prev = ' ';
    for c in n.chars() {
        if c.is_uppercase() && prev.is_lowercase() {
            out.push(' ');
        }
        out.push(c);
        prev = c;
    }
    let mut cs = out.chars();
    cs.next().map(|f| f.to_uppercase().chain(cs).collect()).unwrap_or_default()
}
/// Keycap-sized keycode label.
fn short(name: &str) -> String {
    match name {
        "Escape" => "Esc",
        "Return" | "Enter" => "Enter",
        "PageUp" => "PgUp",
        "PageDown" => "PgDn",
        "PrintScreen" => "PrtSc",
        "CapsLock" => "Caps",
        "ScrollLock" => "ScrLk",
        "NumLock" => "NumLk",
        "Delete" => "Del",
        "Insert" => "Ins",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "Backquote" => "`",
        "Minus" => "-",
        "Equal" => "=",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "ControlLeft" => "LCtrl",
        "ControlRight" => "RCtrl",
        "ShiftLeft" => "LShift",
        "ShiftRight" => "RShift",
        "AltLeft" => "LAlt",
        "AltRight" => "RAlt",
        _ => return name.strip_prefix("Numpad").map_or_else(|| pretty(name), |n| format!("Num {}", pretty(n))),
    }
    .into()
}
fn binding_name(c: &[[u8; 4]; 4]) -> String {
    match c[0] {
        [9, _, id, _] => format!("Macro {}", id as u16 + 1),
        [_, _, 0, _] => "Disabled".into(),
        [_, _, code, _] => code_name(code).map_or_else(|| format!("Code {code}"), pretty),
    }
}
fn binding_short(c: &[[u8; 4]; 4]) -> String {
    match c[0] {
        [9, _, id, _] => format!("M{}", id as u16 + 1),
        [_, _, 0, _] => "Off".into(),
        [_, _, code, _] => code_name(code).map_or_else(|| code.to_string(), short),
    }
}
/// Phosphor glyph per EFFECTS value.
fn effect_icon(effect: u8) -> &'static str {
    use egui_phosphor::regular as i;
    match effect {
        8 => i::PROHIBIT,
        0 => i::SPARKLE,
        1 => i::WAVE_SINE,
        3 => i::WAVE_TRIANGLE,
        4 => i::SPIRAL,
        5 => i::FLASK,
        2 => i::SUN_DIM,
        6 => i::CIRCLE,
        7 => i::TARGET,
        9 => i::FOOTPRINTS,
        10 => i::PERSON_SIMPLE_RUN,
        11 => i::BINARY,
        12 => i::DOTS_NINE,
        13 => i::RAINBOW,
        14 => i::HEARTBEAT,
        15 => i::NUMBER_SQUARE_EIGHT,
        16 => i::LIGHTNING,
        17 => i::PING_PONG,
        18 => i::MOUNTAINS,
        _ => i::QUESTION,
    }
}
fn adv_name(a: &Advanced) -> String {
    match a {
        Advanced::Dks { .. } => "Dynamic keystroke".into(),
        Advanced::ModTap { .. } => "Mod-tap".into(),
        Advanced::Toggle { .. } => "Toggle".into(),
        Advanced::Macro { id, .. } => format!("Macro {}", *id as u16 + 1),
    }
}
fn adv_extra(a: &Advanced) -> String {
    match a {
        Advanced::Dks { modes, .. } => format!("{} stages", modes.iter().filter(|m| **m != 0).count()),
        Advanced::ModTap { timeout } => format!("{timeout} ms"),
        Advanced::Toggle { .. } => String::new(),
        Advanced::Macro { repeat, .. } => ["×1", "toggle", "hold"].get(*repeat as usize).unwrap_or(&"").to_string(),
    }
}
fn adv_short(a: &Advanced) -> &'static str {
    match a {
        Advanced::Dks { .. } => "DKS",
        Advanced::ModTap { .. } => "Tap·Hold",
        Advanced::Toggle { .. } => "Toggle",
        Advanced::Macro { .. } => "Macro",
    }
}
fn parse_macro(text: &str) -> anyhow::Result<Vec<MacroEvent>> {
    let mut events = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<_> = line.split_whitespace().collect();
        anyhow::ensure!(parts.len() == 3, "macro lines: KeyA down|up delay_ms");
        let key = KEYCODES
            .iter()
            .find(|(n, _)| *n == parts[0])
            .ok_or_else(|| anyhow::anyhow!("unknown key {}", parts[0]))?
            .1;
        anyhow::ensure!(["down", "up"].contains(&parts[1]), "expected down or up");
        events.push(MacroEvent { key, press: parts[1] == "down", delay: parts[2].parse()? });
    }
    anyhow::ensure!(!events.is_empty(), "macro must contain events");
    Ok(events)
}
fn keycode_combo(ui: &mut Ui, id: impl std::hash::Hash, code: &mut u8, width: f32) {
    let selected = code_name(*code).filter(|_| *code != 0).map_or_else(|| "None".into(), pretty);
    // Truncate keeps every combo on the same layout path, so short and long names line up.
    egui::ComboBox::from_id_salt(id).width(width).height(300.).wrap_mode(egui::TextWrapMode::Truncate).selected_text(selected).show_ui(ui, |ui| {
        ui.selectable_value(code, 0, "None");
        for (n, c) in KEYCODES.iter().filter(|(_, c)| *c != 0) {
            ui.selectable_value(code, *c, pretty(n));
        }
    });
}

pub struct App {
    state: State,
    applied: State,
    tx: Sender<Command>,
    rx: Receiver<Event>,
    join: Option<std::thread::JoinHandle<()>>,
    device_tex: egui::TextureHandle,
    dry: bool,
    screen: Screen,
    busy: bool,
    progress: (usize, usize),
    notice: Option<(String, bool)>,
    connected: bool,
    board_slot: Option<(u8, u8)>,
    fw: Vec<String>,
    ports: [String; 2],
    /// Multi-selection (lighting, actuation) and single selection (keymap, advanced).
    sel: BTreeSet<u8>,
    key: u8,
    paint: [u8; 3],
    paint_palette: bool,
    side_follow: bool,
    /// Base effect colour follows the Omarchy accent (the theme-set hook is installed).
    theme_sync: bool,
    per_key_scope: bool,
    live: bool,
    travel: f32,
    show_remaps: bool,
    category: usize,
    adv_kind: usize,
    codes: [u8; 4],
    first: f32,
    modes: [u8; 4],
    timeout: u16,
    toggle_kind: u8,
    macro_text: String,
    macro_id: u8,
    repeat: u8,
    confirm: String,
    calib: u8,
    pending_calibration: Option<u8>,
    theme: Theme,
    colours: Colours,
    theme_rx: Receiver<Theme>,
    /// Pending zenity dialog: (save?, picked path).
    dialog: Option<Receiver<(bool, anyhow::Result<Option<PathBuf>>)>>,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, state: State, dry: bool) -> Self {
        w::install(&cc.egui_ctx);
        let (theme, colours) = (Theme::load(), Colours::load());
        theme::apply(&cc.egui_ctx, theme.palette(colours));
        let theme_rx = theme::watch(cc.egui_ctx.clone());
        let img = image::load_from_memory(include_bytes!("../../assets/device.png")).expect("bundled PNG").to_rgba8();
        let device_tex = cc.egui_ctx.load_texture(
            "device",
            egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img),
            egui::TextureOptions::LINEAR,
        );
        let (tx, rx, join) = device::worker(dry);
        // Reads only: firmware, active profile/layer, sleep.
        let _ = tx.send(Command::Info);
        let mut app = Self {
            applied: state.clone(),
            state,
            tx,
            rx,
            join: Some(join),
            device_tex,
            dry,
            screen: Screen::Lighting,
            busy: true,
            progress: (0, 0),
            notice: None,
            connected: false,
            board_slot: None,
            fw: Vec::new(),
            ports: ["—".into(), "—".into()],
            sel: BTreeSet::new(),
            key: 3,
            paint: SWATCHES[0],
            paint_palette: false,
            side_follow: false,
            // Dry runs (and the tests) leave the real hook alone.
            theme_sync: !dry && device::theme_sync_hook().is_ok_and(|h| h.exists()),
            per_key_scope: false,
            live: false,
            travel: 0.,
            show_remaps: true,
            category: 0,
            adv_kind: 0,
            codes: [4, 5, 0, 0],
            first: 1.5,
            modes: [1, 0, 0, 1],
            timeout: 200,
            toggle_kind: 0,
            macro_text: "KeyA down 50\nKeyA up 1".into(),
            macro_id: 0,
            repeat: 0,
            confirm: String::new(),
            calib: 0,
            pending_calibration: None,
            theme,
            colours,
            theme_rx,
            dialog: None,
        };
        // Rewrite the hook so it points at this binary even after a reinstall elsewhere.
        if app.theme_sync && !dry && let Err(e) = device::set_theme_sync(true) {
            app.notice = Some((format!("Could not update the theme sync hook: {e}"), true));
        }
        app.follow_theme();
        app.sync_layer_ui();
        app
    }
    /// The hook already wrote the board and the saved file; bring this window's copies in line.
    fn follow_theme(&mut self) {
        if let (true, Some(rgb)) = (self.theme_sync, self.theme.accent) {
            self.state.follow_theme(rgb);
            self.applied.follow_theme(rgb);
        }
    }
    fn set_theme_sync(&mut self, on: bool) {
        if !self.dry && let Err(e) = device::set_theme_sync(on) {
            self.notice = Some((format!("Could not {} theme sync: {e}", if on { "turn on" } else { "turn off" }), true));
            return;
        }
        self.theme_sync = on;
        let Some(rgb) = self.theme.accent.filter(|_| on) else { return };
        // Turning it on applies the accent now, like a theme switch would.
        let mut s = self.applied.clone();
        (s.profile, s.layer) = (self.state.profile, self.state.layer);
        self.state.follow_theme(rgb);
        if s.follow_theme(rgb) {
            let l = s.lighting_state();
            let r = p::lighting(&l.lighting, false).and_then(|a| Ok(vec![a, p::lighting(&l.side_lighting, true)?]));
            self.write(r, Some(s));
        }
    }
    /// Lighting mode and side-light link follow whatever the selected layer holds.
    fn sync_layer_ui(&mut self) {
        self.paint_palette = false;
        let l = self.state.lighting_state();
        self.side_follow = l.side_lighting == l.lighting && l.colors[128..158].iter().all(|c| c.mode == 0);
    }
    fn sync_side_lighting(&mut self) {
        if self.side_follow {
            let l = self.state.lighting_state_mut();
            l.side_lighting = l.lighting.clone();
            // Side overrides would mask the followed preset.
            for c in &mut l.colors[128..158] {
                c.mode = 0;
            }
        }
    }
    fn send(&mut self, c: Command) {
        match self.tx.send(c) {
            Ok(()) => {
                self.busy = true;
                self.progress = (0, 0);
            }
            Err(e) => self.notice = Some((e.to_string(), true)),
        }
    }
    fn write(&mut self, r: anyhow::Result<Vec<p::Report>>, saved: Option<State>) {
        let r = r.and_then(|reports| {
            let Some(s) = saved.as_ref() else { return Ok(reports); };
            if self.board_slot == Some((s.profile, s.layer)) {
                return Ok(reports);
            }
            let profile_changed = self.board_slot.map(|slot| slot.0) != Some(s.profile);
            let mut activation = if profile_changed {
                state::activate_reports(s)?
            } else {
                let mut r = vec![p::set_profile(s.profile, s.layer)?];
                r.extend(state::actuation_reports(s)?);
                r
            };
            // Activation already includes saved actuation, and profile-wide lighting on profile changes.
            if self.screen != Screen::Actuation && !(profile_changed && self.screen == Screen::Lighting) {
                activation.extend(reports);
            }
            Ok(activation)
        });
        match r {
            Ok(reports) if reports.is_empty() => self.notice = Some(("Nothing to write.".into(), false)),
            Ok(reports) => self.send(Command::Write { reports, state: saved.map(Box::new) }),
            Err(e) => self.notice = Some((format!("{e:#}"), true)),
        }
    }
    fn at(&self) -> (usize, usize) {
        (self.state.profile as usize, self.state.layer as usize)
    }
    /// Last applied state with `parts` of the edited selection merged in.
    fn commit(&self, parts: &[Part]) -> State {
        let (pi, li) = self.at();
        let mut s = self.applied.clone();
        (s.profile, s.layer) = (self.state.profile, self.state.layer);
        s.profiles[pi].selected_layer = self.state.layer;
        for part in parts {
            copy(*part, pi, li, &self.state, &mut s);
        }
        s
    }
    fn screen_dirty(&self, screen: Screen) -> bool {
        let (pi, li) = self.at();
        screen.parts().iter().any(|part| !same(*part, pi, li, &self.state, &self.applied))
    }
    fn dirty(&self) -> bool {
        self.screen_dirty(self.screen)
    }
    fn needs_apply(&self) -> bool {
        self.dirty() || self.board_slot != Some((self.state.profile, self.state.layer))
    }
    fn revert(&mut self) {
        let (pi, li) = self.at();
        for part in self.screen.parts() {
            copy(*part, pi, li, &self.applied, &mut self.state);
        }
        self.sync_layer_ui();
    }
    fn apply(&mut self) {
        match self.screen {
            Screen::Lighting => {
                self.sync_side_lighting();
                let r = state::lighting_changes(&self.state, &self.applied);
                self.write(r, Some(self.commit(&[Part::Preset, Part::PerKey])));
            }
            Screen::Actuation => self.write(state::actuation_reports(&self.state), Some(self.commit(&[Part::Actuation]))),
            Screen::Keymap | Screen::Advanced => {
                self.write(state::key_changes(&self.state, &self.applied), Some(self.commit(&[Part::Keys])))
            }
            Screen::Performance => {
                self.write(state::performance_changes(&self.state, &self.applied), Some(self.commit(&[Part::Performance])))
            }
            Screen::Device => {}
        }
    }
    /// Browse the saved slot and its draft without changing the keyboard.
    fn select_slot(&mut self, profile: u8, layer: u8) {
        (self.state.profile, self.state.layer) = (profile, layer);
        self.state.profiles[profile as usize].selected_layer = layer;
        self.sync_layer_ui();
    }
    fn select_profile(&mut self, profile: u8) {
        self.select_slot(profile, self.state.profiles[profile as usize].selected_layer);
    }
    fn start_calibration(&mut self) {
        if self.busy || self.confirm != "CALIBRATE" {
            return;
        }
        self.confirm.clear();
        let stage = self.calib + 1;
        self.write(p::calibration(stage).map(|r| vec![r]), None);
        if self.busy {
            self.pending_calibration = Some(stage);
        }
    }
    fn poll(&mut self) {
        while let Ok(e) = self.rx.try_recv() {
            match e {
                Event::Progress(i, n) => self.progress = (i, n),
                Event::Info { fw, profile, layer, ports, .. } => {
                    self.fw = fw.split(" / ").map(String::from).collect();
                    self.ports = ports;
                    self.connected = true;
                    if !self.dry {
                        self.board_slot = Some((profile, layer));
                    }
                    // Dry-run replies are all zeros; keep the saved slot instead.
                    if !self.dry {
                        for s in [&mut self.state, &mut self.applied] {
                            (s.profile, s.layer) = (profile, layer);
                            s.profiles[profile as usize].selected_layer = layer;
                        }
                        self.sync_layer_ui();
                    }
                }
                Event::Saved(s) => self.applied = *s,
                Event::ActiveSlot { profile, layer } => self.board_slot = Some((profile, layer)),
                Event::Live(v) if self.live => self.travel = v,
                Event::Live(_) => {}
                Event::Done => {
                    self.busy = false;
                    if let Some(stage) = self.pending_calibration.take() {
                        self.calib = stage % 4;
                    }
                }
                Event::Error(e) => {
                    self.board_slot = None;
                    self.pending_calibration = None;
                    self.live = false;
                    self.busy = false;
                    self.notice = Some((e, true));
                }
                Event::Status(s) => self.notice = Some((s, false)),
            }
        }
    }

    /// Sidebar: brand, device card, profile/layer and labelled navigation.
    fn sidebar(&mut self, ui: &mut Ui) {
        let c = pal();
        let (dot, status) = match (self.dry, self.connected, self.busy) {
            (true, ..) => (c.accent, "Dry run · simulated".to_string()),
            (_, true, _) => (c.ok, format!("USB · {}", self.fw.first().map_or("", String::as_str))),
            (_, _, true) => (c.dim, "Connecting…".into()),
            _ => (c.danger, "Not connected".into()),
        };
        ui.spacing_mut().item_spacing.y = 24.;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            w::spaced(ui, "INGLORIOUS", w::logo(24.), c.accent, 1.);
            w::spaced(ui, "CORE · LINUX", w::strong(11.), c.dim, 2.);
        });
        egui::Frame::new().fill(c.surface1).corner_radius(12).inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.;
            // device.png is 600x400 with the board drawn at (66, 98)-(534, 302), shadow included.
            let uv = egui::Rect::from_min_max(pos2(66. / 600., 98. / 400.), pos2(534. / 600., 302. / 400.));
            let width = ui.available_width();
            let (r, _) = ui.allocate_exact_size(vec2(width, width * 204. / 468.), Sense::hover());
            ui.painter().image(self.device_tex.id(), r, uv, Color32::WHITE);
            w::text(ui, "GMMK 3 HE 75%", w::strong(15.), c.text);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(8., 8.), Sense::hover());
                ui.painter().circle_filled(r.center(), 4., dot);
                w::text(ui, status, w::body(12.), c.muted);
            });
        });
        let active = self.board_slot.map_or_else(|| "Keyboard profile: unknown".into(), |(p, l)| format!("Keyboard (last confirmed): {} · {}", p + 1, LAYERS[l as usize]));
        w::text(ui, active, w::body(11.), c.muted);
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.spacing_mut().item_spacing.y = 24.;
            let (pi, li) = (self.state.profile, self.state.layer);
            for (title, labels, current) in [("EDIT PROFILE", ["1", "2", "3"], pi), ("EDIT LAYER", LAYERS, li)] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6., 8.);
                    w::spaced(ui, title, w::strong(11.), c.dim, 1.5);
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 12.) / 3.;
                        for (i, l) in (0u8..).zip(labels) {
                            let tip = if title == "EDIT PROFILE" {
                                format!("Edit saved profile {} — changes reach the keyboard only when applied", i + 1)
                            } else {
                                format!("Edit the {} layer — changes reach the keyboard only when applied", LAYERS[i as usize])
                            };
                            if w::button_w(ui, l, Kind::Seg(i == current), Some(width)).on_hover_text(tip).clicked() && i != current {
                                if title == "EDIT PROFILE" { self.select_profile(i) } else { self.select_slot(pi, i) }
                            }
                        }
                    });
                });
            }
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            for (n, (s, label, icon)) in SCREENS.into_iter().enumerate() {
                let resp = w::nav_item(ui, icon, label, self.screen == s, self.screen_dirty(s));
                if resp.on_hover_text(format!("Ctrl+{}", n + 1)).clicked() {
                    self.screen = s;
                }
            }
        });
        if ui.available_height() > 40. {
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 2.;
                w::text(ui, format!("inglorious-core {}", env!("CARGO_PKG_VERSION")), w::body(11.), c.dim);
                w::text(ui, "Browse here; Apply writes to keyboard.", w::body(11.), c.dim);
            });
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let c = pal();
        let (pi, li) = self.at();
        let (title, sub) = match self.screen {
            Screen::Lighting => ("Lighting", format!("Profile {} — shared across Base, Fn and Fn2", pi + 1)),
            Screen::Actuation => ("Actuation", "Hall-effect travel, rapid trigger and deadzone".into()),
            Screen::Keymap => ("Keymap", format!("Remap keys on Profile {} · {}", pi + 1, LAYERS[li])),
            Screen::Advanced => ("Advanced keys", "Dynamic keystroke, mod-tap, toggle and macros".into()),
            Screen::Performance => ("Performance", format!("Polling rate, debounce and sleep — applies to Profile {}", pi + 1)),
            Screen::Device => ("Device", "GMMK 3 HE 75% ANSI · 342d:e3de".into()),
        };
        let title_block = |ui: &mut Ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.;
                w::text(ui, title, w::heading(28.), c.text);
                w::text(ui, &sub, w::body(13.), c.muted);
            });
        };
        // Narrow tiles put the actions on their own line under the title.
        if ui.available_width() >= 760. {
            ui.horizontal(|ui| {
                title_block(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.header_actions(ui));
            });
        } else {
            title_block(ui);
            ui.horizontal(|ui| ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.header_actions(ui)));
        }
        if let Some((msg, err)) = self.notice.clone() {
            egui::Frame::new()
                .fill(c.surface1)
                .stroke(w::line(1., if err { c.danger } else { c.border_strong }))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(16, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let icon = if err { egui_phosphor::regular::WARNING } else { egui_phosphor::regular::INFO };
                        w::text(ui, icon, w::body(16.), if err { c.danger } else { c.muted });
                        ui.allocate_ui(vec2(ui.available_width() - 90., 36.), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(msg).color(if err { c.danger } else { c.muted })).wrap());
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if w::button(ui, "Dismiss", Kind::Ghost).clicked() {
                                self.notice = None;
                            }
                        });
                    });
                });
        }
    }
    /// Header actions, right to left: Apply (or Re-read), Revert, status pill.
    fn header_actions(&mut self, ui: &mut Ui) {
        let c = pal();
        ui.spacing_mut().item_spacing.x = 12.;
        let dirty = self.dirty();
        if self.screen == Screen::Device {
            let reread = ui.add_enabled_ui(!self.busy, |ui| w::button(ui, "Re-read device", Kind::Secondary)).inner;
            if reread.on_hover_text("Read firmware, profile and sleep again (Ctrl+R)").clicked() {
                self.send(Command::Info);
            }
        } else {
            let apply = ui.add_enabled_ui(!self.busy && self.needs_apply(), |ui| w::button(ui, "Apply to keyboard", Kind::Primary)).inner;
            if apply.on_hover_text("Write this screen's changes to the keyboard (Ctrl+S)").clicked() {
                self.apply();
            }
            if dirty && !self.busy && w::button(ui, "Revert", Kind::Ghost).on_hover_text("Discard this screen's unapplied changes (Ctrl+Z)").clicked() {
                self.revert();
            }
        }
        match (self.busy, self.progress, self.screen) {
            (true, (i, n), _) if n > 0 => w::pill(ui, c.accent, &format!("Writing {i}/{n}…"), true),
            (true, ..) => w::pill(ui, c.accent, if self.connected { "Working…" } else { "Connecting…" }, true),
            (_, _, Screen::Device) if !self.connected => w::pill(ui, c.danger, "Not connected", false),
            _ if self.board_slot != Some((self.state.profile, self.state.layer)) => w::pill(ui, c.accent, "Activation pending", false),
            _ if dirty => w::pill(ui, c.accent, "Unsaved changes", false),
            _ => w::pill(ui, c.ok, "No unapplied changes", false),
        }
    }
    /// Ctrl+1…6 switch screens, Ctrl+S / Ctrl+Enter apply, Ctrl+Z reverts, Ctrl+R / F5 re-reads.
    /// Skipped while a text field has focus so its own keys (undo, Enter) keep working.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        if ctx.wants_keyboard_input() {
            return;
        }
        let hit = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        for (n, k) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6].into_iter().enumerate() {
            if hit(Modifiers::COMMAND, k) {
                self.screen = SCREENS[n].0;
            }
        }
        if self.busy {
            return;
        }
        if (hit(Modifiers::COMMAND, Key::S) || hit(Modifiers::COMMAND, Key::Enter)) && self.needs_apply() {
            self.apply();
        }
        if hit(Modifiers::COMMAND, Key::Z) && self.dirty() {
            self.revert();
        }
        if hit(Modifiers::COMMAND, Key::R) || hit(Modifiers::NONE, Key::F5) {
            self.send(Command::Info);
        }
    }
    /// Runs the zenity file dialog on its own thread so the window keeps drawing.
    fn open_dialog(&mut self, ctx: &egui::Context, save: bool) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((save, device::pick_file(save)));
            ctx.request_repaint();
        });
        self.dialog = Some(rx);
    }
    /// Export or import once the dialog returns.
    fn poll_dialog(&mut self) {
        let Some(Ok((save, picked))) = self.dialog.as_ref().map(Receiver::try_recv) else {
            return;
        };
        self.dialog = None;
        let path = match picked {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(e) => return self.notice = Some((format!("{e:#}"), true)),
        };
        if save {
            self.notice = Some(match device::save_state_to(&self.applied, &path) {
                Ok(()) => (format!("Exported to {}", path.display()), false),
                Err(e) => (format!("{e:#}"), true),
            });
            return;
        }
        match device::load_state_from(&path) {
            Ok(mut s) => {
                (s.profile, s.layer) = (self.state.profile, self.state.layer);
                self.state = s;
                self.sync_layer_ui();
                self.notice = Some(("Imported. Review each screen, then apply.".into(), false));
            }
            Err(e) => self.notice = Some((format!("{e:#}"), true)),
        }
    }

    /// Quick-select chips; returns true when one was clicked.
    fn quick(&mut self, ui: &mut Ui, names: &[&str]) -> bool {
        let mut clicked = false;
        for name in names {
            let set: BTreeSet<u8> = w::keys()
                .filter(|(_, n)| match *name {
                    "All" => true,
                    "Alphas" => n.starts_with("Key"),
                    "Numbers" => n.starts_with("Digit"),
                    "F-keys" => n.len() > 1 && n.starts_with('F') && n[1..].chars().all(|c| c.is_ascii_digit()),
                    "WASD" => matches!(*n, "KeyW" | "KeyA" | "KeyS" | "KeyD"),
                    "Arrows" => n.starts_with("Arrow"),
                    "Modifiers" => matches!(*n, "ShiftLeft" | "ShiftRight" | "ControlLeft" | "MetaLeft" | "AltLeft" | "AltRight" | "Fn"),
                    _ => false,
                })
                .map(|(i, _)| i)
                .collect();
            if w::button(ui, name, Kind::Seg(!set.is_empty() && set == self.sel)).clicked() {
                self.sel = set;
                clicked = true;
            }
        }
        clicked
    }
    fn toggle_sel(&mut self, k: u8) {
        if !self.sel.remove(&k) {
            self.sel.insert(k);
        }
    }
    fn set_colour(&mut self, c: [u8; 3]) {
        if self.paint_palette {
            self.paint = c;
            let l = self.state.lighting_state_mut();
            for k in &self.sel {
                l.colors[*k as usize] = KeyLight { rgb: c, mode: 1 };
            }
        } else {
            // The original only offers a colour in single-colour mode.
            let l = &mut self.state.lighting_state_mut().lighting;
            l.rgb = c;
            if !MULTICOLOUR_ONLY.contains(&l.effect) {
                l.color_mode = SINGLE_COLOUR;
            }
        }
    }

    fn lighting(&mut self, ui: &mut Ui) {
        if self.state.lighting_state().lighting.effect == 6 {
            self.state.lighting_state_mut().lighting.color_mode = SINGLE_COLOUR;
        }
        // A newly picked effect takes the theme colour too.
        if let (true, Some(rgb)) = (self.theme_sync, self.theme.accent) {
            self.state.lighting_state_mut().follow_theme(rgb);
        }
        self.sync_side_lighting();
        w::panel(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.;
            w::toolbar(ui, 860., |ui, right| {
                if right {
                    let on = &mut self.state.lighting_state_mut().per_key_on;
                    if w::button(ui, "Per-key paint", Kind::Seg(*on)).on_hover_text("Painted colours on top of the base effect").clicked() {
                        *on = true;
                        self.paint_palette = true;
                    }
                    if w::button(ui, "Preset effect", Kind::Seg(!*on)).on_hover_text("Base effect on every key; painted colours are kept").clicked() {
                        *on = false;
                        self.paint_palette = false;
                    }
                } else {
                    self.quick(ui, &["All", "Alphas", "Numbers", "F-keys", "WASD", "Arrows", "Modifiers", "Clear"]);
                }
            });
            let l = self.state.lighting_state();
            // Keys without a per-key colour preview the base effect (in the app only).
            let (t, off) = (ui.input(|i| i.time) as f32, pal().surface2);
            if effects::animated(l.lighting.effect) {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
            }
            let clicked = w::keyboard(ui, &|k| KeyLook {
                fill: match l.colors.get(k as usize).filter(|c| l.per_key_on && c.mode != 0) {
                    Some(c) => Some(w::rgb(c.rgb)),
                    None if l.lighting.effect != 8 => w::key_pos(k).map(|(x, y)| effects::sample(&l.lighting, x, y, t, off)),
                    None => None,
                },
                selected: self.sel.contains(&k),
                ..Default::default()
            });
            if let Some(k) = clicked.filter(|k| *k != w::ENCODER) {
                self.toggle_sel(k);
            }
        });
        let r = w::row(ui, &[0., 360., 250.], 314.);
        let mut show_effect = false;
        let (can_apply, mut apply_now, theme_sync) = (self.needs_apply(), false, self.theme_sync);
        w::place(ui, r[0], |ui| {
            w::card(ui, "Base effect", Some("Shown on keys without a per-key colour"), |ui| {
                let (t, off) = (ui.input(|i| i.time) as f32, pal().surface2);
                let l = &mut self.state.lighting_state_mut().lighting;
                let shown = l.clone();
                let name = EFFECTS.iter().find(|(_, v)| *v == l.effect).map_or("Unknown", |(n, _)| *n);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("effect")
                        .width(ui.available_width() - 96.)
                        .height(480.)
                        .selected_text(egui::RichText::new(format!("{}   {name}", effect_icon(l.effect))).size(14.).color(pal().text))
                        .show_ui(ui, |ui| {
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                            for (name, v) in EFFECTS {
                                let row = egui::RichText::new(format!("{}   {name}", effect_icon(*v))).size(14.);
                                let resp = ui.selectable_label(l.effect == *v, row);
                                // Single Color previews the chosen RGB; other effects preview at full brightness.
                                let look = p::Lighting { effect: *v, color_mode: if *v == 6 { SINGLE_COLOUR } else { MULTICOLOUR }, brightness: 100, ..shown.clone() };
                                let thumb = egui::Rect::from_min_size(pos2(resp.rect.right() - 62., resp.rect.center().y - 9.), vec2(54., 18.));
                                effects::thumb(ui.painter(), thumb, &look, t, pal().surface3);
                                if resp.clicked() {
                                    // Picking a base effect hides painted overrides.
                                    l.effect = *v;
                                    if *v == 6 {
                                        l.color_mode = SINGLE_COLOUR;
                                    } else if MULTICOLOUR_ONLY.contains(v) {
                                        l.color_mode = MULTICOLOUR;
                                    }
                                    show_effect = true;
                                }
                            }
                        });
                    let (chip, _) = ui.allocate_exact_size(vec2(84., w::CONTROL_H), Sense::hover());
                    ui.painter().rect_filled(chip, 8, off);
                    effects::thumb(ui.painter(), chip.shrink(6.), l, t, pal().surface3);
                    ui.interact(chip, ui.id().with("chip"), Sense::hover()).on_hover_text("Preview, drawn by the app: the keyboard runs the real effect");
                });
                // Same action as the header button.
                let apply_button = |ui: &mut Ui| ui.add_enabled_ui(can_apply, |ui| w::button(ui, "Apply", Kind::Primary)).inner.on_hover_text("Ctrl+S").clicked();
                let narrow = ui.available_width() < 320.;
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!MULTICOLOUR_ONLY.contains(&l.effect) && !theme_sync, |ui| {
                        ui.spacing_mut().item_spacing.x = 6.;
                        for (mode, label) in [(MULTICOLOUR, "Multicolour"), (SINGLE_COLOUR, "Single colour")] {
                            if l.effect == 6 && mode == MULTICOLOUR { continue; }
                            if w::button(ui, label, Kind::Seg(l.color_mode == mode)).clicked() {
                                l.color_mode = mode;
                                show_effect = true;
                            }
                        }
                    });
                    if !narrow {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| apply_now = apply_button(ui));
                    }
                });
                if narrow {
                    apply_now = apply_button(ui);
                }
            })
        });
        if show_effect {
            self.state.lighting_state_mut().per_key_on = false;
            self.paint_palette = false;
        }
        if apply_now {
            self.apply();
        }
        w::place(ui, r[1], |ui| {
            w::card(ui, "Colour", None, |ui| {
                ui.horizontal(|ui| {
                    if w::button(ui, "Base colour", Kind::Seg(!self.paint_palette)).clicked() {
                        self.paint_palette = false;
                    }
                    if w::button(ui, "Key paint", Kind::Seg(self.paint_palette)).clicked() {
                        self.paint_palette = true;
                        self.state.lighting_state_mut().per_key_on = true;
                    }
                });
                let per_key = self.paint_palette;
                let mut sync = self.theme_sync;
                if per_key {
                    ui.label("Paint colour for selected keys");
                } else {
                    let tip = "The base effect takes the Omarchy theme accent on every theme switch, also while the app is closed. Effects with firmware colours are left alone.";
                    w::split(ui, "Base effect colour", |ui| {
                        ui.add_enabled_ui(self.theme.accent.is_some() || sync, |ui| w::toggle(ui, &mut sync))
                            .response
                            .on_hover_text(tip)
                            .on_disabled_hover_text("No accent colour in the current Omarchy theme");
                        w::text(ui, "Follow theme", w::body(13.), pal().muted);
                    });
                }
                let locked = sync && !per_key;
                let target = if per_key { self.paint } else { self.state.lighting_state().lighting.rgb };
                let mut pick = target;
                let mut picked = false;
                let mut erase = false;
                let theme_rgb = self.theme.keyboard;
                ui.add_enabled_ui(!locked, |ui| ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.;
                    let n = SWATCHES.len() + 1 + usize::from(per_key) + usize::from(theme_rgb.is_some());
                    let size = ((ui.available_width() + 6.) / n as f32 - 6.).clamp(20., 28.);
                    if let Some(t) = theme_rgb {
                        let tip = format!("{} theme keyboard colour", self.theme.name.as_deref().unwrap_or("Omarchy"));
                        if w::swatch(ui, w::rgb(t), target == t, size).on_hover_text(tip).clicked() {
                            pick = t;
                            picked = true;
                        }
                    }
                    for c in SWATCHES {
                        if w::swatch(ui, w::rgb(c), target == c, size).clicked() {
                            pick = c;
                            picked = true;
                        }
                    }
                    picked |= w::colour_button(ui, &mut pick, size, Some(egui_phosphor::regular::PLUS)).on_hover_text("Custom colour").changed();
                    if per_key {
                        erase = w::no_colour(ui, size).on_hover_text("Remove colour: selected keys show the base effect").clicked();
                    }
                }));
                if sync != self.theme_sync {
                    self.set_theme_sync(sync);
                }
                if picked {
                    self.set_colour(pick);
                }
                if erase {
                    let l = self.state.lighting_state_mut();
                    for k in &self.sel {
                        l.colors[*k as usize] = KeyLight::default();
                    }
                }
                let l = &mut self.state.lighting_state_mut().lighting;
                w::slider_int(ui, "Brightness", &format!("{}%", l.brightness), &mut l.brightness, 0.0..=100., 1.);
                ui.add_enabled_ui(!matches!(l.effect, 6 | 8), |ui| {
                    w::slider_int(ui, "Speed", &format!("{}%", l.speed), &mut l.speed, 0.0..=100., 1.);
                });
            })
        });
        w::place(ui, r[2], |ui| {
            w::card(ui, "Side lights", None, |ui| {
                w::split(ui, "Follow keyboard", |ui| w::toggle(ui, &mut self.side_follow));
                ui.add_enabled_ui(!self.side_follow, |ui| {
                    ui.spacing_mut().item_spacing.y = 16.;
                    let l = self.state.lighting_state_mut();
                    let s = &mut l.side_lighting;
                    let recoloured = w::split(ui, "Colour", |ui| w::colour_button(ui, &mut s.rgb, 28., None)).changed();
                    w::slider_int(ui, "Brightness", &format!("{}%", s.brightness), &mut s.brightness, 0.0..=100., 1.);
                    if recoloured {
                        s.effect = 6;
                        s.color_mode = SINGLE_COLOUR;
                        // Keep the remembered side colour current even while paint is off.
                        l.colors[128..158].fill(KeyLight { rgb: s.rgb, mode: 1 });
                    }
                });
            })
        });
    }

    fn actuation(&mut self, ui: &mut Ui) {
        let global = self.state.current().actuation.clone();
        w::panel(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.;
            w::toolbar(ui, 560., |ui, right| {
                if right {
                    if w::button(ui, "Per-key", Kind::Seg(self.per_key_scope)).on_hover_text("Override the selected keys").clicked() {
                        self.per_key_scope = true;
                    }
                    if w::button(ui, "Global", Kind::Seg(!self.per_key_scope)).on_hover_text("One setting for every key on this layer").clicked() {
                        self.per_key_scope = false;
                    }
                } else if self.quick(ui, &["All", "Alphas", "WASD", "Arrows", "Clear"]) {
                    self.per_key_scope = !self.sel.is_empty();
                }
            });
            let l = self.state.current();
            let clicked = w::keyboard(ui, &|k| {
                let a = l.per_key.get(&k);
                KeyLook {
                    value: (k < 90 && k != w::ENCODER).then(|| format!("{:.1}", a.unwrap_or(&global).press)),
                    selected: self.per_key_scope && self.sel.contains(&k),
                    raised: a.is_some(),
                    ..Default::default()
                }
            });
            if let Some(k) = clicked.filter(|k| *k < 90 && *k != w::ENCODER) {
                if !self.per_key_scope {
                    self.per_key_scope = true;
                    self.sel.clear();
                }
                self.toggle_sel(k);
            }
        });
        let keys: Vec<u8> = self.sel.iter().copied().filter(|k| *k < 90 && *k != w::ENCODER).collect();
        let usable = !self.per_key_scope || !keys.is_empty();
        let mut a = match (self.per_key_scope, keys.first()) {
            (true, Some(k)) => self.state.current().per_key.get(k).cloned().unwrap_or_else(|| global.clone()),
            _ => global.clone(),
        };
        let sub = match (self.per_key_scope, keys.len()) {
            (false, _) => "All keys · 0.1 – 4.0 mm".to_string(),
            (true, 0) => "Select keys to override the global point".into(),
            (true, n) => format!("{n} keys selected · 0.1 – 4.0 mm"),
        };
        let has_override = self.per_key_scope && keys.iter().any(|k| self.state.current().per_key.contains_key(k));
        let (mut changed, mut reset) = (false, false);
        let mut deadzone = global.deadzone.unwrap_or(4.);
        let r = w::row(ui, &[0., 0., 260.], 246.);
        let dz_changed = w::place(ui, r[0], |ui| {
            w::card(ui, "Actuation point", Some(&sub), |ui| {
                ui.add_enabled_ui(usable, |ui| {
                    changed |= w::slider(ui, "Press point", &format!("{:.1} mm", a.press), &mut a.press, 0.1..=4., 0.1).changed();
                });
                let dz = w::slider(ui, "Bottom deadzone", &format!("{deadzone:.1} mm"), &mut deadzone, 0.1..=4., 0.1)
                    .on_hover_text("Global for the layer; the firmware ignores per-key deadzones.")
                    .changed();
                if has_override {
                    reset = w::button(ui, "Reset selected to global", Kind::Secondary).clicked();
                }
                dz
            })
        });
        w::place(ui, r[1], |ui| {
            w::card(ui, "Rapid trigger", Some("Re-fire on small movements"), |ui| {
                ui.add_enabled_ui(usable, |ui| {
                    ui.spacing_mut().item_spacing.y = 16.;
                    changed |= w::split(ui, "Enabled", |ui| w::toggle(ui, &mut a.rapid)).changed();
                    ui.add_enabled_ui(a.rapid, |ui| {
                        ui.spacing_mut().item_spacing.y = 16.;
                        changed |= w::slider(ui, "Press sensitivity", &format!("{:.1} mm", a.rapid_press), &mut a.rapid_press, 0.1..=4., 0.1).changed();
                        changed |= w::slider(ui, "Release sensitivity", &format!("{:.1} mm", a.rapid_release), &mut a.rapid_release, 0.1..=4., 0.1).changed();
                    });
                });
            })
        });
        let press = a.press;
        w::place(ui, r[2], |ui| {
            w::card(ui, "Live travel", Some("Press any key to test"), |ui| {
                let mm = self.travel * 4.;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.;
                    let (bar, _) = ui.allocate_exact_size(vec2(28., 120.), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(bar, 8, pal().surface3);
                    let level = egui::Rect::from_min_max(pos2(bar.left(), bar.bottom() - bar.height() * self.travel), bar.max);
                    p.rect_filled(level, egui::CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }, pal().accent);
                    let y = bar.bottom() - bar.height() * (press / 4.);
                    p.line_segment([pos2(bar.left(), y), pos2(bar.right(), y)], w::line(2., pal().text));
                    ui.vertical(|ui| {
                        ui.add_space(70.);
                        ui.spacing_mut().item_spacing.y = 4.;
                        w::text(ui, format!("{mm:.1} mm"), w::mono(22.), pal().text);
                        let down = self.live && mm >= press;
                        let state = if !self.live { "live view off" } else if down { "actuated" } else { "released" };
                        w::text(ui, state, w::body(12.), if down { pal().ok } else { pal().dim });
                    });
                });
                let mut live = self.live;
                if w::split(ui, "Live view", |ui| w::toggle(ui, &mut live)).changed() {
                    self.live = live;
                    self.travel = 0.;
                    self.send(Command::Live(live));
                }
            })
        });
        let l = self.state.current_mut();
        if changed {
            if self.per_key_scope {
                a.deadzone = None;
                for k in &keys {
                    l.per_key.insert(*k, a.clone());
                }
            } else {
                a.deadzone = l.actuation.deadzone;
                l.actuation = a;
            }
        }
        if dz_changed {
            l.actuation.deadzone = Some(deadzone);
        }
        if reset {
            for k in &keys {
                l.per_key.remove(k);
            }
        }
    }

    fn keymap(&mut self, ui: &mut Ui) {
        w::panel(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.;
            w::toolbar(ui, 560., |ui, right| {
                if right {
                    if w::button(ui, "Show remaps", Kind::Seg(self.show_remaps)).clicked() {
                        self.show_remaps = true;
                    }
                    if w::button(ui, "Show labels", Kind::Seg(!self.show_remaps)).clicked() {
                        self.show_remaps = false;
                    }
                } else {
                    w::text(ui, "Click a key, then pick what it should send.", w::body(13.), pal().muted);
                }
            });
            let l = self.state.current();
            let clicked = w::keyboard(ui, &|k| {
                let remap = l.bindings.get(&k).filter(|_| self.show_remaps && !l.advanced.contains_key(&k));
                KeyLook { value: remap.map(binding_short), raised: remap.is_some(), selected: k == self.key, ..Default::default() }
            });
            if let Some(k) = clicked.filter(|k| *k != w::BADGE) {
                self.key = k;
            }
        });
        let k = self.key;
        let l = self.state.current();
        let default = state::default_code(k).ok().and_then(code_name).map_or_else(|| "—".into(), pretty);
        let sends = l.bindings.get(&k).map_or_else(|| "Default".into(), binding_name);
        let custom = l.bindings.contains_key(&k) || l.advanced.contains_key(&k);
        let current = l.bindings.get(&k).map(|c| c[0]);
        let changes: Vec<(u8, String)> =
            l.bindings.iter().filter(|(k, _)| !l.advanced.contains_key(k)).map(|(k, c)| (*k, binding_name(c))).collect();
        let r = w::row(ui, &[280., 0., 300.], 246.);
        w::place(ui, r[0], |ui| {
            w::card(ui, &pretty(w::key_name(k)), Some(&format!("Selected key · physical #{k}")), |ui| {
                ui.spacing_mut().item_spacing.y = 14.;
                w::kv(ui, "Default", &default, w::strong(13.));
                w::kv(ui, "Sends", &sends, w::strong(13.));
                if ui.add_enabled_ui(custom, |ui| w::button(ui, "Reset to default", Kind::Secondary)).inner.clicked() {
                    let l = self.state.current_mut();
                    l.bindings.remove(&k);
                    l.advanced.remove(&k);
                }
            })
        });
        let mut pick = None;
        w::place(ui, r[1], |ui| {
            w::card(ui, "Assign", Some(CATEGORIES[self.category].1), |ui| {
                ui.spacing_mut().item_spacing.y = 14.;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6., 6.);
                    for (i, (n, _)) in CATEGORIES.iter().enumerate() {
                        if w::button(ui, n, Kind::Seg(i == self.category)).clicked() {
                            self.category = i;
                        }
                    }
                });
                egui::ScrollArea::vertical().id_salt("codes").auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6., 6.);
                        match self.category {
                            0 | 2 => {
                                let mut seen = BTreeSet::new();
                                for (n, c) in KEYCODES {
                                    if *c == 0 || (*c >= 240) != (self.category == 2) || !seen.insert(*c) {
                                        continue;
                                    }
                                    let on = current == Some([0, 0, *c, 0]);
                                    if w::button(ui, &short(n), Kind::Seg(on)).on_hover_text(pretty(n)).clicked() {
                                        pick = Some(*c);
                                    }
                                }
                            }
                            1 => {
                                w::para(ui, "Media keys need consumer-control codes this port doesn't send yet.", w::body(13.), pal().dim);
                            }
                            3 => {
                                w::para(ui, "Macros are recorded and bound in Advanced keys.", w::body(13.), pal().dim);
                            }
                            _ => {
                                if w::button(ui, "Disable key", Kind::Seg(current == Some([0; 4]))).clicked() {
                                    pick = Some(0);
                                }
                            }
                        }
                    });
                });
            })
        });
        if let Some(c) = pick {
            let l = self.state.current_mut();
            l.bindings.insert(k, [[0, 0, c, 0], [0; 4], [0; 4], [0; 4]]);
            l.advanced.remove(&k);
        }
        w::place(ui, r[2], |ui| {
            w::card(ui, "Changes on this layer", None, |ui| {
                ui.spacing_mut().item_spacing.y = 14.;
                if changes.is_empty() {
                    w::para(ui, "No remaps on this layer.", w::body(13.), pal().dim);
                }
                egui::ScrollArea::vertical().id_salt("changes").show(ui, |ui| {
                    for (k, t) in changes {
                        if w::list_item(ui, w::key_label(k), &t, "", k == self.key).clicked() {
                            self.key = k;
                        }
                    }
                });
            })
        });
    }

    fn select_adv(&mut self, k: u8) {
        self.key = k;
        let l = self.state.current();
        let (codes, adv) = (l.bindings.get(&k).copied(), l.advanced.get(&k).cloned());
        match adv {
            Some(Advanced::Dks { first, modes, .. }) => (self.adv_kind, self.first, self.modes) = (0, first, modes),
            Some(Advanced::ModTap { timeout }) => (self.adv_kind, self.timeout) = (1, timeout),
            Some(Advanced::Toggle { kind }) => (self.adv_kind, self.toggle_kind) = (2, kind),
            Some(Advanced::Macro { id, repeat, events }) => {
                (self.adv_kind, self.macro_id, self.repeat) = (3, id, repeat);
                self.macro_text = events
                    .iter()
                    .map(|e| format!("{} {} {}", code_name(e.key).unwrap_or("?"), if e.press { "down" } else { "up" }, e.delay))
                    .collect::<Vec<_>>()
                    .join("\n");
                return;
            }
            None => return,
        }
        if let Some(c) = codes {
            self.codes = c.map(|s| s[2]);
        }
    }
    /// "+ Add binding": stores the edited advanced function on the selected key (applied later).
    fn store_adv(&mut self) {
        let slots = self.codes.map(|c| [0, 0, c, 0]);
        let r: anyhow::Result<_> = match self.adv_kind {
            0 => Ok((Advanced::Dks { first: self.first, second: self.first, modes: self.modes }, slots)),
            1 => Ok((Advanced::ModTap { timeout: self.timeout }, slots)),
            2 => Ok((Advanced::Toggle { kind: self.toggle_kind }, slots)),
            _ => (|| {
                let events = parse_macro(&self.macro_text)?;
                let code = p::macro_keycode(self.macro_id, self.repeat)?;
                Ok((Advanced::Macro { id: self.macro_id, repeat: self.repeat, events }, [code, [0; 4], [0; 4], [0; 4]]))
            })(),
        };
        match r {
            Ok((a, c)) => {
                let l = self.state.current_mut();
                l.advanced.insert(self.key, a);
                l.bindings.insert(self.key, c);
            }
            Err(e) => self.notice = Some((format!("{e:#}"), true)),
        }
    }

    fn advanced(&mut self, ui: &mut Ui) {
        let has = self.state.current().advanced.contains_key(&self.key);
        let mut add = false;
        w::panel(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.;
            w::toolbar(ui, 560., |ui, right| {
                if right {
                    let tip = "Store this function on the selected key; Apply writes it";
                    add = w::button(ui, if has { "Update binding" } else { "+ Add binding" }, Kind::Secondary).on_hover_text(tip).clicked();
                } else {
                    for (i, n) in ADV_KINDS.iter().enumerate() {
                        if w::button(ui, n, Kind::Seg(i == self.adv_kind)).clicked() {
                            self.adv_kind = i;
                        }
                    }
                }
            });
            let l = self.state.current();
            let clicked = w::keyboard(ui, &|k| {
                let a = l.advanced.get(&k);
                KeyLook { value: a.map(|a| adv_short(a).into()), raised: a.is_some(), selected: k == self.key, ..Default::default() }
            });
            if let Some(k) = clicked.filter(|k| *k != w::BADGE) {
                self.select_adv(k);
            }
        });
        if add {
            self.store_adv();
        }
        let label = w::key_label(self.key);
        let r = w::row(ui, &[300., 0., 300.], 246.);
        let actions = |ui: &mut Ui, codes: &mut [u8; 4], names: [&str; 2]| {
            for (i, n) in names.iter().enumerate() {
                w::split(ui, n, |ui| keycode_combo(ui, ("slot", i), &mut codes[i], 180.));
            }
        };
        match self.adv_kind {
            0 => {
                w::place(ui, r[0], |ui| {
                    w::card(ui, "Actuation point", Some(&format!("{label} · dynamic keystroke")), |ui| {
                        w::slider(ui, "First point", &format!("{:.1} mm", self.first), &mut self.first, 0.5..=2.5, 0.1);
                        w::para(ui, "The firmware takes one DKS point.", w::body(12.), pal().dim);
                    })
                });
                w::place(ui, r[1], |ui| {
                    w::card(ui, "Stages", Some("●  fires once  ·  ▬  held through the stage  ·  click a cell to cycle"), |ui| self.stage_matrix(ui))
                });
            }
            1 => {
                w::place(ui, r[0], |ui| {
                    w::card(ui, "Mod-tap", Some(&format!("{label} · hold for one action, tap for another")), |ui| {
                        w::slider_int(ui, "Hold timeout", &format!("{} ms", self.timeout), &mut self.timeout, 10.0..=1000., 10.);
                    })
                });
                w::place(ui, r[1], |ui| w::card(ui, "Actions", None, |ui| actions(ui, &mut self.codes, ["Hold", "Tap"])));
            }
            2 => {
                w::place(ui, r[0], |ui| {
                    w::card(ui, "Toggle", Some(&format!("{label} · latch an action")), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.;
                            for (i, n) in (0u8..).zip(["Type 1", "Type 2"]) {
                                if w::button(ui, n, Kind::Seg(self.toggle_kind == i)).clicked() {
                                    self.toggle_kind = i;
                                }
                            }
                        });
                    })
                });
                w::place(ui, r[1], |ui| w::card(ui, "Actions", None, |ui| actions(ui, &mut self.codes, ["Toggle action", "Base action"])));
            }
            _ => {
                w::place(ui, r[0], |ui| {
                    w::card(ui, "Macro", Some(&format!("{label} · plays recorded events")), |ui| {
                        w::slider_int(ui, "Macro slot", &format!("{}", self.macro_id as u16 + 1), &mut self.macro_id, 0.0..=255., 1.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.;
                            for (i, n) in (0u8..).zip(["Once", "Toggle", "Hold"]) {
                                if w::button(ui, n, Kind::Seg(self.repeat == i)).clicked() {
                                    self.repeat = i;
                                }
                            }
                        });
                    })
                });
                w::place(ui, r[1], |ui| {
                    w::card(ui, "Events", Some("One per line: KeyA down 50 (delay after, ms)"), |ui| {
                        ui.add(egui::TextEdit::multiline(&mut self.macro_text).font(w::mono(13.)).desired_rows(7).desired_width(f32::INFINITY));
                    })
                });
            }
        }
        let rows: Vec<(u8, String, String)> =
            self.state.current().advanced.iter().map(|(k, a)| (*k, adv_name(a), adv_extra(a))).collect();
        w::place(ui, r[2], |ui| {
            w::card(ui, "Bindings on this layer", None, |ui| {
                ui.spacing_mut().item_spacing.y = 14.;
                if rows.is_empty() {
                    w::para(ui, "Pick a key and type, then + Add binding.", w::body(13.), pal().dim);
                }
                egui::ScrollArea::vertical().id_salt("bindings").show(ui, |ui| {
                    for (k, n, x) in rows {
                        if w::list_item(ui, w::key_label(k), &n, &x, k == self.key).clicked() {
                            self.select_adv(k);
                        }
                    }
                });
            })
        });
    }
    /// The firmware stores one action (slot) and one mode per stage (CreateDKSModes).
    fn stage_matrix(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing = vec2(6., 6.);
        let cw = ((ui.available_width() - 84. - 24.) / 4.).floor();
        let cell = |ui: &mut Ui, s: &str, width: f32, font: egui::FontId, color: Color32| {
            let (r, _) = ui.allocate_exact_size(vec2(width, w::CONTROL_H), Sense::hover());
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, s, font, color);
        };
        if cw < 72. {
            for (i, stage) in STAGES.iter().enumerate() {
                egui::Frame::new().fill(pal().surface2).corner_radius(8).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        cell(ui, stage, 84., w::body(12.), pal().muted);
                        let combo = (ui.available_width() - 52.).max(60.);
                        ui.allocate_ui(vec2(combo, w::CONTROL_H), |ui| keycode_combo(ui, ("stage", i), &mut self.codes[i], combo - 8.));
                        if w::stage_cell(ui, self.modes[i], 40.).clicked() {
                            self.modes[i] = (self.modes[i] + 1) % 3;
                        }
                    });
                });
            }
            return;
        }
        ui.horizontal(|ui| {
            cell(ui, "", 84., w::body(12.), pal().dim);
            for s in STAGES {
                cell(ui, s, cw, w::body(12.), pal().dim);
            }
        });
        for row in 0..2 {
            egui::Frame::new().fill(pal().surface2).corner_radius(8).show(ui, |ui| {
                // Every cell gets an absolute rect in the row, so a combo can never push the
                // next one down (egui's centred rows re-centre on whatever grew last).
                let (line, _) = ui.allocate_exact_size(vec2(84. + 4. * (cw + 6.), w::CONTROL_H), Sense::hover());
                let at = |x: f32, width: f32| egui::Rect::from_min_size(pos2(line.left() + x, line.top()), vec2(width, w::CONTROL_H));
                let label = at(0., 84.);
                ui.painter().text(label.center(), egui::Align2::CENTER_CENTER, ["Sends", "Mode"][row], w::strong(13.), pal().text);
                for i in 0..4 {
                    let slot = at(90. + i as f32 * (cw + 6.), cw);
                    let builder = egui::UiBuilder::new().max_rect(slot).layout(Layout::top_down(Align::Center));
                    ui.scope_builder(builder, |ui| {
                        if row == 0 {
                            keycode_combo(ui, ("stage", i), &mut self.codes[i], cw - 8.);
                        } else if w::stage_cell(ui, self.modes[i], cw).clicked() {
                            self.modes[i] = (self.modes[i] + 1) % 3;
                        }
                    });
                }
            });
        }
    }

    fn performance(&mut self, ui: &mut Ui) {
        let pi = self.state.profile as usize;
        let pr = &mut self.state.profiles[pi];
        w::card_with(ui, 20, false, false, "Polling rate", Some("How often the keyboard reports to the PC"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6., 6.);
                for rate in RATES {
                    if w::button(ui, &format!("{rate} Hz"), Kind::Seg(pr.polling == rate)).clicked() {
                        pr.polling = rate;
                    }
                }
            });
            w::para(ui, "8000 Hz gives the lowest latency but uses more CPU. Drop to 1000 Hz if you see stutter.", w::body(12.), pal().dim);
        });
        let r = w::row(ui, &[0., 0.], 190.);
        w::place(ui, r[0], |ui| {
            w::card(ui, "Debounce", Some("Ignores chatter after a press"), |ui| {
                w::slider_int(ui, "Debounce time", &format!("{} ms", pr.debounce), &mut pr.debounce, 0.0..=100., 5.);
                w::para(ui, "Hall-effect switches rarely need more than 5 ms.", w::body(12.), pal().dim);
            })
        });
        w::place(ui, r[1], |ui| {
            w::card(ui, "Sleep", Some("Turns lighting off when idle"), |ui| {
                let mut on = pr.sleep != 0;
                let now = if on { format!("Currently after {} min", pr.sleep / 60) } else { "Currently never".into() };
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.;
                        w::text(ui, "Sleep on USB", w::body(13.), pal().text);
                        w::text(ui, now, w::body(12.), pal().dim);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if w::toggle(ui, &mut on).changed() {
                            pr.sleep = if on { 300 } else { 0 };
                        }
                    });
                });
                let mut min = (pr.sleep / 60).max(1);
                ui.add_enabled_ui(on, |ui| {
                    if w::slider_int(ui, "Idle time", &format!("{min} min"), &mut min, 1.0..=60., 1.).changed() {
                        pr.sleep = min * 60;
                    }
                });
            })
        });
    }

    fn device(&mut self, ui: &mut Ui) {
        let fw = |i: usize| self.fw.get(i).map_or_else(|| "—".to_string(), |s| s.trim_start_matches(|c: char| c.is_ascii_uppercase()).into());
        let r = w::row(ui, &[0., 0.], 194.);
        w::place(ui, r[0], |ui| {
            w::card(ui, "Firmware", None, |ui| {
                w::kv_select(ui, "Keyboard", &fw(0), w::mono(13.));
                w::kv_select(ui, "RF", &fw(1), w::mono(13.));
                w::kv_select(ui, "LED", &fw(2), w::mono(13.));
                w::para(ui, "Firmware updates are not supported on Linux.", w::body(12.), pal().dim);
            })
        });
        w::place(ui, r[1], |ui| {
            w::card(ui, "Connection", None, |ui| {
                w::kv(ui, "Mode", if self.dry { "Dry run" } else { "USB" }, w::mono(13.));
                w::kv_select(ui, "Settings port", &format!("{} · FFFF/02", self.ports[0]), w::mono(13.));
                w::kv_select(ui, "Events port", &format!("{} · FFFF/01", self.ports[1]), w::mono(13.));
            })
        });
        let full = device::state_path().map(|p| p.display().to_string()).unwrap_or_default();
        let path = match dirs::home_dir() {
            Some(h) => full.replacen(&*h.to_string_lossy(), "~", 1),
            None => full.clone(),
        };
        let mut action = None;
        let idle = self.dialog.is_none();
        let r = w::row(ui, &[0., 360.], 168.);
        w::place(ui, r[0], |ui| w::card(ui, "Saved settings", Some("The keyboard cannot report most settings back, so the app keeps a copy"), |ui| {
            w::split(ui, "File", |ui| {
                if w::icon_button(ui, egui_phosphor::regular::COPY, "Copy path").clicked() {
                    ui.ctx().copy_text(full.clone());
                }
                ui.add(egui::Label::new(egui::RichText::new(&path).font(w::mono(13.)).color(pal().text)).selectable(true).truncate());
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10., 10.);
                for (i, (l, tip)) in [
                    ("Export…", "Save the applied settings to a file"),
                    ("Import…", "Load a settings file into the editor (nothing is written until Apply)"),
                    ("Re-apply everything", "Send this slot's saved lighting, actuation, keys and performance again"),
                ]
                .iter()
                .enumerate()
                {
                    if ui.add_enabled_ui(idle || i == 2, |ui| w::button(ui, l, Kind::Secondary)).inner.on_hover_text(*tip).clicked() {
                        action = Some(i);
                    }
                }
            });
        }));
        let mut colours = self.colours;
        w::place(ui, r[1], |ui| {
            w::card(ui, "Appearance", Some("Colours only; the layout is the same either way"), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6., 6.);
                    let theme = self.theme.name.as_deref().map_or_else(|| "Omarchy theme".into(), |n| format!("{n} theme"));
                    let found = self.theme.palette.is_some();
                    let omarchy = ui.add_enabled_ui(found, |ui| w::button(ui, &theme, Kind::Seg(colours == Colours::Omarchy))).inner;
                    if omarchy.on_disabled_hover_text("No Omarchy theme found").clicked() {
                        colours = Colours::Omarchy;
                    }
                    if w::button(ui, "Amber", Kind::Seg(colours == Colours::Amber)).clicked() {
                        colours = Colours::Amber;
                    }
                });
                w::para(ui, "Omarchy colours follow theme changes as they happen.", w::body(12.), pal().dim);
            })
        });
        if colours != self.colours {
            self.colours = colours;
            theme::apply(ui.ctx(), self.theme.palette(colours));
            if let Err(e) = colours.save() {
                self.notice = Some((format!("Could not save the colour choice: {e}"), true));
            }
        }
        match action {
            Some(i @ (0 | 1)) => self.open_dialog(&ui.ctx().clone(), i == 0),
            Some(_) => {
                let mut s = self.applied.clone();
                (s.profile, s.layer) = (self.state.profile, self.state.layer);
                let r = (|| {
                    let mut r = state::activate_reports(&s)?;
                    let pr = &s.profiles[s.profile as usize];
                    r.extend([p::report_rate(s.profile, pr.polling)?, p::debounce(pr.debounce)?, p::set_sleep(pr.sleep)]);
                    r.extend(state::key_changes(&s, &State::default())?);
                    Ok(r)
                })();
                self.write(r, None);
            }
            None => {}
        }
        let mut act = None;
        let calib = if self.calib == 0 { "Start calibration".to_string() } else { format!("Calibration step {}/4", self.calib + 1) };
        w::card_with(ui, 20, false, true, "Danger zone", Some("These write to the keyboard immediately"), |ui| {
            for (title, desc, label, word) in [
                ("Calibrate switches", "Release all keys, then press each key fully when asked", calib.as_str(), "CALIBRATE"),
                ("Factory reset", "Erases all profiles, lighting and keymaps on the board", "Reset keyboard", "RESET"),
            ] {
                let text = |ui: &mut Ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.;
                        w::text(ui, title, w::body(13.), pal().text);
                        w::text(ui, desc, w::body(12.), pal().dim);
                    });
                };
                let mut button = |ui: &mut Ui| {
                    let tip = format!("Type {word} in the box below to unlock");
                    // Both danger buttons share one width so their edges line up.
                    if ui.add_enabled_ui(self.confirm == word, |ui| w::button_w(ui, label, Kind::Danger, Some(176.))).inner.on_disabled_hover_text(tip).clicked() {
                        act = Some(word);
                    }
                };
                // Narrow tiles stack the button under its description.
                if ui.available_width() >= 560. {
                    ui.horizontal(|ui| {
                        text(ui);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| button(ui));
                    });
                } else {
                    text(ui);
                    button(ui);
                }
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.confirm)
                    .hint_text("Type RESET or CALIBRATE to unlock")
                    .desired_width(270.)
                    .margin(vec2(12., 8.)),
            );
        });
        match act {
            Some("CALIBRATE") => {
                self.start_calibration();
            }
            Some(_) => {
                self.confirm.clear();
                self.write(Ok(vec![p::reset()]), None);
            }
            None => {}
        }
    }
}
impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        while let Ok(t) = self.theme_rx.try_recv() {
            theme::apply(ctx, t.palette(self.colours));
            self.theme = t;
            self.follow_theme();
        }
        self.poll_dialog();
        self.shortcuts(ctx);
        // Worker events arrive on a channel; poll it only while some can arrive. Idle windows
        // draw nothing (theme and dialog threads wake the UI themselves).
        if self.busy || self.live {
            ctx.request_repaint_after(std::time::Duration::from_millis(if self.live { 33 } else { 50 }));
        }
        let c = pal();
        let size = ctx.input(|i| i.content_rect().size());
        let side = 248.;
        let pad: i8 = if size.x - side >= 1000. { 32 } else { 20 };
        egui::SidePanel::left("nav")
            .exact_width(side)
            .resizable(false)
            .frame(egui::Frame::new().fill(c.panel).inner_margin(egui::Margin { left: 16, right: 16, top: pad, bottom: 20 }))
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(c.bg)).show(ctx, |ui| {
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                egui::Frame::new().inner_margin(pad).show(ui, |ui| {
                    // Content follows the window; only below ~360 px does it scroll sideways.
                    ui.set_width(ui.available_width().max(360.));
                    ui.spacing_mut().item_spacing.y = 20.;
                    self.header(ui);
                    ui.add_enabled_ui(!self.busy, |ui| {
                        ui.spacing_mut().item_spacing.y = 20.;
                        match self.screen {
                            Screen::Lighting => self.lighting(ui),
                            Screen::Actuation => self.actuation(ui),
                            Screen::Keymap => self.keymap(ui),
                            Screen::Advanced => self.advanced(ui),
                            Screen::Performance => self.performance(ui),
                            Screen::Device => self.device(ui),
                        }
                    });
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_sync_on_sends_the_accent_once_and_locks_the_base_colour() {
        let (mut app, commands, _) = app();
        app.theme.accent = Some([0x45, 0x8f, 0x82]);
        let l = app.state.lighting_state_mut();
        l.lighting.effect = 1;
        l.side_lighting = l.lighting.clone(); // side lights follow the keyboard
        app.applied = app.state.clone();
        app.set_theme_sync(true);
        let Command::Write { reports, state: Some(saved) } = commands.try_recv().unwrap() else { panic!("sync write") };
        assert_eq!(reports.iter().map(|r| (r[0], r[4], r[5..8].to_vec())).collect::<Vec<_>>(), [(7, SINGLE_COLOUR, vec![0x45, 0x8f, 0x82]), (8, SINGLE_COLOUR, vec![0x45, 0x8f, 0x82])]);
        assert_eq!(saved.lighting_state().lighting.rgb, [0x45, 0x8f, 0x82]);
        // Picking Multicolour afterwards snaps back to the theme colour.
        app.state.lighting_state_mut().lighting.color_mode = MULTICOLOUR;
        Probe::new(&mut app);
        assert_eq!(app.state.lighting_state().lighting.color_mode, SINGLE_COLOUR);
    }
    fn app() -> (App, Receiver<Command>, Sender<Event>) {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = App::new(&cc, State::default(), true);
        app.tx.send(Command::Shutdown).unwrap();
        app.join.take().unwrap().join().unwrap();
        let (tx, commands) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        app.tx = tx;
        app.rx = rx;
        app.busy = false;
        app.board_slot = Some((0, 0));
        (app, commands, events)
    }
    #[test]
    fn live_samples_after_switching_off_do_not_move_the_gauge() {
        let (mut app, _, events) = app();
        app.live = true;
        events.send(Event::Live(0.75)).unwrap();
        app.poll();
        assert_eq!(app.travel, 0.75);
        app.live = false;
        events.send(Event::Live(0.25)).unwrap();
        app.poll();
        assert_eq!(app.travel, 0.75, "ignore samples queued before live view was disabled");
    }
    #[test]
    fn profile_selection_browses_saved_slots_without_writes_and_keeps_drafts() {
        let (mut app, commands, _) = app();
        app.applied.profiles[1].layers[0].lighting.brightness = 23;
        app.state = app.applied.clone();
        app.state.lighting_state_mut().lighting.brightness = 71;
        app.select_slot(1, 2);
        assert!(commands.try_recv().is_err(), "browsing must never send a device command");
        assert!(!app.busy);
        assert_eq!((app.state.profile, app.state.layer), (1, 2));
        assert_eq!(app.state.lighting_state().lighting.brightness, 23);
        assert!(!app.dirty());
        assert_eq!((app.applied.profile, app.applied.layer), (0, 0));
        app.select_slot(0, 0);
        assert_eq!(app.state.lighting_state().lighting.brightness, 71);
        assert!(app.dirty(), "returning to a slot must preserve its unapplied draft");
        assert!(commands.try_recv().is_err());
    }
    #[test]
    fn apply_after_browsing_targets_selected_slot_and_failure_preserves_selection() {
        let (mut app, commands, events) = app();
        app.select_slot(2, 1);
        assert!(commands.try_recv().is_err());
        app.state.lighting_state_mut().lighting.brightness = 73;
        app.apply();
        let Command::Write { reports, state: Some(saved) } = commands.try_recv().unwrap() else {
            panic!("Apply must send the draft");
        };
        assert_eq!((saved.profile, saved.layer), (2, 1));
        assert_eq!(saved.lighting_state().lighting.brightness, 73);
        assert_eq!(&reports[0][..3], &[5, 2, 1]);
        assert_eq!(reports.iter().find(|r| r[0] == 7).unwrap()[3], 15);
        assert_eq!(saved.profiles[0].layers[0].lighting.brightness, 50);
        events.send(Event::Error("test write failure".into())).unwrap();
        app.poll();
        assert_eq!((app.state.profile, app.state.layer), (2, 1));
        assert_eq!(app.state.lighting_state().lighting.brightness, 73);
        assert_eq!(app.applied.profiles[2].layers[0].lighting.brightness, 50);
        assert!(app.dirty());
    }
    #[test]
    fn calibration_requires_confirmation_and_advances_only_on_success() {
        let (mut app, commands, events) = app();
        for stage in 1..=4 {
            app.start_calibration();
            assert!(commands.try_recv().is_err(), "locked stages must not write");
            app.confirm = "CALIBRATE".into();
            app.start_calibration();
            assert!(app.confirm.is_empty(), "each stage must relock");
            app.confirm = "CALIBRATE".into();
            app.start_calibration();
            app.confirm.clear();
            assert_eq!(app.calib, stage - 1, "sending is not success");
            let Command::Write { reports, state } = commands.recv().unwrap() else {
                panic!("expected calibration");
            };
            assert_eq!(reports, vec![p::calibration(stage).unwrap()]);
            assert!(state.is_none());
            assert!(commands.try_recv().is_err(), "busy clicks must not queue stages");
            if stage == 1 {
                events.send(Event::Error("disconnected".into())).unwrap();
                app.poll();
                assert_eq!(app.calib, 0);
                app.confirm = "CALIBRATE".into();
                app.start_calibration();
                let Command::Write { reports, .. } = commands.recv().unwrap() else {
                    panic!("expected retry");
                };
                assert_eq!(reports, vec![p::calibration(1).unwrap()]);
            }
            events.send(Event::Done).unwrap();
            app.poll();
            assert_eq!(app.calib, stage % 4);
            assert!(app.confirm.is_empty());
        }
    }
    struct Probe {
        ctx: egui::Context,
        shapes: Vec<egui::epaint::ClippedShape>,
        tick: f64,
    }
    impl Probe {
        fn new(app: &mut App) -> Self {
            let ctx = egui::Context::default();
            w::install(&ctx);
            let mut p = Self { ctx, shapes: vec![], tick: 0. };
            p.frame(app, vec![]);
            p.frame(app, vec![]);
            p
        }
        fn frame(&mut self, app: &mut App, events: Vec<egui::Event>) {
            self.tick += 0.05;
            let out = self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(pos2(0., 0.), vec2(1600., 1100.))),
                    time: Some(self.tick),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        app.header_actions(ui);
                        app.lighting(ui);
                    });
                },
            );
            self.shapes = out.shapes;
        }
        fn texts(&self) -> Vec<(String, egui::Pos2)> {
            fn add(s: &egui::epaint::Shape, out: &mut Vec<(String, egui::Pos2)>) {
                match s {
                    egui::epaint::Shape::Text(t) => {
                        out.push((t.galley.job.text.clone(), t.pos + t.galley.rect.center().to_vec2()))
                    }
                    egui::epaint::Shape::Vec(v) => {
                        for s in v {
                            add(s, out)
                        }
                    }
                    _ => {}
                }
            }
            let mut out = vec![];
            for s in &self.shapes {
                add(&s.shape, &mut out)
            }
            out
        }
        fn click_at(&mut self, app: &mut App, pos: egui::Pos2) {
            for pressed in [true, false] {
                self.frame(
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            self.frame(app, vec![]);
        }
        fn click_text(&mut self, app: &mut App, label: &str) {
            let pos = self
                .texts()
                .into_iter()
                .rev()
                .find(|(t, _)| t == label || t.ends_with(&format!("   {label}")))
                .unwrap_or_else(|| panic!("missing {label}; {:?}", self.texts()))
                .1;
            self.click_at(app, pos);
        }
        fn swatch(&mut self, app: &mut App, rgb: [u8; 3]) {
            let pos = self
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::epaint::Shape::Circle(c) if c.fill == w::rgb(rgb) && c.radius >= 9. && c.radius <= 14. => {
                        Some(c.center)
                    }
                    _ => None,
                })
                .next()
                .expect("swatch");
            self.click_at(app, pos);
        }
    }
    #[test]
    fn single_color_palette_edits_base_even_with_saved_per_key_lighting() {
        let (mut app, commands, _) = app();
        app.state.lighting_state_mut().colors[9] = KeyLight { rgb: [12, 34, 56], mode: 1 };
        let original = app.state.lighting_state().colors.clone();
        let mut ui = Probe::new(&mut app);
        ui.click_text(&mut app, "WASD");
        for rgb in SWATCHES {
            ui.swatch(&mut app, rgb);
            assert_eq!(app.state.lighting_state().lighting.rgb, rgb);
        }
        assert_eq!(app.state.lighting_state().colors, original);
        assert!(commands.try_recv().is_err(), "palette clicks only edit the draft");
        app.apply();
        let Command::Write { reports, .. } = commands.try_recv().unwrap() else { panic!("apply") };
        let main = reports.iter().find(|r| r[0] == 7).unwrap();
        assert_eq!((main[1], main[4] & 15), (7, 9));
        assert_eq!(&main[5..8], &SWATCHES[SWATCHES.len() - 1]);
    }

    #[test]
    fn palette_target_switch_keeps_base_and_key_colours_separate() {
        let (mut app, commands, _) = app();
        let mut ui = Probe::new(&mut app);
        ui.click_text(&mut app, "WASD");
        ui.swatch(&mut app, SWATCHES[2]);
        ui.click_text(&mut app, "Key paint");
        ui.swatch(&mut app, SWATCHES[3]);
        assert_eq!(app.state.lighting_state().lighting.rgb, SWATCHES[2]);
        assert!(app.sel.iter().all(|k| app.state.lighting_state().colors[*k as usize].rgb == SWATCHES[3]));
        ui.click_text(&mut app, "Base colour");
        ui.swatch(&mut app, SWATCHES[4]);
        assert_eq!(app.state.lighting_state().lighting.rgb, SWATCHES[4]);
        assert!(app.sel.iter().all(|k| app.state.lighting_state().colors[*k as usize].rgb == SWATCHES[3]));
        ui.click_text(&mut app, "Key paint");
        ui.click_text(&mut app, "Single colour");
        assert!(!app.paint_palette);
        assert!(!app.state.lighting_state().per_key_on);
        ui.click_text(&mut app, "Per-key paint");
        assert!(app.paint_palette);
        app.select_profile(1);
        assert!(!app.paint_palette, "opening a profile starts with its base colour");
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn lighting_current_swatch_still_paints_new_selection() {
        let (mut app, _, _) = app();
        let mut ui = Probe::new(&mut app);
        ui.click_text(&mut app, "WASD");
        ui.click_text(&mut app, "Key paint");
        let paint = app.paint;
        ui.swatch(&mut app, paint);
        assert!(
            app.sel.iter().all(|k| app.state.lighting_state().colors[*k as usize].rgb == paint
                && app.state.lighting_state().colors[*k as usize].mode == 1),
            "clicking the current paint colour must paint newly selected keys"
        );
    }

    #[test]
    fn lighting_follow_applies_current_main_settings_before_next_render() {
        let (mut app, commands, _) = app();
        app.side_follow = true;
        let mut ui = Probe::new(&mut app);
        let label = ui.texts().into_iter().find(|(t, _)| t == "Brightness").unwrap().1;
        let rail = ui
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Rect(r)
                    if (r.rect.height() - 6.).abs() < 0.1
                        && r.rect.center().y > label.y
                        && r.rect.center().y < label.y + 55. =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .max_by(|a, b| a.width().total_cmp(&b.width()))
            .unwrap();
        let before = app.state.lighting_state().lighting.brightness;
        let target = pos2(rail.right() - 8., rail.center().y);
        ui.frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(target),
                egui::Event::PointerButton {
                    pos: target,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(app.state.lighting_state().lighting.brightness, 100);
        assert_ne!(before, 100);
        ui.frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::S,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        let Command::Write { reports, state } = commands.try_recv().expect("Ctrl+S must apply") else {
            panic!("apply")
        };
        assert_eq!(
            &reports[0][1..8],
            &reports[1][1..8],
            "side follow must match the main report on Ctrl+S while moving a slider"
        );
        assert_eq!(state.unwrap().lighting_state().side_lighting.brightness, 100);
    }
    #[test]
    fn lighting_empty_paint_mode_switch_can_finish_apply() {
        let (mut app, commands, events) = app();
        let mut ui = Probe::new(&mut app);
        ui.click_text(&mut app, "Preset effect");
        assert!(app.dirty());
        ui.click_text(&mut app, "Apply to keyboard");
        if let Ok(Command::Write { state: Some(saved), .. }) = commands.try_recv() {
            events.send(Event::Saved(saved)).unwrap();
            events.send(Event::Done).unwrap();
            app.poll();
        }
        assert!(!app.dirty(), "switching mode on an unpainted board must not stay permanently unsaved");
    }
    impl Probe {
        fn side_toggle(&mut self, app: &mut App) {
            let label = self.texts().into_iter().find(|(t, _)| t == "Follow keyboard").unwrap().1;
            let rect = self
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::epaint::Shape::Rect(r)
                        if (r.rect.width() - 40.).abs() < 0.1
                            && (r.rect.height() - 22.).abs() < 0.1
                            && (r.rect.center().y - label.y).abs() < 5. =>
                    {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .expect("follow toggle");
            self.click_at(app, rect.center());
        }
        fn side_colour(&mut self, app: &mut App) -> [u8; 3] {
            let old = app.state.lighting_state().side_lighting.rgb;
            self.swatch(app, old);
            let square = self
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::epaint::Shape::Mesh(m) if m.calc_bounds().width() > 70. && m.calc_bounds().height() > 70. => {
                        Some(m.calc_bounds())
                    }
                    _ => None,
                })
                .next_back()
                .expect("side colour picker");
            self.click_at(app, pos2(square.left() + square.width() * 0.5, square.top() + 10.));
            self.click_at(app, pos2(10., 1090.));
            let rgb = app.state.lighting_state().side_lighting.rgb;
            assert_ne!(rgb, old);
            rgb
        }
        fn erase(&mut self, app: &mut App) {
            let at = self
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::epaint::Shape::Circle(c) if c.radius >= 9. && c.radius <= 14. && c.fill == pal().surface2 => {
                        Some(c.center)
                    }
                    _ => None,
                })
                .next_back()
                .expect("erase swatch");
            self.click_at(app, at);
        }
    }
    #[test]
    fn side_follow_clears_side_overrides_without_touching_key_paint() {
        let (mut app, commands, _) = app();
        app.side_follow = false;
        app.state.lighting_state_mut().side_lighting.rgb = [10, 20, 30];
        app.state.lighting_state_mut().colors[128..158].fill(KeyLight { rgb: [0, 0, 255], mode: 1 });
        app.state.lighting_state_mut().colors[9] = KeyLight { rgb: [255, 0, 0], mode: 1 };
        app.state.lighting_state_mut().colors[158] = KeyLight { rgb: [0, 255, 0], mode: 1 };
        app.applied = app.state.clone();
        let mut ui = Probe::new(&mut app);
        ui.side_toggle(&mut app);
        assert!(app.side_follow);
        app.apply();
        let Command::Write { reports, state: Some(saved) } = commands.try_recv().unwrap() else {
            panic!("apply follow")
        };
        assert_eq!(saved.lighting_state().colors[9], KeyLight { rgb: [255, 0, 0], mode: 1 });
        assert_eq!(saved.lighting_state().colors[158], KeyLight { rgb: [0, 255, 0], mode: 1 });
        assert!(
            saved.lighting_state().shown_colors()[128..158].iter().all(|c| c.mode == 0),
            "Follow keyboard must stop sending stale custom side overrides"
        );
        assert_eq!(reports.iter().filter(|r| r[0] == 81).count(), 3, "must send cleared side mode pages");
    }
    #[test]
    fn side_colour_from_off_preset_sends_a_visible_single_colour() {
        let (mut app, commands, _) = app();
        app.side_follow = false;
        let l = app.state.current_mut();
        l.per_key_on = false;
        l.lighting.effect = 8;
        l.lighting.color_mode = 8;
        l.side_lighting = l.lighting.clone();
        l.side_lighting.rgb = [10, 20, 30];
        app.applied = app.state.clone();
        let mut ui = Probe::new(&mut app);
        let rgb = ui.side_colour(&mut app);
        app.apply();
        let Command::Write { reports, .. } = commands.try_recv().unwrap() else { panic!("side apply") };
        let side = reports.iter().find(|r| r[0] == 8).unwrap();
        assert_eq!((side[1], side[4] & 15), (7, 9), "side colour picker must not send Off or leave multicolour active");
        assert_eq!(&side[5..8], rgb);
    }
    #[test]
    fn side_colour_stays_consistent_when_reenabling_per_key_paint() {
        let (mut app, _, _) = app();
        app.side_follow = false;
        let l = app.state.current_mut();
        l.per_key_on = false;
        l.side_lighting.rgb = [10, 20, 30];
        l.colors[128..158].fill(KeyLight { rgb: [0, 0, 255], mode: 1 });
        let mut ui = Probe::new(&mut app);
        let rgb = ui.side_colour(&mut app);
        ui.click_text(&mut app, "Per-key paint");
        assert!(app.state.lighting_state().per_key_on);
        assert!(
            app.state.lighting_state().shown_colors()[128..158].iter().all(|c| c.rgb == rgb),
            "the side swatch must agree with the restored side colour bytes"
        );
    }
    #[test]
    fn side_colour_updates_exactly_30_leds_and_preserves_key_and_badge() {
        let (mut app, commands, _) = app();
        app.side_follow = false;
        let l = app.state.current_mut();
        l.side_lighting.rgb = [10, 20, 30];
        l.colors[9] = KeyLight { rgb: [255, 0, 0], mode: 1 };
        l.colors[158] = KeyLight { rgb: [0, 255, 0], mode: 1 };
        app.applied = app.state.clone();
        let mut ui = Probe::new(&mut app);
        let rgb = ui.side_colour(&mut app);
        app.apply();
        let Command::Write { reports, state: Some(saved) } = commands.try_recv().unwrap() else { panic!("apply") };
        let bytes: Vec<_> = reports.iter().filter(|r| r[0] == 12).flat_map(|r| r[8..].to_vec()).collect();
        let modes: Vec<_> = reports.iter().filter(|r| r[0] == 81).flat_map(|r| r[8..].to_vec()).collect();
        assert_eq!(bytes.len(), 504);
        assert_eq!(modes.len(), 168);
        for i in 0..168 {
            let expected = match i {
                9 => ([255, 0, 0], 1),
                128..=157 => (rgb, 1),
                158 => ([0, 255, 0], 1),
                _ => ([0, 0, 0], 0),
            };
            assert_eq!((&bytes[i * 3..i * 3 + 3], modes[i]), (expected.0.as_slice(), expected.1), "LED {i}");
        }
        assert_eq!(saved.lighting_state().side_lighting.rgb, rgb);
    }
    #[test]
    fn erasing_selected_keys_keeps_other_keys_and_side_colours() {
        let (mut app, commands, _) = app();
        app.side_follow = false;
        let l = app.state.current_mut();
        l.colors[128..158].fill(KeyLight { rgb: [0, 0, 255], mode: 1 });
        for k in [9, 14, 158] {
            l.colors[k] = KeyLight { rgb: [255, 0, 0], mode: 1 };
        }
        app.applied = app.state.clone();
        app.sel = BTreeSet::from([9, 158]);
        let mut ui = Probe::new(&mut app);
        ui.click_text(&mut app, "Key paint");
        ui.erase(&mut app);
        app.apply();
        let Command::Write { reports, state: Some(saved) } = commands.try_recv().unwrap() else { panic!("apply") };
        let modes: Vec<_> = reports.iter().filter(|r| r[0] == 81).flat_map(|r| r[8..].to_vec()).collect();
        assert_eq!(modes[9], 0);
        assert_eq!(modes[158], 0);
        assert_eq!(modes[14], 1);
        assert!(modes[128..158].iter().all(|m| *m == 1));
        assert_eq!(saved.lighting_state().colors[14], KeyLight { rgb: [255, 0, 0], mode: 1 });
    }
    #[test]
    fn saved_independent_sides_do_not_become_linked_when_preset_values_match() {
        let (mut app, _, _) = app();
        let rgb = app.state.lighting_state().lighting.rgb;
        app.state.lighting_state_mut().colors[128..158].fill(KeyLight { rgb, mode: 1 });
        app.sync_layer_ui();
        assert!(!app.side_follow, "an independent side colour must remain independent after loading");
        app.state.lighting_state_mut().per_key_on = false;
        app.sync_layer_ui();
        assert!(!app.side_follow, "hidden remembered side paint must not become linked");
    }
    #[test]
    fn saved_profile_can_be_applied_without_editing_it_first() {
        let (mut app, commands, _) = app();
        app.applied.profiles[1].layers[0].lighting.brightness = 23;
        app.state = app.applied.clone();
        app.select_slot(1, 0);
        assert!(commands.try_recv().is_err(), "browsing is read-only");
        let mut ui = Probe::new(&mut app);
        ui.click_text(&mut app, "Apply to keyboard");
        let Command::Write { reports, state: Some(saved) } =
            commands.try_recv().expect("saved profile must be activatable with Apply")
        else {
            panic!("activation");
        };
        assert_eq!(&reports[0][..3], &[5, 1, 0]);
        assert!(reports.iter().any(|r| r[0] == 29), "restore the saved actuation mode");
        assert_eq!(reports.iter().find(|r| r[0] == 7).unwrap()[3], 5);
        assert_eq!(saved.profile, 1);
    }
    #[test]
    fn lighting_is_shared_by_function_layers_without_changing_layer_actuation() {
        let (mut app, commands, _) = app();
        app.state.profiles[0].layers[0].lighting.brightness = 23;
        app.state.profiles[0].layers[2].actuation.press = 1.;
        app.select_slot(0, 2);
        let ui = Probe::new(&mut app);
        assert!(ui.texts().iter().any(|(text, _)| text == "23%"), "Fn layers must show the same profile lighting");
        assert_eq!(app.state.current().actuation.press, 1.);
        app.select_slot(0, 0);
        assert_eq!(app.state.current().actuation.press, 2.);
        assert!(commands.try_recv().is_err());
    }
    #[test]
    fn choosing_single_color_preserves_rgb_and_sends_custom_color_mode() {
        let (mut app, commands, _) = app();
        let l = app.state.current_mut();
        l.per_key_on = false;
        l.lighting.effect = 1;
        l.lighting.color_mode = 9;
        l.lighting.rgb = [12, 34, 56];
        app.applied = app.state.clone();
        let mut ui = Probe::new(&mut app);
        ui.click_text(&mut app, "Wave #1");
        ui.click_text(&mut app, "Single Color");
        app.apply();
        let Command::Write { reports, .. } = commands.try_recv().unwrap() else {
            panic!("apply");
        };
        let main = reports.iter().find(|r| r[0] == 7).unwrap();
        assert_eq!((main[1], main[4] & 15), (7, 9));
        assert_eq!(&main[5..8], &[12, 34, 56]);
    }
    #[test]
    fn profile_browsing_remembers_each_profiles_selected_layer_without_writing() {
        let (mut app, commands, _) = app();
        app.select_slot(0, 2);
        app.select_profile(1);
        assert_eq!((app.state.profile, app.state.layer), (1, 0));
        app.select_slot(1, 1);
        app.select_profile(0);
        assert_eq!((app.state.profile, app.state.layer), (0, 2));
        app.select_profile(1);
        assert_eq!((app.state.profile, app.state.layer), (1, 1));
        assert_eq!(app.board_slot, Some((0, 0)));
        assert!(commands.try_recv().is_err());
    }
    #[test]
    fn activation_uses_saved_settings_from_other_screens_not_their_drafts() {
        let (mut app, commands, _) = app();
        app.select_slot(1, 2);
        app.state.current_mut().actuation.press = 1.7;
        app.state.lighting_state_mut().lighting.brightness = 73;
        app.apply();
        let Command::Write { reports, state: Some(saved) } = commands.try_recv().unwrap() else {
            panic!("apply");
        };
        assert_eq!(&reports[0][..3], &[5, 1, 2]);
        assert_eq!(saved.current().actuation.press, 2.);
        assert_eq!(reports.iter().find(|r| r[0] == 26).unwrap()[8], 19);
        assert_eq!(saved.lighting_state().lighting.brightness, 73);
        assert_eq!(app.state.current().actuation.press, 1.7);
    }
    #[test]
    fn layer_activation_restores_actuation_without_replacing_profile_lighting() {
        let (mut app, commands, events) = app();
        app.applied.profiles[0].layers[2].actuation.press = 1.3;
        app.state = app.applied.clone();
        app.screen = Screen::Keymap;
        app.select_slot(0, 2);
        app.apply();
        let Command::Write { reports, state: Some(saved) } = commands.try_recv().unwrap() else {
            panic!("apply layer");
        };
        assert_eq!(reports.iter().map(|r| r[0]).collect::<Vec<_>>(), [5, 29, 26]);
        assert_eq!(reports[2][1], 32);
        assert_eq!(reports[2][8], 12);
        assert_eq!(saved.profiles[0].selected_layer, 2);
        events.send(Event::ActiveSlot { profile: 0, layer: 2 }).unwrap();
        events.send(Event::Saved(saved)).unwrap();
        events.send(Event::Done).unwrap();
        app.poll();
        assert_eq!(app.board_slot, Some((0, 2)));
        assert!(!app.needs_apply());
    }
    #[test]
    fn legacy_multicolour_single_color_is_corrected_as_an_unapplied_draft() {
        let (mut app, commands, _) = app();
        app.state.lighting_state_mut().lighting.color_mode = 8;
        app.applied = app.state.clone();
        let _ui = Probe::new(&mut app);
        assert_eq!(app.state.lighting_state().lighting.color_mode, 9);
        assert_eq!(app.applied.lighting_state().lighting.color_mode, 8);
        assert!(app.dirty());
        assert!(commands.try_recv().is_err());
    }
}

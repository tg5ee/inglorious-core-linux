//! Omarchy theme integration. Colours come from the active theme's `colors.toml`
//! (~/.local/state/omarchy/current/theme) and are re-read when the theme changes.
//! Every UI colour is a mix of the theme background and foreground, the same way the
//! Omarchy shell tints its controls; without a theme the Figma palette is reproduced.
use eframe::egui::{self, Color32, Stroke};
use std::{
    cell::Cell,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, SystemTime},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub sunken: Color32,
    pub surface1: Color32,
    pub surface2: Color32,
    pub surface3: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub toggle_off: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent_ink: Color32,
    pub ok: Color32,
    pub danger: Color32,
}
/// Figma palette (j70ge8ICVlsLKIjCpWPGy2), used when no Omarchy theme is present.
const FIGMA: [u32; 5] = [0x080808, 0xebebeb, 0xf5a43a, 0x5fc656, 0xc64646];

impl Palette {
    pub fn derive(dark: bool, bg: Color32, fg: Color32, accent: Color32, ok: Color32, danger: Color32) -> Self {
        let m = |t: f32| mix(bg, fg, t);
        let surface1 = m(0.048);
        // Small secondary text stays at WCAG AA (4.5:1) against cards.
        let mut t = 0.5;
        while t < 0.9 && contrast(m(t), surface1) < 4.5 {
            t += 0.02;
        }
        let accent_ink = if contrast(bg, accent) >= contrast(fg, accent) { bg } else { fg };
        Self {
            dark,
            bg,
            panel: m(0.062),
            sunken: m(0.009),
            surface1,
            surface2: m(0.079),
            surface3: m(0.123),
            border: m(0.132),
            border_strong: m(0.185),
            toggle_off: m(0.176),
            text: fg,
            muted: m(0.705),
            dim: m(t),
            accent,
            accent_ink,
            ok,
            danger,
        }
    }
    pub fn figma() -> Self {
        let [bg, fg, accent, ok, danger] = FIGMA.map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
        Self::derive(true, bg, fg, accent, ok, danger)
    }
}

/// The active Omarchy theme: its palette (None without a theme) plus the extras themes ship.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub palette: Option<Palette>,
    pub name: Option<String>,
    /// `keyboard.rgb`: the colour the theme suggests for keyboard backlights.
    pub keyboard: Option<[u8; 3]>,
    /// `accent` from colors.toml: what theme sync puts on the keyboard.
    pub accent: Option<[u8; 3]>,
}
impl Theme {
    pub fn load() -> Self {
        let dir = theme_dir();
        let read = |f: &str| dir.as_ref().and_then(|d| std::fs::read_to_string(d.join(f)).ok());
        let colors = read("colors.toml");
        let palette = colors.as_deref().and_then(parse_colors);
        let accent = colors.as_deref().and_then(glorious_core_rs::state::theme_accent);
        let keyboard = read("keyboard.rgb").and_then(|t| hex(t.trim())).map(|c| [c.r(), c.g(), c.b()]);
        let name = dir.and_then(|d| std::fs::read_to_string(d.with_file_name("theme.name")).ok()).map(|n| title(n.trim()));
        Self { palette, name, keyboard, accent }
    }
    /// Palette for the chosen colours; Omarchy falls back to amber when no theme is present.
    pub fn palette(&self, colours: Colours) -> Palette {
        match (colours, self.palette) {
            (Colours::Omarchy, Some(p)) => p,
            _ => Palette::figma(),
        }
    }
}
/// The user's colour choice; only colours differ between the two, never layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colours {
    Omarchy,
    /// The Glorious amber palette from the Figma design.
    Amber,
}
impl Colours {
    fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("glorious-core-rs/colours"))
    }
    fn parse(text: &str) -> Self {
        if text.trim() == "amber" { Self::Amber } else { Self::Omarchy }
    }
    pub fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).map_or(Self::Omarchy, |t| Self::parse(&t))
    }
    pub fn save(self) -> std::io::Result<()> {
        let path = Self::path().ok_or_else(|| std::io::Error::other("no config directory"))?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, if self == Self::Amber { "amber\n" } else { "omarchy\n" })
    }
}
fn theme_dir() -> Option<PathBuf> {
    Some(dirs::state_dir()?.join("omarchy/current/theme"))
}
/// "tokyo-night" -> "Tokyo Night", as `omarchy theme current` prints it.
fn title(slug: &str) -> String {
    slug.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
/// Omarchy v4 colors.toml: flat `key = "#rrggbb"` pairs plus `mode = "dark" | "light"`.
pub fn parse_colors(text: &str) -> Option<Palette> {
    let get = |key: &str| {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
        })
    };
    let color = |key: &str| get(key).and_then(|v| hex(&v));
    let (bg, fg) = (color("background")?, color("foreground")?);
    let dark = get("mode").map_or(luminance(bg) < 0.5, |m| m != "light");
    let figma = Palette::figma();
    Some(Palette::derive(
        dark,
        bg,
        fg,
        color("accent").unwrap_or(figma.accent),
        color("green").unwrap_or(figma.ok),
        color("red").unwrap_or(figma.danger),
    ))
}
fn hex(s: &str) -> Option<Color32> {
    let s = s.strip_prefix('#')?;
    let v = u32::from_str_radix(s.get(..6)?, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Polls the theme every 2 s on its own thread and wakes the UI only when it changed,
/// so an idle window draws no frames.
pub fn watch(ctx: egui::Context) -> Receiver<Theme> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let path = theme_dir().map(|d| d.join("colors.toml"));
        let stamp = || -> Option<SystemTime> { std::fs::metadata(path.as_ref()?).ok()?.modified().ok() };
        let mut last = stamp();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = stamp();
            // A theme switch rewrites the directory; skip the moment the file is missing.
            if now.is_some() && now != last {
                last = now;
                if tx.send(Theme::load()).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        }
    });
    rx
}

thread_local! {
    static PAL: Cell<Palette> = Cell::new(Palette::figma());
}
/// Current palette (UI thread).
pub fn pal() -> Palette {
    PAL.with(Cell::get)
}
/// Installs `p` for the widgets and egui's own controls (combo boxes, text edits, tooltips).
pub fn apply(ctx: &egui::Context, p: Palette) {
    PAL.with(|c| c.set(p));
    let theme = if p.dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |s| {
        let v = &mut s.visuals;
        *v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = p.bg;
        v.window_fill = p.surface2;
        v.window_stroke = Stroke::new(1.0_f32, p.border_strong);
        v.extreme_bg_color = p.sunken;
        v.faint_bg_color = p.surface1;
        v.override_text_color = None;
        v.selection.bg_fill = p.accent.gamma_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0_f32, p.accent);
        v.text_cursor.stroke = Stroke::new(2.0_f32, p.accent);
        for (w, fill, stroke, fg) in [
            (&mut v.widgets.noninteractive, p.surface1, p.border, p.muted),
            (&mut v.widgets.inactive, p.surface2, p.border, p.muted),
            (&mut v.widgets.hovered, p.surface3, p.border_strong, p.text),
            (&mut v.widgets.active, p.surface3, p.accent, p.text),
            (&mut v.widgets.open, p.surface3, p.border_strong, p.text),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0_f32, stroke);
            w.fg_stroke = Stroke::new(1.0_f32, fg);
            w.corner_radius = 8.into();
        }
    });
}

/// sRGB channel mix: `t` = 0 is `a`, 1 is `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
/// WCAG relative luminance.
pub fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}
/// WCAG contrast ratio, 1..21.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    const TREES: &str = "mode = \"dark\"\naccent = \"#458f82\"\nbackground = \"#040904\"\nforeground = \"#daffbd\"\nred = \"#919c6b\"\ngreen = \"#9ed2a3\"\n";
    const LATTE: &str = "mode = \"light\"\naccent = \"#1e66f5\"\nbackground = \"#eff1f5\"\nforeground = \"#4c4f69\"\n";

    #[test]
    fn figma_palette_is_reproduced_without_a_theme() {
        let p = Palette::figma();
        let rgb = |c: Color32| [c.r(), c.g(), c.b()];
        assert_eq!(rgb(p.surface1), [0x13; 3]);
        assert_eq!(rgb(p.surface2), [0x1a; 3]);
        assert_eq!(rgb(p.surface3), [0x24; 3]);
        assert_eq!(rgb(p.border), [0x26; 3]);
        assert_eq!(rgb(p.border_strong), [0x32; 3]);
        assert_eq!(rgb(p.muted), [0xa8; 3]);
        assert_eq!(rgb(p.accent_ink), [0x08; 3]);
    }
    #[test]
    fn omarchy_themes_parse_and_keep_small_text_readable() {
        let dark = parse_colors(TREES).unwrap();
        assert!(dark.dark && dark.accent == Color32::from_rgb(0x45, 0x8f, 0x82));
        assert_eq!(dark.danger, Color32::from_rgb(0x91, 0x9c, 0x6b));
        let light = parse_colors(LATTE).unwrap();
        assert!(!light.dark && light.ok == Palette::figma().ok);
        assert!(parse_colors("accent = \"#123456\"").is_none());
        // Every installed stock theme, when present on this machine.
        let stock = std::fs::read_dir("/usr/share/omarchy/themes").into_iter().flatten().flatten();
        for p in [dark, light].into_iter().chain(stock.filter_map(|e| parse_colors(&std::fs::read_to_string(e.path().join("colors.toml")).ok()?))) {
            assert!(contrast(p.dim, p.surface1) >= 4.5 || contrast(p.text, p.bg) < 7., "dim text too faint: {p:?}");
            assert!(contrast(p.accent_ink, p.accent) >= 3., "unreadable button label: {p:?}");
        }
    }
    #[test]
    fn colour_choice_only_swaps_the_palette() {
        let theme = Theme { palette: parse_colors(TREES), name: Some("Trees".into()), keyboard: Some([0x45, 0x8f, 0x82]), accent: None };
        assert_eq!(theme.palette(Colours::Amber), Palette::figma());
        assert_eq!(theme.palette(Colours::Omarchy), parse_colors(TREES).unwrap());
        let none = Theme { palette: None, name: None, keyboard: None, accent: None };
        assert_eq!(none.palette(Colours::Omarchy), Palette::figma());
        assert_eq!((Colours::parse("amber\n"), Colours::parse("omarchy"), Colours::parse("")), (Colours::Amber, Colours::Omarchy, Colours::Omarchy));
    }
    #[test]
    fn contrast_and_title() {
        assert!((contrast(Color32::WHITE, Color32::BLACK) - 21.).abs() < 0.01);
        assert_eq!(title("tokyo-night"), "Tokyo Night");
        assert_eq!(hex("#FFFCF0"), Some(Color32::from_rgb(0xff, 0xfc, 0xf0)));
    }
}

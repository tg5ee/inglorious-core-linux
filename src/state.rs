use crate::protocol::{Actuation, KeyLight, Lighting, MacroEvent};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub lighting: Lighting,
    pub side_lighting: Lighting,
    pub colors: Vec<KeyLight>,
    /// Per-key colours shown on the board; off = preset effect on every key, colours kept here.
    pub per_key_on: bool,
    pub actuation: Actuation,
    pub per_key: BTreeMap<u8, Actuation>,
    pub bindings: BTreeMap<u8, [[u8; 4]; 4]>,
    pub advanced: BTreeMap<u8, Advanced>,
}
impl Layer {
    /// Theme sync: the base effect takes `rgb` in single-colour mode, and side lights that follow
    /// the keyboard come along. Effects whose colours the firmware owns are left alone.
    pub fn follow_theme(&mut self, rgb: [u8; 3]) -> bool {
        if crate::tables::MULTICOLOUR_ONLY.contains(&self.lighting.effect) {
            return false;
        }
        let (old, side) = (self.lighting.clone(), self.side_lighting == self.lighting);
        (self.lighting.rgb, self.lighting.color_mode) = (rgb, crate::tables::SINGLE_COLOUR);
        if side {
            self.side_lighting = self.lighting.clone();
        }
        self.lighting != old
    }
    /// The per-key array as the board should show it.
    pub fn shown_colors(&self) -> Vec<KeyLight> {
        let mut c = self.colors.clone();
        if !self.per_key_on {
            c.iter_mut().for_each(|k| k.mode = 0);
        }
        c
    }
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            lighting: Lighting::default(),
            side_lighting: Lighting::default(),
            colors: vec![KeyLight::default(); 168],
            per_key_on: true,
            actuation: Actuation::default(),
            per_key: BTreeMap::new(),
            bindings: BTreeMap::new(),
            advanced: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Advanced {
    Dks {
        first: f32,
        second: f32,
        modes: [u8; 4],
    },
    ModTap {
        timeout: u16,
    },
    Toggle {
        kind: u8,
    },
    Macro {
        id: u8,
        repeat: u8,
        events: Vec<MacroEvent>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub layers: [Layer; 3],
    pub selected_layer: u8,
    pub polling: u16,
    pub debounce: u16,
    pub sleep: u16,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| Layer::default()),
            selected_layer: 0,
            polling: 1000,
            debounce: 5,
            sleep: 0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub profile: u8,
    pub layer: u8,
    pub profiles: [Profile; 3],
}
impl Default for State {
    fn default() -> Self {
        Self {
            profile: 0,
            layer: 0,
            profiles: std::array::from_fn(|_| Profile::default()),
        }
    }
}
impl State {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.profile < 3 && self.layer < 3,
            "invalid saved profile/layer"
        );
        for p in &self.profiles {
            ensure!(p.selected_layer < 3, "invalid remembered layer");
            crate::protocol::report_rate(0, p.polling)?;
            crate::protocol::debounce(p.debounce)?;
            for l in &p.layers {
                ensure!(l.colors.len() == 168, "invalid color state");
                crate::protocol::lighting(&l.lighting, false)?;
                crate::protocol::lighting(&l.side_lighting, true)?;
                crate::protocol::per_key_lighting(&l.colors)?;
                crate::protocol::actuation(0, 0, None, &l.actuation)?;
                for (key, codes) in &l.bindings {
                    ensure!(*key < 90, "invalid binding physical key");
                    crate::protocol::complex_keystroke(0, 0, *key, *codes)?;
                }
                for (key, advanced) in &l.advanced {
                    ensure!(*key < 90, "invalid advanced physical key");
                    match advanced {
                        Advanced::Dks {
                            first,
                            second,
                            modes,
                        } => {
                            crate::protocol::dynamic_keystroke(
                                0,
                                0,
                                *key,
                                Some(*first),
                                Some(*second),
                                *modes,
                            )?;
                        }
                        Advanced::ModTap { timeout } => {
                            crate::protocol::mod_tap(0, 0, *key, *timeout)?;
                        }
                        Advanced::Toggle { kind } => {
                            crate::protocol::toggle(0, 0, *key, *kind)?;
                        }
                        Advanced::Macro { id, repeat, events } => {
                            crate::protocol::macro_keycode(*id, *repeat)?;
                            crate::protocol::macro_reports(*id, 1, events)?;
                        }
                    }
                }
                for (k, a) in &l.per_key {
                    ensure!(*k < 90, "invalid physical key");
                    crate::protocol::actuation(0, 0, Some(*k), a)?;
                }
            }
        }
        Ok(())
    }
    /// `Layer::follow_theme` on every profile's lighting; true when any of them changed.
    pub fn follow_theme(&mut self, rgb: [u8; 3]) -> bool {
        self.profiles.iter_mut().fold(false, |changed, p| p.layers[0].follow_theme(rgb) | changed)
    }
    /// CORE stores lighting per profile. Keep the base-layer storage for file compatibility.
    pub fn lighting_state(&self) -> &Layer {
        &self.profiles[self.profile as usize].layers[0]
    }
    pub fn lighting_state_mut(&mut self) -> &mut Layer {
        &mut self.profiles[self.profile as usize].layers[0]
    }
    pub fn current(&self) -> &Layer {
        &self.profiles[self.profile as usize].layers[self.layer as usize]
    }
    pub fn current_mut(&mut self) -> &mut Layer {
        &mut self.profiles[self.profile as usize].layers[self.layer as usize]
    }
}
pub fn actuation_reports(s: &State) -> Result<Vec<crate::protocol::Report>> {
    use crate::protocol as p;
    s.validate()?;
    let l = s.current();
    let mut r = vec![
        p::custom_actuation(),
        p::actuation(s.profile, s.layer, None, &l.actuation)?,
    ];
    for (k, a) in &l.per_key {
        let mut a = a.clone();
        a.deadzone = None; // Handler omits per-key deadzone, so firmware gets 40.
        r.push(p::actuation(s.profile, s.layer, Some(*k), &a)?);
    }
    Ok(r)
}
pub fn activate_reports(s: &State) -> Result<Vec<crate::protocol::Report>> {
    use crate::protocol as p;
    s.validate()?;
    let mut r = vec![p::set_profile(s.profile, s.layer)?];
    r.extend(actuation_reports(s)?);
    r.push(p::lighting(&s.lighting_state().lighting, false)?);
    r.push(p::lighting(&s.lighting_state().side_lighting, true)?);
    // Explicit restore must also clear any overrides left on the board.
    r.extend(p::per_key_lighting(&s.lighting_state().shown_colors())?);
    Ok(r)
}
/// Lighting reports for the current profile, as the original's lighting onSave sends them:
/// the preset (keyboard + side) when it changed, then the per-key arrays when they changed.
pub fn lighting_changes(new: &State, old: &State) -> Result<Vec<crate::protocol::Report>> {
    use crate::protocol as p;
    let a = new.lighting_state();
    let b = &old.profiles[new.profile as usize].layers[0];
    let mut r = Vec::new();
    if a.lighting != b.lighting || a.side_lighting != b.side_lighting {
        r.push(p::lighting(&a.lighting, false)?);
        r.push(p::lighting(&a.side_lighting, true)?);
    }
    if a.per_key_on != b.per_key_on || a.shown_colors() != b.shown_colors() {
        r.extend(p::per_key_lighting(&a.shown_colors())?);
    }
    Ok(r)
}
/// Performance reports for the selected profile, in the original's setPerformance order
/// (polling rate, input latency, idle time), only for the fields that changed.
pub fn performance_changes(new: &State, old: &State) -> Result<Vec<crate::protocol::Report>> {
    use crate::protocol as p;
    let (a, b) = (&new.profiles[new.profile as usize], &old.profiles[new.profile as usize]);
    let mut r = Vec::new();
    if a.polling != b.polling {
        r.push(p::report_rate(new.profile, a.polling)?);
    }
    if a.debounce != b.debounce {
        r.push(p::debounce(a.debounce)?);
    }
    if a.sleep != b.sleep {
        r.push(p::set_sleep(a.sleep));
    }
    Ok(r)
}
/// KeyCodeMapping entry for a physical key, as #clearHEBindings restores it.
pub fn default_code(key: u8) -> Result<u8> {
    use crate::tables::{KEYCODES, PHYSICAL_KEYS};
    PHYSICAL_KEYS
        .iter()
        .filter(|(_, i)| *i == key)
        .find_map(|(n, _)| KEYCODES.iter().find(|(c, _)| c == n).map(|(_, v)| *v))
        .ok_or_else(|| anyhow::anyhow!("physical key {key} has no default keycode"))
}
/// Reports that put one key of the current layer into its saved binding/advanced state,
/// or back to its default keycode when it has neither.
pub fn key_reports(s: &State, key: u8) -> Result<Vec<crate::protocol::Report>> {
    use crate::protocol as p;
    let (pi, li, l) = (s.profile, s.layer, s.current());
    let default = [[0, 0, default_code(key).unwrap_or(0), 0], [0; 4], [0; 4], [0; 4]];
    let codes = l.bindings.get(&key).copied();
    let mut r = match l.advanced.get(&key) {
        Some(Advanced::Macro { id, repeat, events }) => {
            let mut r = p::macro_reports(*id, 1, events)?;
            r.push(p::keystroke(key, pi, li, 0, p::macro_keycode(*id, *repeat)?)?);
            r
        }
        Some(_) => p::complex_keystroke(pi, li, key, codes.unwrap_or(default))?,
        None => {
            if codes.is_none() {
                default_code(key)?;
            }
            p::complex_keystroke(pi, li, key, codes.unwrap_or(default))?
        }
    };
    r.push(match l.advanced.get(&key) {
        Some(Advanced::Dks { first, second, modes }) => {
            p::dynamic_keystroke(pi, li, key, Some(*first), Some(*second), *modes)?
        }
        Some(Advanced::ModTap { timeout }) => p::mod_tap(pi, li, key, *timeout)?,
        Some(Advanced::Toggle { kind }) => p::toggle(pi, li, key, *kind)?,
        Some(Advanced::Macro { .. }) | None => p::keystroke_clear(pi, li, key)?,
    });
    Ok(r)
}
/// Reports for every key on `new`'s current layer whose binding or advanced function
/// differs from the same layer in `old`.
pub fn key_changes(new: &State, old: &State) -> Result<Vec<crate::protocol::Report>> {
    new.validate()?;
    let a = new.current();
    let b = &old.profiles[new.profile as usize].layers[new.layer as usize];
    let keys: BTreeSet<u8> = [&a.bindings, &b.bindings]
        .into_iter()
        .flat_map(|m| m.keys())
        .chain([&a.advanced, &b.advanced].into_iter().flat_map(|m| m.keys()))
        .copied()
        .collect();
    let mut r = Vec::new();
    for k in keys {
        if a.bindings.get(&k) != b.bindings.get(&k) || a.advanced.get(&k) != b.advanced.get(&k) {
            r.extend(key_reports(new, k)?);
        }
    }
    Ok(r)
}
/// `accent = "#rrggbb"` from an Omarchy v4 colors.toml.
pub fn theme_accent(colors_toml: &str) -> Option<[u8; 3]> {
    let v = colors_toml.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "accent").then(|| v.trim().trim_matches('"').trim_start_matches('#').to_owned())
    })?;
    let byte = |i: usize| u8::from_str_radix(v.get(i..i + 2)?, 16).ok();
    (v.len() == 6).then_some([byte(0)?, byte(2)?, byte(4)?])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_sync_recolours_colour_effects_only() {
        assert_eq!(theme_accent("mode = \"dark\"\naccent = \"#458f82\"\n"), Some([0x45, 0x8f, 0x82]));
        assert_eq!(theme_accent("accent = \"#45\""), None);
        let mut s = State::default();
        s.profiles[0].layers[0].lighting.color_mode = crate::tables::MULTICOLOUR;
        s.profiles[0].layers[0].side_lighting = s.profiles[0].layers[0].lighting.clone();
        s.profiles[1].layers[0].lighting.effect = 0; // Glorious Mode: firmware colours
        s.profiles[2].layers[0].side_lighting.rgb = [255, 0, 0]; // independent side lights
        assert!(s.follow_theme([1, 2, 3]));
        let l = &s.profiles[0].layers[0];
        assert_eq!((l.lighting.rgb, l.lighting.color_mode), ([1, 2, 3], crate::tables::SINGLE_COLOUR));
        assert_eq!(l.side_lighting, l.lighting);
        assert_eq!(s.profiles[1].layers[0].lighting.rgb, Lighting::default().rgb);
        assert_eq!(s.profiles[2].layers[0].side_lighting.rgb, [255, 0, 0]);
        assert!(!s.follow_theme([1, 2, 3]), "same accent again changes nothing");
    }
    #[test]
    fn profile_lighting_is_layer_independent_and_preserves_legacy_layer_data() {
        let mut s = State::default();
        s.lighting_state_mut().lighting.rgb = [12, 34, 56];
        s.profiles[0].layers[2].lighting.rgb = [90, 80, 70];
        s.layer = 2;
        assert_eq!(s.lighting_state().lighting.rgb, [12, 34, 56]);
        s.lighting_state_mut().colors[9] = KeyLight {rgb:[255,0,0], mode:1};
        s.layer = 1;
        assert_eq!(s.lighting_state().colors[9].mode, 1);
        let restored: State = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.profiles[0].layers[2].lighting.rgb, [90, 80, 70]);
        s.profiles[2].selected_layer = 3;
        assert!(s.validate().is_err());
    }
    #[test]
    fn reapply_always_restores_the_complete_shown_color_layer() {
        for per_key_on in [false, true] {
            for painted in [false, true] {
                let mut s = State::default();
                s.current_mut().per_key_on = per_key_on;
                if painted {
                    s.current_mut().colors[9] = KeyLight { rgb: [255, 0, 0], mode: 1 };
                }
                let reports = activate_reports(&s).unwrap();
                let modes: Vec<_> = reports.iter().filter(|r| r[0] == 81).collect();
                assert_eq!(modes.len(), 3, "restore must include mode pages even for preset-only lighting");
                for (page, r) in modes.iter().enumerate() {
                    for (offset, mode) in r[8..].iter().enumerate() {
                        assert_eq!(*mode, u8::from(per_key_on && painted && page * 56 + offset == 9));
                    }
                }
                assert_eq!(reports.iter().filter(|r| r[0] == 12).count(), 9);
            }
        }
    }
    #[test]
    fn key_changes_cover_remap_reset_and_macro() {
        let old = State::default();
        let mut new = old.clone();
        // Caps (physical 3) -> Escape: 4 keystroke slots then a clear.
        new.current_mut().bindings.insert(3, [[0, 0, 41, 0], [0; 4], [0; 4], [0; 4]]);
        let r = key_changes(&new, &old).unwrap();
        assert_eq!(r.len(), 5);
        assert_eq!((r[0][0], r[0][2], r[0][8..12].to_vec()), (19, 3, vec![0, 0, 41, 0]));
        assert_eq!((r[4][0], r[4][6]), (26, 3));
        // Resetting restores KeyCodeMapping.CapsLock (57).
        let r = key_changes(&old, &new).unwrap();
        assert_eq!(r[0][8..12], [0, 0, 57, 0]);
        // Macro: macro pages first, then the macro keycode on slot 0, then a clear.
        new.current_mut().advanced.insert(
            3,
            Advanced::Macro { id: 2, repeat: 0, events: vec![MacroEvent { key: 4, press: true, delay: 5 }] },
        );
        let r = key_changes(&new, &old).unwrap();
        assert_eq!(r[0][0], 22);
        let n = r.len();
        assert_eq!((r[n - 2][0], r[n - 2][8..11].to_vec()), (19, vec![9, 0, 2]));
        assert_eq!(r[n - 1][0], 26);
        assert!(key_changes(&old, &old).unwrap().is_empty());
    }
    #[test]
    fn performance_changes_send_only_changed_fields() {
        let old = State::default();
        let mut new = old.clone();
        assert!(performance_changes(&new, &old).unwrap().is_empty());
        new.profiles[0].polling = 8000;
        let r = performance_changes(&new, &old).unwrap();
        assert_eq!((r.len(), r[0][..8].to_vec()), (1, vec![4, 0, 0, 0, 0, 0, 0, 251]));
        new.profiles[0].debounce = 10;
        new.profiles[0].sleep = 300;
        let r = performance_changes(&new, &old).unwrap();
        assert_eq!(r.iter().map(|r| r[0]).collect::<Vec<_>>(), [4, 0x11, 0x12]);
    }
    #[test]
    fn lighting_changes_send_preset_then_per_key_like_on_save() {
        let old = State::default();
        let mut new = old.clone();
        assert!(lighting_changes(&new, &old).unwrap().is_empty());
        new.current_mut().lighting.effect = 1;
        let r = lighting_changes(&new, &old).unwrap();
        assert_eq!(r.iter().map(|r| r[0]).collect::<Vec<_>>(), [7, 8]);
        new.current_mut().colors[9] = KeyLight { rgb: [255, 0, 0], mode: 1 };
        let r = lighting_changes(&new, &old).unwrap();
        assert_eq!((r[0][0], r[1][0], r[2][0], r.len()), (7, 8, 12, 2 + 9 + 3));
        let mut only_keys = old.clone();
        only_keys.current_mut().colors = new.current().colors.clone();
        assert_eq!(lighting_changes(&only_keys, &old).unwrap()[0][0], 12);
        // Preset mode: per-key layer off keeps the colours but sends every mode byte as 0.
        let mut off = only_keys.clone();
        off.current_mut().per_key_on = false;
        let r = lighting_changes(&off, &only_keys).unwrap();
        assert_eq!(r.len(), 9 + 3);
        assert!(r[9..].iter().all(|m| m[8..64].iter().all(|b| *b == 0)));
        assert_eq!(off.current().colors[9].mode, 1);
    }
}

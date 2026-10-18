//! In-app preview of the firmware base effects. Nothing here talks to the keyboard: the board runs
//! the real effect itself, and Core ships no keyboard preview to copy (its keyframe previews are
//! for the V3 mice and A2 headset only). Each effect is drawn from Tom's description of the board
//! (2026-10-18), so treat these as approximations.
use super::theme::mix;
use eframe::egui::{Color32, Painter, Rect, ecolor::Hsva, vec2};
use glorious_core_rs::{
    protocol::Lighting,
    tables::{MULTICOLOUR_ONLY, SINGLE_COLOUR},
};
use std::f32::consts::TAU;

/// Preview cycles per second at speed 0 and at speed 100.
// ponytail: guessed, not timed against the board; adjust by eye if the preview runs fast or slow.
const RATE: (f32, f32) = (0.12, 1.1);
/// Seconds between the made-up key presses that drive the reactive effects.
const TAP: f32 = 0.9;
/// Key-row centres in board coordinates (0 = F-row, 1 = bottom row).
pub const ROWS: [f32; 6] = [0., 0.24, 0.43, 0.62, 0.81, 1.];
/// Width of the keybed over its height, so distances come out round.
const ASPECT: f32 = 3.;

/// Effects that change over time; the others are a still image.
pub fn animated(effect: u8) -> bool {
    !matches!(effect, 6 | 8 | 18)
}

/// Colour of the board at (x, y), both 0..=1 across the keybed, `t` seconds in. `off` is an unlit key.
pub fn sample(l: &Lighting, x: f32, y: f32, t: f32, off: Color32) -> Color32 {
    let multi = l.color_mode != SINGLE_COLOUR || MULTICOLOUR_ONLY.contains(&l.effect);
    let p = t * (RATE.0 + (RATE.1 - RATE.0) * l.speed as f32 / 100.);
    let band = |u: f32| 0.5 + 0.5 * (fract(u) * TAU).cos();
    // Waves: a rainbow in multicolour, a moving bright band in single colour.
    let wave = |u: f32| (u, if multi { 1. } else { band(u) });
    let key = hash(x * 31.7 + y * 7.3);
    let (hue, v) = match l.effect {
        8 => return off,
        // Glorious Mode: colour rings radiating from the centre.
        0 => (((x - 0.5) * ASPECT).hypot(y - 0.5) / ASPECT * 1.4 - p, 1.),
        1 => wave(x - p),
        // Wave #2: Wave #1 with bands twice as wide.
        3 => wave(x * 0.5 - p),
        4 => wave(((y - 0.5).atan2((x - 0.5) * ASPECT) / TAU) - p),
        // Acid Mode: the whole board fades from one solid colour to another.
        5 => {
            let f = fract(p);
            ((p.floor() + f * f * (3. - 2. * f)) * 0.382, 1.)
        }
        2 => (p.floor() * 0.13, band(p + 0.5)),
        6 => (x, 1.),
        // Rainbow: colour bars running from the top down.
        13 => (y - p, 1.),
        // Heartbeat Sensor: an ECG trace scrolling through the middle row.
        14 => {
            let u = fract(x - p);
            let g = |c: f32, w: f32| (-((u - c) / w).powi(2)).exp();
            let trace = 0.5 + 0.08 * g(0.25, 0.05) - 0.5 * g(0.5, 0.03) + 0.3 * g(0.56, 0.03);
            (x, (1. - (y - trace).abs() * 5.).max(0.))
        }
        11 => {
            let s = p * 1.5 + key * 7.;
            (key, if hash(s.floor() + key) > 0.7 { 1. - fract(s) } else { 0. })
        }
        12 => {
            let col = (x * 14.).floor();
            let d = fract(p * 0.8 + hash(col)) * 1.4 - y;
            (col / 14., if (0.0..0.5).contains(&d) { 1. - d * 2. } else { 0. })
        }
        15 => (key, if hash((p * 4.).floor() + key) > 0.65 { 1. } else { 0. }),
        17 => {
            let pos = 1. - (fract(p) * 2. - 1.).abs();
            (x, (1. - (x - pos).abs() * 10.).max(0.))
        }
        // Surmount: a still rainbow across the board.
        18 => (x, 1.),
        // Kamehemeha: two balls come in from the sides, circle each other in the middle, then part.
        16 => {
            let f = fract(p * 0.5);
            let (r, ang) = match f {
                f if f < 0.35 => (0.3 + (1. - f / 0.35) * 1.2, 0.),
                f if f < 0.65 => (0.3, (f - 0.35) / 0.3 * 2. * TAU),
                f => (0.3 + (f - 0.65) / 0.35 * 1.2, 0.),
            };
            // Distances in board heights (the board is ASPECT wide), so the orbit is round.
            let (ox, oy) = (r * ang.cos(), r * ang.sin());
            [(ox, oy, 0.), (-ox, -oy, 0.5)]
                .into_iter()
                .map(|(bx, by, h)| (h, (1. - ((x - 0.5) * ASPECT - bx).hypot(y - 0.5 - by) / 0.28).max(0.)))
                .fold((0., 0.), |b, c| if c.1 > b.1 { c } else { b })
        }
        7 | 9 | 10 => taps(t)
            .map(|(age, tx, ty, h)| {
                let a = age * (RATE.0 + (RATE.1 - RATE.0) * l.speed as f32 / 100.) * 3.;
                let dx = (x - tx) * ASPECT;
                let same_row = ((y - ty) * 10.).abs() < 0.5;
                let v = match l.effect {
                    // Ripple Graff: a ring spreading from the pressed key.
                    7 => (1. - ((dx.hypot(y - ty) - a) * 6.).abs()).max(0.) * (1. - a / 3.).max(0.),
                    // Pass Without Trace: the pressed key lights, then fades.
                    9 => f32::from(u8::from(dx.abs() < 0.08 && same_row)) * (1. - a).max(0.),
                    // Fast Run Without Trace: light runs both ways along the row.
                    _ => f32::from(u8::from(same_row)) * (1. - ((dx.abs() - a * 2.) * 4.).abs()).max(0.),
                };
                (h, v)
            })
            .fold((0., 0.), |b, c| if c.1 > b.1 { c } else { b }),
        _ => (x, 1.),
    };
    let lit = if multi { Hsva::new(fract(hue), 1., 1., 1.).into() } else { Color32::from_rgb(l.rgb[0], l.rgb[1], l.rgb[2]) };
    mix(off, lit, (v * l.brightness as f32 / 100.).clamp(0., 1.))
}

/// The effect on a 14 × 6 grid of cells, for the chip and the dropdown rows.
pub fn thumb(p: &Painter, r: Rect, l: &Lighting, t: f32, off: Color32) {
    let (cols, gap) = (14, 1.);
    let cell = vec2((r.width() + gap) / cols as f32 - gap, (r.height() + gap) / ROWS.len() as f32 - gap);
    for (j, y) in ROWS.iter().enumerate() {
        for i in 0..cols {
            let x = i as f32 / (cols - 1) as f32;
            let at = r.min + vec2(i as f32 * (cell.x + gap), j as f32 * (cell.y + gap));
            p.rect_filled(Rect::from_min_size(at, cell), 1, sample(l, x, *y, t, off));
        }
    }
}

/// The last few made-up key presses: (age in s, x, row, hue).
fn taps(t: f32) -> impl Iterator<Item = (f32, f32, f32, f32)> {
    let n = (t / TAP).floor();
    (0..3).map(move |i| {
        let k = n - i as f32;
        (t - k * TAP, hash(k), ROWS[(hash(k + 0.5) * 6.) as usize % 6], hash(k + 0.25))
    })
}
fn fract(v: f32) -> f32 {
    v.rem_euclid(1.)
}
/// Repeatable pseudo-random 0..1 from any number.
fn hash(n: f32) -> f32 {
    fract((n * 12.9898).sin() * 43_758.547)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_effect_previews_without_going_out_of_range() {
        let off = Color32::from_gray(30);
        for (_, e) in glorious_core_rs::tables::EFFECTS {
            for mode in [8, SINGLE_COLOUR] {
                let l = Lighting { effect: *e, color_mode: mode, brightness: 100, speed: 100, ..Default::default() };
                // Something lights up at some point, except "None".
                let lit = (0..200).any(|f| ROWS.iter().any(|y| (0..14).any(|i| sample(&l, i as f32 / 13., *y, f as f32 * 0.05, off) != off)));
                assert_eq!(lit, *e != 8, "effect {e} mode {mode}");
            }
        }
        // Zero brightness is an unlit board.
        let l = Lighting { effect: 1, brightness: 0, ..Default::default() };
        assert_eq!(sample(&l, 0.3, 0.5, 1., off), off);
    }
}

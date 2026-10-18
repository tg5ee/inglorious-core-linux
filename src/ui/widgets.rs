//! Widgets for the Figma design (file j70ge8ICVlsLKIjCpWPGy2, "Components" page), coloured by
//! the active Omarchy theme (theme.rs). Every interactive widget draws a focus ring for Tab
//! navigation; layout helpers wrap or scale instead of overflowing narrow tiles.
use super::theme::{contrast, mix, pal};
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, EventFilter, FontFamily, FontId, Key, Layout, Margin, Rect, Response,
    RichText, Sense, Stroke, StrokeKind, Ui,
    pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};
use glorious_core_rs::tables::PHYSICAL_KEYS;
use std::sync::Arc;

/// Fonts, text styles and spacing; colours are installed by `theme::apply`.
pub fn install(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::default();
    for (name, bytes) in [
        ("SpaceGrotesk-Medium", include_bytes!("../../assets/fonts/SpaceGrotesk-Medium.ttf").as_slice()),
        ("SpaceGrotesk-Bold", include_bytes!("../../assets/fonts/SpaceGrotesk-Bold.ttf").as_slice()),
        ("WorkSans-Medium", include_bytes!("../../assets/fonts/WorkSans-Medium.ttf").as_slice()),
        ("WorkSans-SemiBold", include_bytes!("../../assets/fonts/WorkSans-SemiBold.ttf").as_slice()),
        ("JetBrainsMono-Medium", include_bytes!("../../assets/fonts/JetBrainsMono-Medium.ttf").as_slice()),
        // Mark Cuda's MIT "Omarchy Font": the Omarchy wordmark lettering, other letters from Delta Corps Priest 1.
        ("OmarchyFont", include_bytes!("../../assets/fonts/OmarchyFont.ttf").as_slice()),
    ] {
        f.font_data.insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    }
    // egui's bundled fonts stay as fallbacks for glyphs the latin subsets lack (arrows etc.).
    let mut fallback = f.families[&FontFamily::Proportional].clone();
    fallback.push("Hack".into()); // ← → ● ▬
    f.families.get_mut(&FontFamily::Proportional).unwrap().push("Hack".into());
    for (family, font) in [
        (FontFamily::Proportional, "WorkSans-Medium"),
        (FontFamily::Monospace, "JetBrainsMono-Medium"),
        (FontFamily::Name("strong".into()), "WorkSans-SemiBold"),
        (FontFamily::Name("heading".into()), "SpaceGrotesk-Medium"),
        (FontFamily::Name("brand".into()), "SpaceGrotesk-Bold"),
        (FontFamily::Name("logo".into()), "OmarchyFont"),
    ] {
        let list = f.families.entry(family).or_insert_with(|| fallback.clone());
        list.insert(0, font.into());
    }
    egui_phosphor::add_to_fonts(&mut f, egui_phosphor::Variant::Regular);
    ctx.set_fonts(f);
    ctx.all_styles_mut(|s| {
        use egui::TextStyle::*;
        s.text_styles = [(Small, body(11.)), (Body, body(13.)), (Button, body(13.)), (Monospace, mono(13.)), (Heading, heading(16.))].into();
        s.interaction.selectable_labels = false;
        s.spacing.item_spacing = vec2(8., 8.);
        // egui's own controls (combo boxes) come out at CONTROL_H: 16 px of text + 2 × 8.
        s.spacing.button_padding = vec2(12., 8.);
    });
}

/// Typed stroke constructor; `Stroke::new(1., c)` hits the f32-fallback lint.
pub fn line(width: f32, c: Color32) -> Stroke {
    Stroke::new(width, c)
}
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn strong(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("strong".into()))
}
pub fn heading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("heading".into()))
}
pub fn brand(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("brand".into()))
}
/// The sidebar wordmark, in Omarchy's logo lettering.
pub fn logo(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("logo".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Single-line label; elided with "…" (full text on hover) rather than widening its container.
pub fn text(ui: &mut Ui, s: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.add(egui::Label::new(RichText::new(s).font(font).color(color)).truncate())
}
/// Wrapping hint or explanation text.
pub fn para(ui: &mut Ui, s: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.add(egui::Label::new(RichText::new(s).font(font).color(color)).wrap())
}
/// Letter-spaced caps label (brand, sidebar section titles).
pub fn spaced(ui: &mut Ui, s: &str, font: FontId, color: Color32, spacing: f32) {
    let mut job = LayoutJob::default();
    job.append(s, 0., TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() });
    ui.label(job);
}
/// Hover tint that reads on light and dark themes alike.
fn hover(c: Color32) -> Color32 {
    mix(c, pal().text, 0.08)
}
/// Accent ring just outside `rect` while the widget has keyboard focus.
fn focus_ring(ui: &Ui, resp: &Response, rect: Rect, radius: f32) {
    if resp.has_focus() {
        ui.painter().rect_stroke(rect.expand(2.), radius + 2., line(2., pal().accent), StrokeKind::Outside);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Seg(bool),
    Primary,
    Secondary,
    Ghost,
    Danger,
}
/// Height of buttons, combo boxes and single-line fields.
pub const CONTROL_H: f32 = 32.;
pub fn button(ui: &mut Ui, label: &str, kind: Kind) -> Response {
    button_w(ui, label, kind, None)
}
pub fn button_w(ui: &mut Ui, label: &str, kind: Kind, width: Option<f32>) -> Response {
    let c = pal();
    let (fill, border, ink) = match kind {
        Kind::Seg(true) | Kind::Primary => (c.accent, None, c.accent_ink),
        Kind::Seg(false) => (c.surface2, Some(c.border), c.muted),
        Kind::Secondary => (c.surface3, Some(c.border_strong), c.text),
        Kind::Ghost => (Color32::TRANSPARENT, None, c.muted),
        Kind::Danger => (Color32::TRANSPARENT, Some(c.danger), c.danger),
    };
    let font = if kind == Kind::Seg(false) { body(13.) } else { strong(13.) };
    // One control height everywhere; combo boxes match it through button_padding.
    let (h, pad) = (CONTROL_H, 14.);
    let galley = ui.painter().layout_no_wrap(label.into(), font.clone(), ink);
    let size = vec2(width.unwrap_or(galley.size().x + pad * 2.), h);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered() && ui.is_enabled();
        let (fill, ink) = match (hovered, fill == Color32::TRANSPARENT) {
            (false, _) => (fill, ink),
            (true, true) => (c.surface2, if kind == Kind::Danger { ink } else { c.text }),
            (true, false) => (hover(fill), if kind == Kind::Seg(false) { c.text } else { ink }),
        };
        let stroke = border.map_or(Stroke::NONE, |b| line(1., b));
        ui.painter().rect(rect, 8, fill, stroke, StrokeKind::Inside);
        let galley = ui.painter().layout_no_wrap(label.into(), font, ink);
        ui.painter().galley(rect.center() - galley.size() / 2., galley, ink);
        focus_ring(ui, &resp, rect, 8.);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}
/// Square ghost button holding one Phosphor glyph.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str) -> Response {
    let c = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(28., 28.), Sense::click());
    let hovered = resp.hovered() && ui.is_enabled();
    if hovered {
        ui.painter().rect_filled(rect, 6, c.surface2);
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, body(15.), if hovered { c.text } else { c.muted });
    focus_ring(ui, &resp, rect, 6.);
    resp.on_hover_text(tip).on_hover_cursor(CursorIcon::PointingHand)
}

/// Sidebar entry: indicator bar, icon and label; `dirty` adds an accent dot for unapplied
/// edits on that screen.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, active: bool, dirty: bool) -> Response {
    let c = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.), Sense::click());
    let p = ui.painter();
    if active || resp.hovered() {
        p.rect_filled(rect, 8, if active { c.surface3 } else { c.surface2 });
    }
    let bar = Rect::from_min_size(pos2(rect.left() + 12., rect.center().y - 8.), vec2(3., 16.));
    p.rect_filled(bar, 2, if active { c.accent } else { c.surface3 });
    let ink = if active { c.text } else { c.muted };
    p.text(pos2(bar.right() + 12., rect.center().y), Align2::LEFT_CENTER, icon, body(16.), ink);
    let font = if active { strong(14.) } else { body(14.) };
    p.text(pos2(bar.right() + 36., rect.center().y), Align2::LEFT_CENTER, label, font, ink);
    if dirty {
        p.circle_filled(pos2(rect.right() - 14., rect.center().y), 3.5, c.accent);
    }
    focus_ring(ui, &resp, rect, 8.);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Slider row: label + mono value, then a 6 px rail with a 16 px thumb. Arrow keys step it
/// while focused.
pub fn slider(ui: &mut Ui, label: &str, value: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, step: f32) -> Response {
    let c = pal();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 10.;
        ui.horizontal(|ui| {
            text(ui, label, body(13.), c.muted);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| text(ui, value, mono(13.), c.text));
        });
        let (track, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 16.), Sense::click_and_drag());
        let (min, max) = (*range.start(), *range.end());
        let snap = |nv: f32| ((nv / step).round() * step).clamp(min, max);
        let mut target = resp.interact_pointer_pos().map(|pos| {
            let t = ((pos.x - track.left() - 8.) / (track.width() - 16.)).clamp(0., 1.);
            snap(min + t * (max - min))
        });
        if resp.has_focus() {
            ui.memory_mut(|m| m.set_focus_lock_filter(resp.id, EventFilter { horizontal_arrows: true, ..Default::default() }));
            let d = ui.input(|i| i.key_pressed(Key::ArrowRight) as i8 - i.key_pressed(Key::ArrowLeft) as i8);
            if d != 0 {
                target = Some(snap(*v + d as f32 * step));
            }
        }
        if let Some(nv) = target
            && (nv - *v).abs() > step / 2.
        {
            *v = nv;
            resp.mark_changed();
        }
        let t = ((*v - min) / (max - min)).clamp(0., 1.);
        let x = track.left() + 8. + t * (track.width() - 16.);
        let p = ui.painter();
        let rail = Rect::from_center_size(track.center(), vec2(track.width(), 6.));
        p.rect_filled(rail, 3, c.surface3);
        p.rect_filled(Rect::from_min_max(rail.min, pos2(x, rail.bottom())), 3, c.accent);
        p.circle_filled(pos2(x, track.center().y), 8., c.text);
        focus_ring(ui, &resp, track, 8.);
        resp.on_hover_cursor(CursorIcon::PointingHand)
    })
    .inner
}
/// `slider` over an integer value.
pub fn slider_int<T: Copy + Into<f64> + TryFrom<i64>>(ui: &mut Ui, label: &str, value: &str, v: &mut T, range: std::ops::RangeInclusive<f32>, step: f32) -> Response {
    let mut f = (*v).into() as f32;
    let resp = slider(ui, label, value, &mut f, range, step);
    if resp.changed()
        && let Ok(n) = T::try_from(f.round() as i64)
    {
        *v = n;
    }
    resp
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let c = pal();
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40., 22.), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let p = ui.painter();
    p.rect_filled(rect, 11, mix(c.toggle_off, c.accent, t));
    p.circle_filled(pos2(rect.left() + 11. + 18. * t, rect.center().y), 8., mix(c.muted, c.accent_ink, t));
    focus_ring(ui, &resp, rect, 11.);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}
/// Label on the left, widget on the right.
/// The right-hand widget is laid out first so the label centres on the full row height.
pub fn split<R>(ui: &mut Ui, label: &str, right: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let r = right(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| text(ui, label, body(13.), pal().muted));
            r
        })
        .inner
    })
    .inner
}
pub fn kv(ui: &mut Ui, k: &str, v: &str, font: FontId) {
    split(ui, k, |ui| text(ui, v, font, pal().text));
}
/// `kv` whose value can be selected and copied.
pub fn kv_select(ui: &mut Ui, k: &str, v: &str, font: FontId) {
    split(ui, k, |ui| ui.add(egui::Label::new(RichText::new(v).font(font).color(pal().text)).selectable(true).truncate()));
}

pub fn swatch(ui: &mut Ui, c: Color32, selected: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let p = ui.painter();
    let r = size / 2.;
    p.circle_filled(rect.center(), r, c);
    if selected {
        p.circle_stroke(rect.center(), r - 1.5, line(3., pal().accent));
    } else {
        p.circle_stroke(rect.center(), r - 1., line(2., pal().border_strong));
    }
    focus_ring(ui, &resp, rect, r);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}
/// Round swatch that opens egui's colour picker; `icon` marks the custom-colour slot.
pub fn colour_button(ui: &mut Ui, value: &mut [u8; 3], size: f32, icon: Option<&str>) -> Response {
    let mut resp = swatch(ui, rgb(*value), false, size);
    if let Some(icon) = icon {
        ui.painter().text(resp.rect.center(), Align2::CENTER_CENTER, icon, body(size * 0.5), ink_on(rgb(*value)));
    }
    let mut c = rgb(*value);
    let changed = egui::Popup::from_toggle_button_response(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque))
        .is_some_and(|r| r.inner);
    if changed {
        *value = [c.r(), c.g(), c.b()];
        resp.mark_changed();
    }
    resp
}
/// "No colour" swatch: empty circle with a slash.
pub fn no_colour(ui: &mut Ui, size: f32) -> Response {
    let c = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let (p, m, r) = (ui.painter(), rect.center(), size / 2.);
    p.circle(m, r - 1., c.surface2, line(2., if resp.hovered() { c.muted } else { c.border_strong }));
    p.line_segment([m + vec2(-r * 0.57, r * 0.57), m + vec2(r * 0.57, -r * 0.57)], line(2., c.danger));
    focus_ring(ui, &resp, rect, r);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}
/// Status pill; `busy` swaps the dot for a spinner.
pub fn pill(ui: &mut Ui, dot: Color32, label: &str, busy: bool) {
    let c = pal();
    let g = ui.painter().layout_no_wrap(label.into(), body(12.), c.muted);
    let (rect, _) = ui.allocate_exact_size(vec2(g.size().x + 40., 26.), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 13, c.surface2);
    let at = pos2(rect.left() + 16., rect.center().y);
    if busy {
        egui::Spinner::new().size(12.).color(dot).paint_at(ui, Rect::from_center_size(at, vec2(12., 12.)));
    } else {
        p.circle_filled(at, 4., dot);
    }
    ui.painter().galley(pos2(rect.left() + 28., rect.center().y - g.size().y / 2.), g, c.muted);
}

/// Card frame; `fill` stretches it to the rect it was placed in.
pub fn card<R>(ui: &mut Ui, title: &str, sub: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_with(ui, 20, true, false, title, sub, add)
}
pub fn card_with<R>(ui: &mut Ui, pad: i8, fill: bool, danger: bool, title: &str, sub: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let c = pal();
    egui::Frame::new()
        .fill(c.surface1)
        .stroke(line(1., if danger { c.danger } else { c.border }))
        .corner_radius(12)
        .inner_margin(pad)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if fill {
                ui.set_min_height(ui.available_height());
            }
            ui.spacing_mut().item_spacing.y = 16.;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.;
                text(ui, title, heading(16.), if danger { c.danger } else { c.text });
                if let Some(sub) = sub {
                    text(ui, sub, body(12.), c.dim);
                }
            });
            add(ui)
        })
        .inner
}
/// Plain card frame without a header (keyboard canvas).
pub fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let c = pal();
    egui::Frame::new()
        .fill(c.surface1)
        .stroke(line(1., c.border))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}
/// Toolbar above the keyboard. `f(ui, false)` adds the left-hand chips, `f(ui, true)` the
/// right-aligned controls; below `inline` px the chips wrap and the controls get their own line.
pub fn toolbar(ui: &mut Ui, inline: f32, mut f: impl FnMut(&mut Ui, bool)) {
    let right = |ui: &mut Ui, f: &mut dyn FnMut(&mut Ui, bool)| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.;
            f(ui, true)
        });
    };
    if ui.available_width() >= inline {
        ui.horizontal(|ui| {
            // Full control height up front, so leading text centres on the buttons.
            ui.set_min_height(CONTROL_H);
            ui.spacing_mut().item_spacing.x = 6.;
            f(ui, false);
            right(ui, &mut f);
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6., 6.);
            f(ui, false)
        });
        ui.horizontal(|ui| right(ui, &mut f));
    }
}

const GAP: f32 = 20.;
const FLEX_MIN: f32 = 280.;
/// Card rects for one row of `widths` (0 = flexible, at least 280 px) inside `width`, 20 px
/// apart. A row too narrow for its cards wraps onto more lines, each `height` tall; a wrapped
/// line without a flexible card stretches its cards to the full width.
pub fn flow(width: f32, widths: &[f32], height: f32) -> Vec<Rect> {
    let min = |w: f32| if w == 0. { FLEX_MIN } else { w };
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.;
    for (i, w) in widths.iter().enumerate() {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && used + GAP + min(*w) > width {
            lines.push(vec![i]);
            used = min(*w);
        } else {
            used += if line.is_empty() { 0. } else { GAP } + min(*w);
            line.push(i);
        }
    }
    let mut out = vec![Rect::NOTHING; widths.len()];
    for (n, line) in lines.iter().enumerate() {
        let gaps = GAP * (line.len() - 1) as f32;
        let fixed: f32 = line.iter().map(|i| widths[*i]).sum();
        let flex = line.iter().filter(|i| widths[**i] == 0.).count();
        let scale = if flex == 0 && lines.len() > 1 { (width - gaps) / fixed } else { 1. };
        let flex_w = if flex > 0 { ((width - gaps - fixed) / flex as f32).max(0.) } else { 0. };
        let mut x = 0.;
        for i in line {
            let w = if widths[*i] == 0. { flex_w } else { widths[*i] * scale };
            out[*i] = Rect::from_min_size(pos2(x, n as f32 * (height + GAP)), vec2(w, height));
            x += w + GAP;
        }
    }
    out
}
/// Allocates a `flow` of cards and returns their screen rects.
pub fn row(ui: &mut Ui, widths: &[f32], height: f32) -> Vec<Rect> {
    let rects = flow(ui.available_width(), widths, height);
    let bottom = rects.iter().map(|r| r.bottom()).fold(0., f32::max);
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), bottom), Sense::hover());
    rects.into_iter().map(|r| r.translate(area.min.to_vec2())).collect()
}
pub fn place<R>(ui: &mut Ui, r: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), add).inner
}

/// Binding row in the "Changes / Bindings on this layer" cards.
pub fn list_item(ui: &mut Ui, key: &str, target: &str, extra: &str, selected: bool) -> Response {
    let c = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 33.), Sense::click());
    let p = ui.painter();
    let stroke = if selected { line(1., c.accent) } else { Stroke::NONE };
    p.rect(rect, 8, if resp.hovered() { c.surface3 } else { c.surface2 }, stroke, StrokeKind::Inside);
    let mut x = rect.left() + 12.;
    for (s, font, color) in [(key, strong(13.), c.text), ("→", body(13.), c.dim), (target, body(13.), c.muted)] {
        x = p.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, s, font, color).right() + 10.;
    }
    p.text(pos2(rect.right() - 12., rect.center().y), Align2::RIGHT_CENTER, extra, mono(11.), c.dim);
    focus_ring(ui, &resp, rect, 8.);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// DKS stage cell: 0 absent (ring), 1 fires once (dot), 2 held (bar).
pub fn stage_cell(ui: &mut Ui, mode: u8, width: f32) -> Response {
    let c = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 30.), Sense::click());
    let p = ui.painter();
    let m = rect.center();
    match mode {
        0 => {
            p.circle_stroke(m, 4.25, line(1.5, if resp.hovered() { c.muted } else { c.border_strong }));
        }
        1 => {
            p.circle_filled(m, 6., c.accent);
        }
        _ => {
            p.rect_filled(Rect::from_center_size(m, vec2(width - 16., 6.)), 3, c.accent);
        }
    }
    focus_ring(ui, &resp, rect.shrink(2.), 6.);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// (PHYSICAL_KEYS name, label, x, y, width in U) from the Figma keybed; U = 56 px including the 4 px gap.
const KEYBED: &[(&str, &str, f32, f32, f32)] = &[
    ("Escape", "Esc", 20., 20., 1.), ("F1", "F1", 90., 20., 1.), ("F2", "F2", 146., 20., 1.), ("F3", "F3", 202., 20., 1.),
    ("F4", "F4", 258., 20., 1.), ("F5", "F5", 328., 20., 1.), ("F6", "F6", 384., 20., 1.), ("F7", "F7", 440., 20., 1.),
    ("F8", "F8", 496., 20., 1.), ("F9", "F9", 566., 20., 1.), ("F10", "F10", 622., 20., 1.), ("F11", "F11", 678., 20., 1.),
    ("F12", "F12", 734., 20., 1.),
    ("Backquote", "`", 20., 90., 1.), ("Digit1", "1", 76., 90., 1.), ("Digit2", "2", 132., 90., 1.), ("Digit3", "3", 188., 90., 1.),
    ("Digit4", "4", 244., 90., 1.), ("Digit5", "5", 300., 90., 1.), ("Digit6", "6", 356., 90., 1.), ("Digit7", "7", 412., 90., 1.),
    ("Digit8", "8", 468., 90., 1.), ("Digit9", "9", 524., 90., 1.), ("Digit0", "0", 580., 90., 1.), ("Minus", "-", 636., 90., 1.),
    ("Equal", "=", 692., 90., 1.), ("Backspace", "Backspace", 748., 90., 2.), ("Delete", "Del", 860., 90., 1.),
    ("Tab", "Tab", 20., 146., 1.5), ("KeyQ", "Q", 104., 146., 1.), ("KeyW", "W", 160., 146., 1.), ("KeyE", "E", 216., 146., 1.),
    ("KeyR", "R", 272., 146., 1.), ("KeyT", "T", 328., 146., 1.), ("KeyY", "Y", 384., 146., 1.), ("KeyU", "U", 440., 146., 1.),
    ("KeyI", "I", 496., 146., 1.), ("KeyO", "O", 552., 146., 1.), ("KeyP", "P", 608., 146., 1.), ("BracketLeft", "[", 664., 146., 1.),
    ("BracketRight", "]", 720., 146., 1.), ("Backslash", "\\", 776., 146., 1.5), ("PageUp", "PgUp", 860., 146., 1.),
    ("CapsLock", "Caps", 20., 202., 1.75), ("KeyA", "A", 118., 202., 1.), ("KeyS", "S", 174., 202., 1.), ("KeyD", "D", 230., 202., 1.),
    ("KeyF", "F", 286., 202., 1.), ("KeyG", "G", 342., 202., 1.), ("KeyH", "H", 398., 202., 1.), ("KeyJ", "J", 454., 202., 1.),
    ("KeyK", "K", 510., 202., 1.), ("KeyL", "L", 566., 202., 1.), ("Semicolon", ";", 622., 202., 1.), ("Quote", "'", 678., 202., 1.),
    ("Enter", "Enter", 734., 202., 2.25), ("PageDown", "PgDn", 860., 202., 1.),
    ("ShiftLeft", "Shift", 20., 258., 2.25), ("KeyZ", "Z", 146., 258., 1.), ("KeyX", "X", 202., 258., 1.), ("KeyC", "C", 258., 258., 1.),
    ("KeyV", "V", 314., 258., 1.), ("KeyB", "B", 370., 258., 1.), ("KeyN", "N", 426., 258., 1.), ("KeyM", "M", 482., 258., 1.),
    ("Comma", ",", 538., 258., 1.), ("Period", ".", 594., 258., 1.), ("Slash", "/", 650., 258., 1.), ("ShiftRight", "Shift", 706., 258., 1.75),
    ("ArrowUp", "↑", 804., 258., 1.), ("End", "End", 860., 258., 1.),
    ("ControlLeft", "Ctrl", 20., 314., 1.25), ("MetaLeft", "Super", 90., 314., 1.25), ("AltLeft", "Alt", 160., 314., 1.25),
    ("Space", "", 230., 314., 6.25), ("AltRight", "Alt", 580., 314., 1.25), ("Fn", "Fn", 650., 314., 1.25),
    ("ArrowLeft", "←", 748., 314., 1.), ("ArrowDown", "↓", 804., 314., 1.), ("ArrowRight", "→", 860., 314., 1.),
];
/// HW colour index of the logo badge, and the physical number of the encoder knob.
pub const BADGE: u8 = 158;
pub const ENCODER: u8 = 84;

fn physical(name: &str) -> u8 {
    PHYSICAL_KEYS.iter().find(|(n, _)| *n == name).map(|(_, i)| *i).expect("KEYBED names are PHYSICAL_KEYS names")
}
/// (physical index, PHYSICAL_KEYS name) of every key cap.
pub fn keys() -> impl Iterator<Item = (u8, &'static str)> {
    KEYBED.iter().map(|(n, ..)| (physical(n), *n))
}
/// Centre of a key cap across the keybed, both 0..=1 (rows match effects::ROWS).
pub fn key_pos(idx: u8) -> Option<(f32, f32)> {
    KEYBED.iter().find(|(n, ..)| physical(n) == idx).map(|&(_, _, x, y, u)| ((x + u * 28. - 22.) / 892., (y - 20.) / 294.))
}
pub fn key_name(idx: u8) -> &'static str {
    KEYBED.iter().find(|(n, ..)| physical(n) == idx).map_or("EncoderClick", |(n, ..)| *n)
}
pub fn key_label(idx: u8) -> &'static str {
    match KEYBED.iter().find(|(n, ..)| physical(n) == idx) {
        Some((_, "", ..)) => "Space",
        Some((_, l, ..)) => l,
        None if idx == ENCODER => "Knob",
        None => "?",
    }
}

#[derive(Default)]
pub struct KeyLook {
    pub fill: Option<Color32>,
    pub value: Option<String>,
    pub selected: bool,
    pub raised: bool,
}
/// Keybed scale for `width`: the 932 px Figma keybed, shrunk to as little as 55 %. In a tall
/// window it may grow to 140 % while the screen still fits: `height` is the window height and
/// everything around the keybed (header, toolbar, the card row) takes about CHROME px.
pub fn keyboard_scale(width: f32, height: f32) -> f32 {
    const CHROME: f32 = 500.;
    let grow = ((height - CHROME) / 386.).clamp(1., 1.4);
    (width / 932.).clamp(0.55, grow)
}
/// The keybed, centred and scaled to the available width. Returns the clicked physical index
/// (or BADGE / ENCODER). Keys take clicks but not Tab focus, so Tab moves past the board.
/// Narrower than its smallest scale, the board scrolls sideways inside its card.
pub fn keyboard(ui: &mut Ui, look: &dyn Fn(u8) -> KeyLook) -> Option<u8> {
    let min = 932. * keyboard_scale(0., 0.);
    if ui.available_width() < min {
        return egui::ScrollArea::horizontal()
            .id_salt("keybed")
            .show(ui, |ui| {
                ui.set_width(min);
                keybed(ui, look)
            })
            .inner;
    }
    keybed(ui, look)
}
fn keybed(ui: &mut Ui, look: &dyn Fn(u8) -> KeyLook) -> Option<u8> {
    let c = pal();
    let s = keyboard_scale(ui.available_width(), ui.ctx().content_rect().height());
    let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), 386. * s), Sense::hover());
    let o = pos2(outer.center().x - 466. * s, outer.top());
    let at = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(o + vec2(x, y) * s, vec2(w, h) * s);
    let (label_font, value_font, radius) = (body((12. * s).max(10.)), mono((10. * s).max(9.)), 8. * s);
    let mut clicked = None;
    let mut hit = |ui: &mut Ui, r: Rect, idx: u8| {
        let resp = ui.interact(r, ui.id().with(("key", idx)), Sense::CLICK).on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() {
            clicked = Some(idx);
        }
        resp.hovered()
    };
    for &(name, label, x, y, u) in KEYBED {
        let idx = physical(name);
        let r = at(x, y, u * 56. - 4., 52.);
        let hovered = hit(ui, r, idx);
        let l = look(idx);
        let stroke = match (l.selected, hovered) {
            (true, _) => line(2., c.accent),
            (false, true) => line(1., c.muted),
            _ => line(1., c.border_strong),
        };
        let fill = l.fill.unwrap_or(if l.raised { c.surface3 } else { c.surface2 });
        let p = ui.painter();
        p.rect(r, radius, fill, stroke, StrokeKind::Inside);
        let ink = l.fill.map_or(c.text, ink_on);
        match &l.value {
            Some(v) => {
                p.text(r.center() - vec2(0., 7. * s), Align2::CENTER_CENTER, label, label_font.clone(), ink);
                p.text(r.center() + vec2(0., 8. * s), Align2::CENTER_CENTER, v, value_font.clone(), if l.fill.is_some() { ink } else { c.muted });
            }
            None => {
                p.text(r.center(), Align2::CENTER_CENTER, label, label_font.clone(), ink);
            }
        }
    }
    let badge = at(790., 20., 20., 52.);
    let hovered = hit(ui, badge, BADGE);
    let l = look(BADGE);
    let p = ui.painter();
    if l.selected || hovered {
        p.rect_stroke(badge, 6. * s, line(if l.selected { 2. } else { 1. }, if l.selected { c.accent } else { c.muted }), StrokeKind::Inside);
    }
    p.text(badge.center(), Align2::CENTER_CENTER, "G", brand(18. * s), l.fill.unwrap_or(c.accent));
    let knob = at(860., 20., 52., 52.);
    let hovered = hit(ui, knob, ENCODER);
    let l = look(ENCODER);
    let stroke = if l.selected { c.accent } else if hovered { c.muted } else { c.border_strong };
    let p = ui.painter();
    p.circle(knob.center(), 25. * s, c.surface3, line(2., stroke));
    if let Some(v) = &l.value {
        p.text(knob.center(), Align2::CENTER_CENTER, v, value_font, c.muted);
    }
    clicked
}
/// Near-black or near-white, whichever reads better on a lit key.
fn ink_on(fill: Color32) -> Color32 {
    let (dark, light) = (Color32::from_gray(12), Color32::from_gray(240));
    if contrast(fill, dark) >= contrast(fill, light) { dark } else { light }
}
pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_fit_at_full_width_and_wrap_when_narrow() {
        // Lighting row at the Figma width: one line, fixed cards keep their widths.
        let r = flow(1128., &[0., 360., 250.], 262.);
        assert_eq!(r.iter().map(|r| (r.left(), r.width(), r.top())).collect::<Vec<_>>(), [(0., 478., 0.), (498., 360., 0.), (878., 250., 0.)]);
        // Half-screen tile: the third card wraps and stretches across the second line.
        let r = flow(700., &[0., 360., 250.], 262.);
        assert_eq!((r[0].top(), r[1].top(), r[2].top()), (0., 0., 282.));
        assert_eq!((r[0].width(), r[2].width()), (320., 700.));
        // Nothing ever extends past the available width.
        for w in [360., 520., 700., 900., 1128., 1800.] {
            for widths in [&[0., 360., 250.][..], &[280., 0., 300.], &[0., 0., 260.], &[0., 0.]] {
                assert!(flow(w, widths, 100.).iter().all(|r| r.right() <= w + 0.01 && r.width() > 0.), "{w} {widths:?}");
            }
        }
    }
    #[test]
    fn keyboard_scales_with_width() {
        assert_eq!(keyboard_scale(1088., 800.), 1.);
        assert!((keyboard_scale(700., 900.) - 700. / 932.).abs() < 1e-6);
        assert_eq!(keyboard_scale(300., 900.), 0.55);
        // Tall windows let a wide keybed grow, never past 140 %.
        assert!((keyboard_scale(1200., 1000.) - 1200. / 932.).abs() < 1e-6);
        assert_eq!(keyboard_scale(1600., 1400.), 1.4);
    }
    #[test]
    fn lit_key_labels_stay_readable() {
        for c in [[0x1f, 0x4f, 0xff], [0x00, 0xe5, 0xff], [0xff, 0xff, 0xff], [0x00, 0xc8, 0x53], [0x7b, 0x2f, 0xf7]] {
            assert!(contrast(rgb(c), ink_on(rgb(c))) >= 4.5, "{c:?}");
        }
    }
}

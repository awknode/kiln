//! Kiln editor. Two faces over the same parameters:
//! - DIGITAL: Blackface look in amber. Spectrum (in / out, TAME's live cut hanging
//!   from the top, draggable TAME range), the type's curve + harmonics, six type
//!   tiles, variants, SATURATION + TAME panels, meters, footer.
//! - ANALOG: a worn rack unit. VU, glowing valves, stepped TYPE rotary, VARIANT
//!   push-buttons, chicken-head knobs, toggle switches and lamps.
//! The preset strip (presets, A/B, undo, power, MIX) sits on top of both.
//! Knobs: drag (Ctrl/Shift fine), scroll, double-click resets.

use crate::analyzer::Analyzer;
use crate::awk_ui::*;
use crate::params::*;
use crate::presets::PresetBar;
use crate::sat;
use crate::tame::{band_freq, BANDS};
use crate::{get, Shared};
use nice_plug::prelude::*;
use nice_plug_egui::create_egui_editor;
use egui::{
    self, pos2, vec2, Align2, Color32, ColorImage, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextureHandle,
    TextureOptions,
};
use std::f32::consts::{PI, TAU};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub const WIDTH: u32 = 1000;
pub const HEIGHT: u32 = 660;
const HEAT: Color32 = rgb(0xff, 0xb5, 0x47);
const COOL: Color32 = rgb(0x7f, 0xe0, 0xd0);
const CREAM: Color32 = rgb(0xea, 0xdf, 0xc6);
const PAINT: Color32 = rgb(0x21, 0x1d, 0x19);

struct St {
    presets: PresetBar,
    skin: Skin,
    an_in: Analyzer,
    an_out: Analyzer,
    worn: Option<TextureHandle>,
    /// when the ANALOG face was switched on (valves warm up)
    warm_from: f64,
    was_analog: bool,
    vu: f32,
    curve_key: (i32, i32, i32, i32),
    curve: Vec<(f32, f32)>,
    harm: [f32; 8],
    /// 0.2: the band the DIGITAL controls edit
    band: usize,
    /// 0.2: the right-hand box shows the CONSOLE EQ instead of the curve
    eq_tab: bool,
    /// 0.2: ANALOG's DRIVE moves every band together
    last_drive: Option<f32>,
    /// ANALOG pitch wheel: drawn roll angle + drag accumulator
    wheel_spin: f32,
    wheel_acc: f32,
    /// a ConsoleEq used only to draw the EQ curve
    eqv: crate::sat::ConsoleEq,
}

fn new_st(preset_dir: &'static str) -> St {
    St {
        presets: PresetBar::new(preset_dir),
        skin: Skin::new(Look::Blackface, HEAT, "KIL", env!("CARGO_PKG_VERSION"), ""),
        an_in: Analyzer::default(),
        an_out: Analyzer::default(),
        worn: None,
        warm_from: -10.0,
        was_analog: false,
        vu: -20.0,
        curve_key: (-1, -1, -1, -1),
        curve: Vec::new(),
        harm: [0.0; 8],
        band: 0,
        eq_tab: false,
        last_drive: None,
        wheel_spin: 0.0,
        wheel_acc: 0.0,
        eqv: crate::sat::ConsoleEq::new(48000.0),
    }
}

pub(crate) fn create(params: Arc<KilnParams>, shared: Arc<Shared>) -> Option<crate::nice_app::AwkEditor> {
    let state = params.editor_state.clone();
    let st = new_st("Kiln");
    create_egui_editor(state, nice_plug_egui::RepaintNotifier::new(), nice_plug_egui::EguiNiceSettings::default(), crate::nice_app::AwkApp::new(fonts, {
        let mut __st = st;
        Box::new(move |ctx: &mut egui::Ui, setter: &ParamSetter| {
            let st = &mut __st;
            frame(ctx, setter, st, params.as_ref(), shared.as_ref())
        })
    }))
}

fn fonts(ctx: &egui::Context) {
    install_fonts(ctx);
    // Add the two ANALOG faces on top of the kit's.
    let mut f = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 6] = [
        ("awk-d", include_bytes!("../assets/Michroma-Regular.ttf")),
        ("awk-l", include_bytes!("../assets/BarlowCondensed-SemiBold.ttf")),
        ("awk-b", include_bytes!("../assets/BarlowCondensed-Bold.ttf")),
        ("awk-m", include_bytes!("../assets/ShareTechMono-Regular.ttf")),
        ("kiln-o", include_bytes!("../assets/Oswald.ttf")),
        ("kiln-s", include_bytes!("../assets/SpecialElite-Regular.ttf")),
    ];
    let fallbacks: Vec<String> = ["Ubuntu-Light", "NotoEmoji-Regular", "emoji-icon-font"].iter().filter(|n| f.font_data.contains_key(**n)).map(|s| s.to_string()).collect();
    for (name, bytes) in faces {
        f.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
        let mut fam = vec![name.to_string()];
        fam.extend(fallbacks.iter().cloned());
        f.families.insert(FontFamily::Name(name.into()), fam);
    }
    for old in ["block", "marker"] {
        let mut fam = vec!["awk-b".to_string()];
        fam.extend(fallbacks.iter().cloned());
        f.families.insert(FontFamily::Name(old.into()), fam);
    }
    if let Some(p) = f.families.get_mut(&FontFamily::Proportional) {
        p.insert(0, "awk-l".into());
    }
    if let Some(m) = f.families.get_mut(&FontFamily::Monospace) {
        m.insert(0, "awk-m".into());
    }
    ctx.set_fonts(f);
}
fn osw(s: f32) -> FontId {
    FontId::new(s, FontFamily::Name("kiln-o".into()))
}
fn typ(s: f32) -> FontId {
    FontId::new(s, FontFamily::Name("kiln-s".into()))
}

/// Draws one character mirrored left-right (the reversed N in KILN), using the
/// font's own glyph so it matches the letters around it. `pos` is its left-centre.
fn mirrored_char(pa: &Painter, ch: char, pos: Pos2, font: FontId, col: Color32) -> Rect {
    let g = pa.layout_no_wrap(ch.to_string(), font, col);
    let r = Align2::LEFT_CENTER.anchor_size(pos, g.size());
    if let Some(gl) = g.rows.first().and_then(|row| row.glyphs.first()) {
        let uv = gl.uv_rect;
        let tl = r.min + gl.pos.to_vec2() + uv.offset;
        let gr = Rect::from_min_size(tl, uv.size);
        let [w, h] = pa.ctx().fonts(|f| f.font_image_size());
        let (u0, v0, u1, v1) = (uv.min[0] as f32 / w as f32, uv.min[1] as f32 / h as f32, uv.max[0] as f32 / w as f32, uv.max[1] as f32 / h as f32);
        let mut m = egui::Mesh::with_texture(egui::TextureId::default());
        let mut vtx = |p: Pos2, u: f32, v: f32| m.vertices.push(egui::epaint::Vertex { pos: p, uv: pos2(u, v), color: col });
        vtx(gr.left_top(), u1, v0);
        vtx(gr.right_top(), u0, v0);
        vtx(gr.right_bottom(), u0, v1);
        vtx(gr.left_bottom(), u1, v1);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        pa.add(Shape::mesh(m));
    }
    r
}

/// The KILN wordmark with its reversed N: "KIL" + a mirrored "N".
fn wordmark(pa: &Painter, pos: Pos2, font: FontId, col: Color32, sp: f32, worn: bool) -> Rect {
    let r = if worn { worn_txt(pa, pos, Align2::LEFT_CENTER, "KIL", font.clone(), col, sp) } else { txt(pa, pos, Align2::LEFT_CENTER, "KIL", font.clone(), col, sp) };
    let n = mirrored_char(pa, 'N', pos2(r.right(), pos.y), font, col);
    if worn {
        let mut rnd = Rng(77);
        for _ in 0..4 {
            let c = pos2(n.left() + rnd.f() * n.width(), n.top() + n.height() * (0.25 + 0.5 * rnd.f()));
            pa.circle_filled(c, 0.4 + rnd.f() * 1.1, al(PAINT, 0.5 + rnd.f() * 0.5));
        }
    }
    r.union(n)
}

fn set_i(setter: &ParamSetter, p: &IntParam, v: i32) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}
fn set_b(setter: &ParamSetter, p: &BoolParam, v: bool) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}
fn set_band_type(setter: &ParamSetter, p: &KilnParams, b: usize, t: i32) {
    let (ty, var, ..) = p.band(b);
    if ty.value() != t {
        set_i(setter, ty, t);
        set_i(setter, var, 0);
    }
}
fn nbands(p: &KilnParams) -> usize {
    p.bands.value().clamp(1, 3) as usize
}
fn eq_set(p: &KilnParams) -> crate::sat::EqSet {
    crate::sat::EqSet { hpf: crate::sat::EQ_HPF_HZ(p.eq_hpf.value()), lf: EQ_LF[p.eq_lf.value().clamp(0, 3) as usize].1, lg: p.eq_lg.value(), mf: EQ_MF[p.eq_mf.value().clamp(0, 5) as usize].1, mg: p.eq_mg.value(), hg: p.eq_hg.value() }
}

#[allow(dead_code)]
fn set_type(setter: &ParamSetter, p: &KilnParams, t: i32) {
    if p.ty.value() != t {
        set_i(setter, &p.ty, t);
        set_i(setter, &p.var, 0);
    }
}

fn frame(root: &mut egui::Ui, setter: &ParamSetter, st: &mut St, p: &KilnParams, sh: &Shared) {
    let ctx_owned = root.ctx().clone();
    let ctx = &ctx_owned;
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
    st.skin.begin(ctx);
    st.an_in.update(&sh.spec_in);
    st.an_out.update(&sh.spec_out);
    let analog = p.analog.load(Ordering::Relaxed);
    if analog && !st.was_analog {
        st.warm_from = st.skin.time;
    }
    st.was_analog = analog;
    if st.band >= nbands(p) {
        st.band = 0;
    }
    let (bty, bvar, bdrive, bcolor, ..) = p.band(st.band);
    let key = (bty.value(), bvar.value(), (bcolor.value() * 100.0) as i32, (bdrive.value() * 4.0) as i32 + 1000 * st.band as i32);
    if key != st.curve_key {
        st.curve_key = key;
        st.curve = sat::curve_points(bty.value(), bvar.value(), bcolor.value(), bdrive.value(), 160);
        st.harm = sat::harmonics(bty.value(), bvar.value(), bcolor.value(), bdrive.value());
    }
    st.eqv.configure(eq_set(p));
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(rgb(0x0c, 0x0a, 0x08))).show(root, |ui| {
        let whole = ui.max_rect();
        st.skin.background(ui.painter(), whole);
        let o = whole.min.to_vec2();
        if analog {
            analog_face(ui, setter, st, p, sh, o);
        } else {
            digital_face(ui, setter, st, p, sh, o);
        }
        // Header last so its dropdown draws over the faces.
        let _ = st.presets.strip(ui, setter, p, whole, &st.skin, &p.mix, &p.bypass);
        // The kit draws "KIL" in the header; finish the wordmark with the reversed N
        // (same place and font the kit uses for the name).
        let pa = ui.painter();
        let cy = whole.top() + crate::presets::STRIP / 2.0;
        let bw = txt_w(pa, "AWK STUDIOS", disp(9.0), 2.0);
        let nx = whole.left() + 18.0 + bw + 14.0 + txt_w(pa, "KIL", labb(21.0), 2.5);
        mirrored_char(pa, 'N', pos2(nx, cy + 1.0), labb(21.0), rgb(0xf2, 0xf2, 0xf2));
    });
}

// =====================================================================================
// DIGITAL
// =====================================================================================

fn mode_pill(ui: &mut egui::Ui, p: &KilnParams, r: Rect, now: f64) {
    let pa = ui.painter().clone();
    let analog = p.analog.load(Ordering::Relaxed);
    pa.rect_filled(r, r.height() / 2.0, rgb(0x0a, 0x08, 0x07));
    pa.rect_stroke(r, r.height() / 2.0, Stroke::new(1.0_f32, rgb(0x3d, 0x34, 0x2a)), StrokeKind::Inside);
    let half = r.width() / 2.0;
    let hl = Rect::from_min_size(r.min + vec2(2.0 + if analog { half - 2.0 } else { 0.0 }, 2.0), vec2(half - 2.0, r.height() - 4.0));
    pa.rect_filled(hl, hl.height() / 2.0, if analog { rgb(0x5a, 0x3a, 0x1e) } else { rgb(0x2f, 0x2a, 0x24) });
    for (i, s) in ["DIGITAL", "ANALOG"].iter().enumerate() {
        let rr = Rect::from_min_size(r.min + vec2(i as f32 * half, 0.0), vec2(half, r.height()));
        let resp = ui.interact(rr, ui.scope_id().with(("mode", i)), Sense::click());
        let on = (i == 1) == analog;
        let col = if on { if i == 1 { HEAT } else { rgb(0xf6, 0xef, 0xe4) } } else if resp.hovered() { rgb(0xb8, 0xad, 0x9f) } else { rgb(0x7d, 0x72, 0x66) };
        txt(&pa, rr.center(), Align2::CENTER_CENTER, *s, labb(12.0), col, 2.0);
        if resp.clicked() {
            p.analog.store(i == 1, Ordering::Relaxed);
        }
    }
    let _ = now;
}

fn chain(pa: &Painter, at: Pos2, p: &KilnParams, t: f64) {
    let tame_on = p.tame.value();
    let names = ["IN".to_string(), "TAME".to_string(), TYPES[p.ty.value() as usize].0.to_string(), "OUT".to_string()];
    let mut x = at.x;
    let mut spans = Vec::new();
    for (i, s) in names.iter().enumerate() {
        let w = txt_w(pa, s, mono(10.0), 1.5) + 14.0;
        let r = Rect::from_min_size(pos2(x, at.y), vec2(w, 18.0));
        let on = i != 1 || tame_on;
        let col = match i {
            1 => COOL,
            2 => HEAT,
            _ => rgb(0xd8, 0xcf, 0xc2),
        };
        pa.rect_filled(r, 2.0, Color32::from_black_alpha(140));
        pa.rect_stroke(r, 2.0, Stroke::new(1.0_f32, if on { al(col, 0.8) } else { Color32::from_white_alpha(38) }), StrokeKind::Inside);
        txt(pa, r.center(), Align2::CENTER_CENTER, s, mono(10.0), if on { col } else { al(rgb(0xd8, 0xcf, 0xc2), 0.3) }, 1.5);
        spans.push(r);
        x += w + 24.0;
    }
    for i in 0..3 {
        let (a, b) = (spans[i].right() + 2.0, spans[i + 1].left() - 2.0);
        let y = at.y + 9.0;
        if i < 2 && !tame_on && i == 0 {
            // bypass hop drawn below
        }
        line(pa, pos2(a, y), pos2(b, y), al(rgb(0xd8, 0xcf, 0xc2), 0.3), 1.0);
        for d in 0..2 {
            let f = ((t * 0.9 + d as f64 * 0.5 + i as f64 * 0.2) % 1.0) as f32;
            let col = if i < 2 { if tame_on { COOL } else { rgb(0xd8, 0xcf, 0xc2) } } else { HEAT };
            pa.circle_filled(pos2(a + f * (b - a), y), 1.9, col);
        }
    }
    if !tame_on {
        let (a, b) = (spans[0].right(), spans[2].left());
        let y = at.y + 18.0;
        dashed(pa, pos2(a + 4.0, y + 6.0), pos2(b - 4.0, y + 6.0), al(rgb(0xd8, 0xcf, 0xc2), 0.35), 1.0, 3.0, 3.0);
        txt(pa, pos2((a + b) / 2.0, y + 14.0), Align2::CENTER_CENTER, "TAME OFF", mono(8.5), al(rgb(0xd8, 0xcf, 0xc2), 0.5), 1.0);
    }
}

fn digital_face(ui: &mut egui::Ui, setter: &ParamSetter, st: &mut St, p: &KilnParams, sh: &Shared, o: egui::Vec2) {
    let at = |x: f32, y: f32| pos2(x, y) + o;
    let rc = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), vec2(w, h));
    let pa = ui.painter().clone();
    let t = st.skin.time;
    chain(&pa, at(18.0, 54.0), p, t);
    mode_pill(ui, p, rc(790.0, 52.0, 192.0, 24.0), t);

    spectrum(ui, setter, st, p, sh, rc(18.0, 86.0, 730.0, 236.0));
    curve_box(ui, setter, st, p, &pa, rc(760.0, 86.0, 222.0, 236.0));

    // Type tiles (for the selected band)
    let tw = (964.0 - 5.0 * 8.0) / 6.0;
    let b = st.band;
    let (bty, bvar, bdrive, bcolor, bsolo, bbyp) = p.band(b);
    let cur = bty.value();
    for (i, ty) in TYPES.iter().enumerate() {
        let r = rc(18.0 + i as f32 * (tw + 8.0), 334.0, tw, 52.0);
        let resp = ui.interact(r, ui.scope_id().with(("tile", i)), Sense::click());
        let on = cur == i as i32;
        pa.rect_filled(r, 3.0, if on { rgb(0x2b, 0x24, 0x1b) } else if resp.hovered() { rgb(0x1c, 0x18, 0x13) } else { rgb(0x14, 0x11, 0x0e) });
        pa.rect_stroke(r, 3.0, Stroke::new(if on { 1.5_f32 } else { 1.0_f32 }, if on { HEAT } else { rgb(0x2e, 0x27, 0x20) }), StrokeKind::Inside);
        txt(&pa, r.left_top() + vec2(12.0, 8.0), Align2::LEFT_TOP, ty.0, labb(16.0), if on { rgb(0xff, 0xf6, 0xe8) } else { rgb(0xb8, 0xad, 0x9f) }, 1.5);
        txt(&pa, r.left_top() + vec2(12.0, 30.0), Align2::LEFT_TOP, ty.2, lab(12.0), if on { rgb(0xcd, 0xbf, 0xa9) } else { rgb(0x7d, 0x72, 0x66) }, 0.5);
        // tiny curve glyph
        let g = sat::curve_points(i as i32, 0, 0.4, 9.0, 24);
        let gr = Rect::from_min_size(r.right_top() + vec2(-30.0, 10.0), vec2(20.0, 24.0));
        let pts: Vec<Pos2> = g.iter().map(|(x, y)| pos2(gr.left() + (x + 1.0) * 0.5 * gr.width(), gr.center().y - y * gr.height() * 0.5)).collect();
        pa.add(Shape::line(pts, Stroke::new(1.4_f32, if on { HEAT } else { al(rgb(0xb8, 0xad, 0x9f), 0.4) })));
        if resp.clicked() {
            set_band_type(setter, p, b, i as i32);
        }
    }
    // Variants
    st.skin.small(&pa, at(18.0, 405.0), Align2::LEFT_CENTER, "VARIANT", st.skin.text_dim());
    let vars = TYPES[cur.clamp(0, 5) as usize].3;
    for (i, v) in vars.iter().enumerate() {
        if st.skin.chip(ui, rc(82.0 + i as f32 * 96.0, 396.0, 88.0, 18.0), v, bvar.value() == i as i32, ("var", i)).clicked() {
            set_i(setter, bvar, i as i32);
        }
    }
    // 0.2: how many bands
    st.skin.small(&pa, at(388.0, 405.0), Align2::LEFT_CENTER, "BANDS", st.skin.text_dim());
    let nb = nbands(p);
    for n in 1..=3usize {
        if st.skin.chip(ui, rc(432.0 + (n - 1) as f32 * 28.0, 396.0, 24.0, 18.0), &n.to_string(), nb == n, ("bands", n)).on_hover_text("Split the saturation into bands, each with its own type and drive").clicked() {
            set_i(setter, &p.bands, n as i32);
        }
    }
    for (k, (x, name, prm)) in [(560.0, "CLEAN LOWS", &p.sat_lo), (780.0, "CLEAN HIGHS", &p.sat_hi)].into_iter().enumerate() {
        let lr = st.skin.small(&pa, at(x, 405.0), Align2::LEFT_CENTER, name, st.skin.text_dim());
        let c = pos2(lr.right() + 16.0, at(0.0, 405.0).y);
        st.skin.mini_knob(ui, setter, c, 8.0, prm);
        let v = prm.normalized_value_to_string(prm.unmodulated_normalized_value(), true);
        txt(&pa, pos2(c.x + 16.0, c.y), Align2::LEFT_CENTER, v, mono(10.0), if k == 0 && crate::sat::Split::lo_on(prm.value()) || k == 1 && crate::sat::Split::hi_on(prm.value()) { HEAT } else { st.skin.text_faint() }, 0.0);
    }

    // Panels
    st.skin.panel(&pa, rc(18.0, 424.0, 612.0, 196.0), "SATURATION", "");
    st.skin.panel(&pa, rc(642.0, 424.0, 258.0, 196.0), "TAME", "");
    st.skin.panel(&pa, rc(912.0, 424.0, 70.0, 196.0), "", "");
    let skin = &st.skin;
    let ci = cur.clamp(0, 5) as usize;
    skin.knob(ui, setter, at(92.0, 516.0), 32.0, "DRIVE", bdrive, true, false, bbyp.value());
    skin.knob(ui, setter, at(205.0, 518.0), 22.0, "COLOR", bcolor, false, false, bbyp.value());
    // 0.2: band tabs + SOLO / BYPASS
    let mut new_band = None;
    if nb > 1 {
        for i in 0..nb {
            let lbl = BAND_NAMES[if nb == 2 && i == 1 { 2 } else { i }];
            if skin.chip(ui, rc(430.0 + i as f32 * 62.0, 431.0, 58.0, 18.0), lbl, b == i, ("btab", i)).on_hover_text("Edit this band").clicked() {
                new_band = Some(i);
            }
        }
        let s_on = bsolo.value();
        if skin.chip(ui, rc(160.0, 592.0, 58.0, 18.0), "SOLO", s_on, "bsolo").on_hover_text("Hear only this band").clicked() {
            set_b(setter, bsolo, !s_on);
        }
    }
    let by_on = bbyp.value();
    if skin.chip(ui, rc(if nb > 1 { 224.0 } else { 160.0 }, 592.0, 92.0, 18.0), if nb > 1 { "BAND BYPASS" } else { "DRIVE BYPASS" }, by_on, "bbyp").on_hover_text("Let this band through without saturation").clicked() {
        set_b(setter, bbyp, !by_on);
    }
    skin.knob(ui, setter, at(294.0, 518.0), 22.0, "DEPTH", &p.depth, false, false, false);
    skin.knob(ui, setter, at(383.0, 518.0), 22.0, "TONE", &p.tone, false, true, false);
    skin.knob(ui, setter, at(472.0, 518.0), 22.0, "MIX", &p.mix, false, false, false);
    skin.knob(ui, setter, at(561.0, 518.0), 22.0, "OUTPUT", &p.out, false, true, false);
    for (x, s) in [(205.0, COLOR_DOES[ci]), (294.0, DEPTH_DOES[ci])] {
        txt(&pa, at(x, 478.0), Align2::CENTER_CENTER, s, mono(8.5), skin.text_faint(), 1.0);
    }
    let dim = !p.tame.value();
    skin.knob(ui, setter, at(690.0, 506.0), 22.0, "AMOUNT", &p.tamt, false, false, dim);
    skin.knob(ui, setter, at(771.0, 506.0), 22.0, "SHARP", &p.tsharp, false, false, dim);
    skin.knob(ui, setter, at(852.0, 506.0), 22.0, "SPEED", &p.tspeed, false, false, dim);
    skin.chip_param(ui, setter, rc(656.0, 584.0, 74.0, 20.0), &p.tame, ("TAME ON", "TAME OFF"));
    if skin.chip(ui, rc(738.0, 584.0, 66.0, 20.0), "DELTA", p.delta.value() && !dim, "delta").on_hover_text("Hear only what TAME removes").clicked() && !dim {
        set_b(setter, &p.delta, !p.delta.value());
    }
    txt(&pa, at(852.0, 594.0), Align2::CENTER_CENTER, format!("{} – {}", fmt_hz(p.tlo.value()), fmt_hz(p.thi.value())), mono(9.5), skin.text_faint(), 0.0);
    let m = &sh.meters;
    st.skin.meter(&pa, 922.0 + o.x, 460.0 + o.y, 126.0, "IN", m.db(0), m.db(1), Side::None, "in");
    st.skin.meter(&pa, 952.0 + o.x, 460.0 + o.y, 126.0, "OUT", m.db(2), m.db(3), Side::None, "out");

    // Footer
    let foot = rc(0.0, 630.0, 1000.0, 30.0);
    st.skin.footer(&pa, foot, 0.0);
    st.skin.os_selector(ui, setter, pos2(foot.left() + 16.0, foot.center().y - 8.5), &p.os, 0.7);
    st.skin.safe_chip(ui, setter, Rect::from_center_size(pos2(foot.left() + 430.0, foot.center().y), vec2(104.0, 18.0)), &p.safe, sh.meters.db(4));
    let ag = get(&sh.ag_db);
    let agl = if p.auto.value() { format!("AUTO GAIN {:+.1}", ag) } else { "AUTO GAIN OFF".into() };
    st.skin.chip_param(ui, setter, Rect::from_center_size(pos2(foot.left() + 556.0, foot.center().y), vec2(132.0, 18.0)), &p.auto, (&agl, &agl));
    if let Some(b) = new_band {
        st.band = b;
    }
}

fn fmt_hz(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1} kHz", f / 1000.0)
    } else {
        format!("{:.0} Hz", f)
    }
}

fn spectrum(ui: &mut egui::Ui, setter: &ParamSetter, st: &mut St, p: &KilnParams, sh: &Shared, r: Rect) {
    let pa0 = ui.painter().clone();
    let (pa, pal) = st.skin.screen(&pa0, r);
    let top = r.top() + 44.0;
    let bot = r.bottom() - 22.0;
    let x0 = r.left() + 34.0;
    let x1 = r.right() - 10.0;
    let mapx = |u: f32| x0 + u * (x1 - x0);
    let mapy = |db: f32| bot - ((db + 84.0) / 84.0).clamp(0.0, 1.0) * (bot - top);
    st.skin.freq_axis(&pa, &pal, mapx, top, bot, r.bottom() - 9.0);
    st.skin.db_axis(&pa, &pal, x0, x1, mapy, &[-12.0, -24.0, -36.0, -48.0, -60.0, -72.0], x0 - 4.0);
    let tame_on = p.tame.value();
    let delta = tame_on && p.delta.value();
    let n = 160;
    let pts = |an: &Analyzer| -> Vec<Pos2> { (0..=n).map(|i| i as f32 / n as f32).map(|u| pos2(mapx(u), mapy(an.at(u)))).collect() };
    // TAME range (draggable edges)
    if tame_on {
        let (lo, hi) = (p.tlo.value(), p.thi.value());
        let (xl, xh) = (mapx(fu(lo)), mapx(fu(hi)));
        for off in [Rect::from_min_max(pos2(x0, top - 30.0), pos2(xl, bot)), Rect::from_min_max(pos2(xh, top - 30.0), pos2(x1, bot))] {
            if off.width() < 1.0 {
                continue;
            }
            pa.rect_filled(off, 0.0, Color32::from_black_alpha(150));
            let hp = pa.with_clip_rect(off);
            let mut x = off.left() - off.height();
            while x < off.right() {
                line(&hp, pos2(x, off.bottom()), pos2(x + off.height(), off.top()), al(COOL, 0.07), 1.0);
                x += 9.0;
            }
            if off.width() > 54.0 {
                txt(&pa, pos2(off.center().x, top + 22.0), Align2::CENTER_CENTER, "TAME OFF", mono(9.0), al(COOL, 0.6), 1.5);
            }
        }
        for (k, (x, prm)) in [(xl, &p.tlo), (xh, &p.thi)].into_iter().enumerate() {
            let hr = Rect::from_min_max(pos2(x - 6.0, top), pos2(x + 6.0, (top + bot) / 2.0));
            let id = ui.scope_id().with(("trange", k));
            let resp = ui.interact(hr, id, Sense::click_and_drag());
            knob_like_freq(ui, setter, &resp, prm, mapx, x0, x1);
            let col = al(COOL, if resp.hovered() || resp.dragged() { 0.9 } else { 0.45 });
            dashed(&pa, pos2(x, top), pos2(x, bot), col, 1.0, 2.0, 3.0);
            txt(&pa, pos2(x, top + 8.0), Align2::CENTER_CENTER, fmt_hz(prm.value()), mono(8.5), col, 0.0);
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
    }
    // CLEAN LOWS / HIGHS: the parts that go around the drive
    for (k, prm) in [&p.sat_lo, &p.sat_hi].into_iter().enumerate() {
        let f = prm.value();
        let on = if k == 0 { crate::sat::Split::lo_on(f) } else { crate::sat::Split::hi_on(f) };
        let x = mapx(fu(f.clamp(20.0, 20000.0)));
        if on {
            let band = if k == 0 { Rect::from_min_max(pos2(x0, top), pos2(x, bot)) } else { Rect::from_min_max(pos2(x, top), pos2(x1, bot)) };
            pa.rect_filled(band, 0.0, al(HEAT, 0.06));
            txt(&pa, pos2(band.center().x, bot - 10.0), Align2::CENTER_CENTER, "CLEAN", mono(9.0), al(HEAT, 0.7), 2.0);
        }
        let hx = if on { x } else if k == 0 { x0 + 6.0 } else { x1 - 6.0 };
        let hr = Rect::from_min_max(pos2(hx - 7.0, (top + bot) / 2.0), pos2(hx + 7.0, bot));
        let resp = ui.interact(hr, ui.scope_id().with(("srange", k)), Sense::click_and_drag());
        knob_like_freq(ui, setter, &resp, prm, mapx, x0, x1);
        let hot = resp.hovered() || resp.dragged();
        let col = al(HEAT, if hot { 0.95 } else if on { 0.6 } else { 0.3 });
        if on {
            line(&pa, pos2(x, (top + bot) / 2.0), pos2(x, bot), col, 1.2);
        }
        // grab tab
        let tab = Rect::from_center_size(pos2(hx, bot - 26.0), vec2(10.0, 22.0));
        pa.rect_filled(tab, 2.0, al(HEAT, if hot { 0.9 } else { 0.45 }));
        if hot || on {
            let s = format!("{} {}", if k == 0 { "CLEAN BELOW" } else { "CLEAN ABOVE" }, prm.normalized_value_to_string(prm.unmodulated_normalized_value(), true));
            txt(&pa, pos2(hx + if k == 0 { 9.0 } else { -9.0 }, bot - 40.0), if k == 0 { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER }, s, mono(9.0), col, 0.5);
        }
        if hot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }
    let pin = pts(&st.an_in);
    polyline(&pa, pin.clone(), al(CREAM, if delta { 0.15 } else { 0.42 }), 1.2, false);
    let pout = pts(&st.an_out);
    if delta {
        polyline(&pa, pout, COOL, 1.6, pal.glow);
    } else {
        fill_under(&pa, &pout, bot, al(HEAT, 0.18));
        polyline(&pa, pout, HEAT, 1.6, pal.glow);
    }
    // TAME cut hanging from the top
    if tame_on {
        let cut: Vec<f32> = (0..BANDS).map(|k| get(&sh.cut[k])).collect();
        let at_f = |f: f32| -> f32 {
            // interpolate across bands in log frequency
            let pos = (f / band_freq(0)).ln() / (band_freq(BANDS - 1) / band_freq(0)).ln() * (BANDS - 1) as f32;
            if pos <= 0.0 {
                return cut[0];
            }
            if pos >= (BANDS - 1) as f32 {
                return cut[BANDS - 1];
            }
            let i = pos as usize;
            cut[i] + (cut[i + 1] - cut[i]) * (pos - i as f32)
        };
        let ry = top - 2.0;
        let k = 70.0 / 18.0;
        let pts: Vec<Pos2> = (0..=n).map(|i| i as f32 / n as f32).map(|u| pos2(mapx(u), ry - at_f(20.0 * 1000f32.powf(u)) * k)).collect();
        let mut poly = pts.clone();
        poly.push(pos2(x1, ry));
        poly.push(pos2(x0, ry));
        let mut m = egui::Mesh::default();
        for (i, q) in pts.iter().enumerate() {
            m.colored_vertex(*q, al(COOL, 0.28));
            m.colored_vertex(pos2(q.x, ry), al(COOL, 0.05));
            if i > 0 {
                let b = (i * 2) as u32;
                m.add_triangle(b - 2, b - 1, b);
                m.add_triangle(b - 1, b, b + 1);
            }
        }
        let inside = pa.with_clip_rect(Rect::from_min_max(pos2(mapx(fu(p.tlo.value())), r.top()), pos2(mapx(fu(p.thi.value())), r.bottom())).intersect(pa.clip_rect()));
        inside.add(Shape::mesh(m));
        polyline(&inside, pts, COOL, 1.3, pal.glow);
        line(&pa, pos2(x0, ry), pos2(x1, ry), al(COOL, 0.25), 1.0);
        let (mut mk, mut mv) = (0usize, 0.0f32);
        for (i, c) in cut.iter().enumerate() {
            if *c < mv {
                mv = *c;
                mk = i;
            }
        }
        if mv < -0.3 {
            let x = mapx(fu(band_freq(mk)));
            txt(&pa, pos2(x, ry - mv * k + 10.0), Align2::CENTER_CENTER, format!("{:.1} dB", mv), mono(10.0), COOL, 0.0);
        }
        // What it caught: a marker on the input at every resonance being cut,
        // with the frequency on the biggest ones.
        let mut caught: Vec<(usize, f32)> = (0..BANDS).filter(|&i| cut[i] < -1.0 && (i == 0 || cut[i] <= cut[i - 1]) && (i + 1 == BANDS || cut[i] <= cut[i + 1])).map(|i| (i, cut[i])).collect();
        caught.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (j, (i, c)) in caught.iter().enumerate() {
            let f = band_freq(*i);
            let q = pos2(mapx(fu(f)), mapy(st.an_in.at(fu(f))));
            pa.circle_stroke(q, 5.0, Stroke::new(1.5_f32, COOL));
            pa.circle_filled(q, 2.0, COOL);
            if j < 4 {
                txt(&pa, q + vec2(0.0, -12.0), Align2::CENTER_CENTER, format!("{} {:.0}", fmt_hz(f), c), mono(9.0), COOL, 0.0);
            }
        }
    }
    // 0.2: band splits (drag the lines); click a band's name to edit it
    let nb = nbands(p);
    if nb > 1 {
        let mut edges = vec![x0];
        for prm in [&p.x1, &p.x2].into_iter().take(nb - 1) {
            edges.push(mapx(fu(prm.value())));
        }
        edges.push(x1);
        for bi in 0..nb {
            let (el, er) = (edges[bi], edges[bi + 1].max(edges[bi] + 1.0));
            let br = Rect::from_min_max(pos2(el, bot - 22.0), pos2(er, bot));
            let resp = ui.interact(br, ui.scope_id().with(("bandsel", bi)), Sense::click());
            if resp.clicked() {
                st.band = bi;
            }
            let sel = st.band == bi;
            if sel {
                pa.rect_filled(Rect::from_min_max(pos2(el, top), pos2(er, bot)), 0.0, al(HEAT, 0.025));
            }
            let name = BAND_NAMES[if nb == 2 && bi == 1 { 2 } else { bi }];
            let (bt, _, _, _, _, bb) = p.band(bi);
            let what = if bb.value() { "CLEAN" } else { TYPES[bt.value().clamp(0, 5) as usize].1 };
            txt(&pa, pos2(br.center().x, bot - 11.0), Align2::CENTER_CENTER, format!("{name} · {what}"), mono(9.0), al(if sel { HEAT } else { CREAM }, if sel { 0.95 } else { 0.5 }), 1.5);
        }
        for (k, prm) in [&p.x1, &p.x2].into_iter().take(nb - 1).enumerate() {
            let x = mapx(fu(prm.value()));
            let my = (top + bot) * 0.5;
            let hr = Rect::from_min_max(pos2(x - 7.0, my), pos2(x + 7.0, bot - 24.0));
            let resp = ui.interact(hr, ui.scope_id().with(("xover", k)), Sense::click_and_drag());
            knob_like_freq(ui, setter, &resp, prm, mapx, x0, x1);
            let hot = resp.hovered() || resp.dragged();
            line(&pa, pos2(x, top), pos2(x, bot), al(HEAT, if hot { 0.9 } else { 0.5 }), 1.5);
            pa.rect_filled(Rect::from_center_size(pos2(x, my + 30.0), vec2(8.0, 24.0)), 2.0, al(HEAT, if hot { 1.0 } else { 0.7 }));
            txt(&pa, pos2(x, my + 52.0), Align2::CENTER_CENTER, fmt_hz(prm.value()), mono(8.5), HEAT, 0.0);
            if hot {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
    }
    // 0.2: CONSOLE EQ curve (gold) around the middle line
    if p.eq.value() {
        let mid = (top + bot) * 0.5;
        let kk = (bot - top) * 0.5 / 24.0;
        line(&pa, pos2(x0, mid), pos2(x1, mid), al(rgb(0xff, 0xe0, 0x8a), 0.15), 1.0);
        let pts: Vec<Pos2> = (0..=n).map(|i| i as f32 / n as f32).map(|u| pos2(mapx(u), mid - st.eqv.db_at(20.0 * 1000f32.powf(u)).clamp(-24.0, 24.0) * kk)).collect();
        polyline(&pa, pts, rgb(0xff, 0xe0, 0x8a), 1.6, pal.glow);
        txt(&pa, pos2(x0 + 6.0, mid - 8.0), Align2::LEFT_CENTER, if p.eq_pos.value() == 0 { "CONSOLE EQ · PRE" } else { "CONSOLE EQ · POST" }, mono(8.5), al(rgb(0xff, 0xe0, 0x8a), 0.8), 1.5);
    }
    let label = if delta { "DELTA · HEARING ONLY WHAT TAME REMOVES" } else { "IN ── OUT" };
    txt(&pa, pos2(r.right() - 12.0, r.top() + 14.0), Align2::RIGHT_CENTER, label, mono(9.0), if delta { COOL } else { al(CREAM, 0.55) }, 1.5);
    st.skin.screen_end(&pa0, r);
}

/// Horizontal drag on a frequency handle.
fn knob_like_freq(ui: &mut egui::Ui, setter: &ParamSetter, resp: &Response, prm: &FloatParam, mapx: impl Fn(f32) -> f32, x0: f32, x1: f32) {
    let _ = mapx;
    if resp.double_clicked() {
        setter.begin_set_parameter(prm);
        setter.set_parameter_normalized(prm, prm.default_normalized_value());
        setter.end_set_parameter(prm);
        return;
    }
    if resp.drag_started() {
        setter.begin_set_parameter(prm);
    }
    if resp.dragged() {
        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            let u = ((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            let f = 20.0 * 1000f32.powf(u);
            setter.set_parameter(prm, f);
        }
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(prm);
    }
}

fn curve_box(ui: &mut egui::Ui, setter: &ParamSetter, st: &mut St, p: &KilnParams, pa0: &Painter, r: Rect) {
    let (pa, pal) = st.skin.screen(pa0, r);
    // 0.2: CURVE / CONSOLE EQ tabs
    let c1 = st.skin.chip(ui, Rect::from_min_size(r.left_top() + vec2(8.0, 6.0), vec2(62.0, 18.0)), "CURVE", !st.eq_tab, "tab-curve").clicked();
    let c2 = st.skin.chip(ui, Rect::from_min_size(r.left_top() + vec2(76.0, 6.0), vec2(104.0, 18.0)), "CONSOLE EQ", st.eq_tab, "tab-eq").on_hover_text("The classic British preamp EQ").clicked();
    if c1 {
        st.eq_tab = false;
    }
    if c2 {
        st.eq_tab = true;
    }
    if st.eq_tab {
        let skin = &st.skin;
        let dim = !p.eq.value();
        skin.chip_param(ui, setter, Rect::from_min_size(r.left_top() + vec2(12.0, 34.0), vec2(66.0, 18.0)), &p.eq, ("EQ IN", "EQ OUT"));
        for (i, s) in ["PRE", "POST"].iter().enumerate() {
            let cr = Rect::from_min_size(r.left_top() + vec2(88.0 + i as f32 * 56.0, 34.0), vec2(52.0, 18.0));
            if skin.chip(ui, cr, s, p.eq_pos.value() == i as i32, ("eqpos", i)).on_hover_text(if i == 0 { "EQ before the drive: shapes what gets driven" } else { "EQ after the drive: shapes the result" }).clicked() {
                set_i(setter, &p.eq_pos, i as i32);
            }
        }
        let cx = |i: usize| r.left() + 40.0 + i as f32 * 71.0;
        skin.knob(ui, setter, pos2(cx(0), r.top() + 92.0), 15.0, "HIGH 12k", &p.eq_hg, false, true, dim);
        skin.knob(ui, setter, pos2(cx(1), r.top() + 92.0), 15.0, "MID", &p.eq_mg, false, true, dim);
        skin.knob(ui, setter, pos2(cx(2), r.top() + 92.0), 15.0, "LOW", &p.eq_lg, false, true, dim);
        skin.knob(ui, setter, pos2(cx(0), r.top() + 172.0), 15.0, "HIGH-PASS", &p.eq_hpf, false, false, dim);
        skin.knob(ui, setter, pos2(cx(1), r.top() + 172.0), 15.0, "MID FREQ", &p.eq_mf, false, false, dim);
        skin.knob(ui, setter, pos2(cx(2), r.top() + 172.0), 15.0, "LOW FREQ", &p.eq_lf, false, false, dim);
        st.skin.screen_end(pa0, r);
        return;
    }
    let s = (r.width() - 40.0).min(132.0);
    let c = pos2(r.center().x, r.top() + 32.0 + s / 2.0);
    let mx = |x: f32| c.x + x * s / 2.0;
    let my = |y: f32| c.y - y * s / 2.0;
    line(&pa, pos2(mx(-1.0), c.y), pos2(mx(1.0), c.y), pal.grid, 1.0);
    line(&pa, pos2(c.x, my(-1.0)), pos2(c.x, my(1.0)), pal.grid, 1.0);
    dashed(&pa, pos2(mx(-1.0), my(-1.0)), pos2(mx(1.0), my(1.0)), Color32::from_white_alpha(30), 1.0, 2.0, 3.0);
    let pts: Vec<Pos2> = st.curve.iter().map(|(x, y)| pos2(mx(*x), my(*y))).collect();
    polyline(&pa, pts, HEAT, 2.0, pal.glow);
    let by = r.bottom() - 20.0;
    let bh = 50.0;
    let bw = (r.width() - 40.0) / 7.0;
    txt(&pa, pos2(r.left() + 12.0, by - bh - 8.0), Align2::LEFT_CENTER, "HARMONICS", mono(9.0), al(HEAT, 0.75), 1.5);
    for k in 2..=8usize {
        let rel = st.harm[k - 1];
        let db = if rel > 1e-5 { 20.0 * rel.log10() } else { -80.0 };
        let hh = ((db + 60.0) / 60.0).clamp(0.0, 1.0) * bh;
        let bx = r.left() + 20.0 + (k - 2) as f32 * bw;
        pa.rect_filled(Rect::from_min_max(pos2(bx + 3.0, by - bh), pos2(bx + bw - 3.0, by)), 0.0, Color32::from_white_alpha(10));
        pa.rect_filled(Rect::from_min_max(pos2(bx + 3.0, by - hh), pos2(bx + bw - 3.0, by)), 0.0, if k % 2 == 0 { HEAT } else { al(CREAM, 0.65) });
        txt(&pa, pos2(bx + bw / 2.0, by + 9.0), Align2::CENTER_CENTER, format!("H{k}"), mono(8.5), al(CREAM, 0.55), 0.0);
    }
    st.skin.screen_end(pa0, r);
}

// =====================================================================================
// ANALOG
// =====================================================================================

struct Rng(u32);
impl Rng {
    fn f(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / 16777216.0
    }
}

/// The worn paint, made once: hammered grain, scratches, chipped edges, rust by the
/// screws, a darker vignette and some finger-polished spots. 2x resolution.
fn worn_image() -> ColorImage {
    let (w, h) = (2000usize, 1232usize);
    let mut px = vec![[0x21 as f32, 0x1d as f32, 0x19 as f32]; w * h];
    let mut rnd = Rng(4242);
    let blend = |px: &mut Vec<[f32; 3]>, x: i32, y: i32, c: [f32; 3], a: f32| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let q = &mut px[y as usize * w + x as usize];
            for k in 0..3 {
                q[k] += (c[k] - q[k]) * a;
            }
        }
    };
    // grain
    for _ in 0..240_000 {
        let (x, y) = ((rnd.f() * w as f32) as i32, (rnd.f() * h as f32) as i32);
        let a = rnd.f() * 0.10;
        if rnd.f() < 0.5 {
            blend(&mut px, x, y, [255.0, 240.0, 220.0], a);
        } else {
            blend(&mut px, x, y, [0.0, 0.0, 0.0], a * 2.0);
        }
    }
    // scratches
    for _ in 0..700 {
        let (mut x, mut y) = (rnd.f() * w as f32, rnd.f() * h as f32);
        let len = 8.0 + rnd.f() * if rnd.f() < 0.1 { 180.0 } else { 50.0 };
        let mut a = (rnd.f() - 0.5) * 1.2 + if rnd.f() < 0.3 { 1.57 } else { 0.0 };
        let bend = (rnd.f() - 0.5) * 0.02;
        let alpha = 0.06 + rnd.f() * 0.18;
        let mut d = 0.0;
        while d < len {
            blend(&mut px, x as i32, y as i32, [200.0, 192.0, 180.0], alpha);
            x += a.cos();
            y += a.sin();
            a += bend;
            d += 1.0;
        }
    }
    // chips (bare aluminium) near the edges and a few on the face
    let mut chip = |px: &mut Vec<[f32; 3]>, rnd: &mut Rng, cx: f32, cy: f32, r: f32| {
        let n = (r * r * 4.0) as i32 + 4;
        for _ in 0..n {
            let a = rnd.f() * TAU;
            let d = rnd.f().sqrt() * r * (0.6 + 0.6 * (a * 3.0).sin().abs());
            blend(px, (cx + a.cos() * d) as i32, (cy + a.sin() * d) as i32, [0x8f as f32, 0x8a as f32, 0x80 as f32], 0.9);
        }
    };
    for _ in 0..120 {
        let e = rnd.f();
        let (x, y) = if e < 0.25 {
            (20.0 + rnd.f() * 60.0, 20.0 + rnd.f() * (h as f32 - 40.0))
        } else if e < 0.5 {
            (w as f32 - 80.0 + rnd.f() * 60.0, 20.0 + rnd.f() * (h as f32 - 40.0))
        } else if e < 0.75 {
            (rnd.f() * w as f32, 4.0 + rnd.f() * 20.0)
        } else {
            (rnd.f() * w as f32, h as f32 - 24.0 + rnd.f() * 20.0)
        };
        let rr_ = 2.0 + rnd.f() * 7.0;
        chip(&mut px, &mut rnd, x, y, rr_);
    }
    for _ in 0..28 {
        let (x, y) = (160.0 + rnd.f() * 1680.0, 60.0 + rnd.f() * 1100.0);
        let rr_ = 1.5 + rnd.f() * 3.0;
        chip(&mut px, &mut rnd, x, y, rr_);
    }
    // rust around the four screws
    for (sx, sy) in [(52.0, 60.0), (52.0, 1160.0), (1948.0, 60.0), (1948.0, 1160.0)] {
        for _ in 0..500 {
            let a = rnd.f() * TAU;
            let d = 16.0 + rnd.f() * 34.0;
            let c = [120.0 + rnd.f() * 50.0, 60.0 + rnd.f() * 20.0, 20.0];
            let (x, y) = (sx + a.cos() * d, sy + a.sin() * d);
            let s = 1 + (rnd.f() * 3.0) as i32;
            let al_ = 0.1 + rnd.f() * 0.35;
            for dx in 0..s {
                for dy in 0..s {
                    blend(&mut px, x as i32 + dx, y as i32 + dy, c, al_);
                }
            }
        }
    }
    // vignette + polished spots
    let spots: Vec<(f32, f32, f32)> = (0..5).map(|_| (300.0 + rnd.f() * 1400.0, 820.0 + rnd.f() * 240.0, 80.0 + rnd.f() * 80.0)).collect();
    let mut img = ColorImage::filled([w, h], Color32::BLACK);
    for y in 0..h {
        for x in 0..w {
            let q = px[y * w + x];
            let (dx, dy) = ((x as f32 - w as f32 / 2.0) / (w as f32 / 2.0), (y as f32 - h as f32 / 2.0) / (h as f32 / 2.0));
            let v = 1.0 - 0.38 * ((dx * dx + dy * dy) / 2.0).sqrt().powf(1.6);
            let mut lift = 0.0;
            for (sx, sy, sr) in spots.iter() {
                let d = ((x as f32 - sx).powi(2) + (y as f32 - sy).powi(2)).sqrt() / sr;
                if d < 1.0 {
                    lift += 9.0 * (1.0 - d);
                }
            }
            let c = |k: usize| ((q[k] + lift) * v).clamp(0.0, 255.0) as u8;
            img.pixels[y * w + x] = Color32::from_rgb(c(0), c(1), c(2));
        }
    }
    img
}

/// Text with paint wear: a few specks of panel paint over the letters.
fn worn_txt(pa: &Painter, pos: Pos2, a: Align2, s: &str, font: FontId, col: Color32, sp: f32) -> Rect {
    let r = txt(pa, pos, a, s, font, col, sp);
    let mut rnd = Rng(s.len() as u32 * 977 + (pos.x * 13.0 + pos.y) as u32);
    let n = (s.len() as f32 * 1.6).max(3.0) as usize;
    for _ in 0..n {
        let c = pos2(r.left() + rnd.f() * r.width(), r.top() + r.height() * (0.2 + 0.6 * rnd.f()));
        pa.circle_filled(c, 0.4 + rnd.f() * 1.1, al(PAINT, 0.5 + rnd.f() * 0.5));
    }
    r
}

fn screw(pa: &Painter, c: Pos2, rot: f32) {
    disc_radial(pa, c, 7.0, rgb(0xd8, 0xd3, 0xc8), rgb(0x4b, 0x47, 0x40), vec2(-2.0, -2.0));
    pa.circle_stroke(c, 7.0, Stroke::new(1.0_f32, Color32::from_black_alpha(150)));
    for k in 0..2 {
        let a = rot + k as f32 * PI / 2.0;
        line(pa, polar(c, a, 5.5), polar(c, a + PI, 5.5), rgb(0x2b, 0x28, 0x23), 2.2);
    }
}

fn lamp(pa: &Painter, c: Pos2, col: Color32, on: bool) {
    pa.circle_filled(c, 8.0, rgb(0x8a, 0x85, 0x7c));
    if on {
        for i in 0..6 {
            pa.circle_filled(c, 7.0 + i as f32 * 2.2, al(col, 0.07 * (1.0 - i as f32 / 6.0)));
        }
        disc_radial(pa, c, 6.0, Color32::WHITE, col, vec2(-1.5, -1.5));
    } else {
        disc_radial(pa, c, 6.0, rgb(0x77, 0x77, 0x77), mix_col(col, rgb(0x28, 0x1e, 0x19), 0.8), vec2(-1.5, -1.5));
    }
}

#[allow(dead_code)]
/// Bat-handle toggle. `ang` is where the lever points (0 = up).
fn bat(pa: &Painter, c: Pos2, ang: f32) {
    disc_radial(pa, c, 12.0, rgb(0xe4, 0xdf, 0xd4), rgb(0x5b, 0x57, 0x4f), vec2(-3.0, -3.0));
    pa.circle_stroke(c, 12.0, Stroke::new(1.0_f32, Color32::from_black_alpha(150)));
    let dir = vec2(ang.sin(), -ang.cos());
    let side = vec2(dir.y, -dir.x);
    let tip = c + dir * 26.0;
    // shadow
    let sh = vec2(3.0, 3.0);
    pa.add(Shape::convex_polygon(vec![c + side * 3.0 + sh, tip + side * 4.5 + sh, tip + dir * 4.5 + sh, tip - side * 4.5 + sh, c - side * 3.0 + sh], Color32::from_black_alpha(110), Stroke::NONE));
    pa.add(Shape::convex_polygon(vec![c + side * 3.0, tip + side * 4.5, tip + dir * 4.5, tip - side * 4.5, c - side * 3.0], rgb(0xc9, 0xc4, 0xba), Stroke::new(1.0_f32, rgb(0x6a, 0x65, 0x5c))));
    line(pa, c + side * 1.0, tip + side * 1.5, Color32::from_white_alpha(140), 1.2);
}

/// Chicken-head pointer knob with a printed 0-10 scale. Returns the response.
#[allow(clippy::too_many_arguments)]
fn chicken<P: Param>(ui: &mut egui::Ui, setter: &ParamSetter, c: Pos2, r: f32, param: &P, label: &str, col: Color32, dim: bool, scale: bool) -> Response {
    let id = ui.scope_id().with(("chick", label, c.x as i32, c.y as i32));
    let resp = ui.interact(Rect::from_center_size(c, egui::Vec2::splat(r * 2.0 + 16.0)), id, Sense::click_and_drag());
    knob_input(ui, setter, &resp, id, param);
    let pa = ui.painter().clone();
    let v = param.unmodulated_normalized_value();
    let a = ang(v);
    let f = |k: Color32| if dim { k.gamma_multiply(0.55) } else { k };
    if scale {
        for i in 0..=10 {
            let aa = ang(i as f32 / 10.0);
            line(&pa, polar(c, aa, r + 6.0), polar(c, aa, r + if i % 5 == 0 { 13.0 } else { 10.0 }), f(al(col, 0.75)), 1.3);
            if i % 2 == 0 {
                txt(&pa, polar(c, aa, r + 22.0), Align2::CENTER_CENTER, format!("{i}"), osw(10.0), f(al(col, 0.8)), 0.0);
            }
        }
    }
    disc_shadow(&pa, c, r * 0.9, vec2(2.0, 5.0), 0.9);
    disc_radial(&pa, c, r * 0.82, f(rgb(0x3a, 0x33, 0x2c)), f(rgb(0x0d, 0x0b, 0x09)), vec2(-r * 0.3, -r * 0.3));
    // pointer: long tapered nose toward the value, short round tail
    let dir = vec2(a.cos(), a.sin());
    let side = vec2(-dir.y, dir.x);
    let (ln, tl, wd) = (r * 1.18, r * 0.78, r * 0.42);
    let mut pts = Vec::new();
    for i in 0..=16 {
        let t = i as f32 / 16.0;
        // nose half: from tip (t=0) back to the widest point
        let along = ln * (1.0 - t);
        let half = wd * (t.powf(0.7));
        pts.push((along, half));
    }
    for i in 1..=12 {
        let t = i as f32 / 12.0 * PI / 2.0;
        pts.push((-tl * t.sin(), wd * t.cos()));
    }
    let mut poly: Vec<Pos2> = pts.iter().map(|(al_, h)| c + dir * *al_ + side * *h).collect();
    let back: Vec<Pos2> = pts.iter().rev().skip(1).map(|(al_, h)| c + dir * *al_ - side * *h).collect();
    poly.extend(back);
    let shadow: Vec<Pos2> = poly.iter().map(|q| *q + vec2(0.0, 3.0)).collect();
    pa.add(Shape::convex_polygon(shadow, Color32::from_black_alpha(150), Stroke::NONE));
    pa.add(Shape::convex_polygon(poly, f(rgb(0x1a, 0x17, 0x14)), Stroke::new(1.0_f32, f(Color32::from_rgba_unmultiplied(255, 240, 220, 30)))));
    disc_radial(&pa, c - dir * r * 0.1, r * 0.36, f(rgb(0x4a, 0x42, 0x3a)), Color32::TRANSPARENT, vec2(-r * 0.15, -r * 0.2));
    line(&pa, c + dir * r * 0.15, c + dir * (ln - 4.0), f(rgb(0xef, 0xe6, 0xd4)), 2.2);
    pa.circle_filled(c, r * 0.16, rgb(0x24, 0x20, 0x1b));
    if !label.is_empty() {
        worn_txt(&pa, pos2(c.x, c.y + r + 34.0), Align2::CENTER_CENTER, label, osw(14.0), f(col), 3.0);
    }
    if resp.hovered() || resp.dragged() {
        let s = param.normalized_value_to_string(v, true);
        let br = Rect::from_center_size(pos2(c.x, c.y - r - 22.0), vec2(txt_w(&pa, &s, mono(10.5), 0.0) + 14.0, 18.0));
        pa.rect_filled(br, 2.0, Color32::from_rgba_unmultiplied(10, 8, 6, 225));
        txt(&pa, br.center(), Align2::CENTER_CENTER, &s, mono(10.5), HEAT, 0.0);
    }
    resp
}

fn vu(pa: &Painter, r: Rect, val: f32, warm: f32) {
    pa.rect_filled(r.expand(8.0), 6.0, rgb(0x14, 0x11, 0x0e));
    pa.rect_stroke(r.expand(8.0), 6.0, Stroke::new(2.0_f32, rgb(0x6d, 0x68, 0x5f)), StrokeKind::Inside);
    vgrad(pa, r, rgb(0xf3, 0xe6, 0xc4), rgb(0xd9, 0xc7, 0x9c));
    // warm backlight
    let pc = pos2(r.center().x, r.bottom());
    for i in 0..8 {
        let t = i as f32 / 8.0;
        let rr = Rect::from_center_size(pc - vec2(0.0, r.height() * 0.25), vec2(r.width() * (1.0 - t * 0.8), r.height() * (1.0 - t * 0.7)));
        pa.rect_filled(rr.intersect(r), 0.0, Color32::from_rgba_unmultiplied(255, 170, 60, (warm * 10.0) as u8));
    }
    let cp = pa.with_clip_rect(r);
    // everything on the scale comes from ONE pivot and ONE radius
    let piv = pos2(r.center().x, r.bottom() + 30.0);
    let rad = r.height() - 20.0;
    let amp = |d: f32| 10f32.powf(d / 20.0);
    let (a0, a1) = (amp(-20.0), amp(3.0));
    // real VU spacing: position follows voltage, so 0 VU sits ~70% across
    let vmap = move |d: f32| -PI / 2.0 + ((amp(d.clamp(-20.0, 3.0)) - a0) / (a1 - a0) - 0.5) * 1.48;
    let ink = rgb(0x2a, 0x24, 0x1c);
    let red = rgb(0xb3, 0x26, 0x1e);
    cp.add(Shape::line(arc_pts(piv, rad, vmap(-20.0), vmap(0.0), 48), Stroke::new(1.3_f32, ink)));
    cp.add(Shape::line(arc_pts(piv, rad + 2.0, vmap(0.0), vmap(3.0), 20), Stroke::new(4.0_f32, red)));
    for d in [-20.0f32, -10.0, -7.0, -5.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0] {
        let a = vmap(d);
        let long = d as i32 % 5 == 0 || d > -4.0;
        let col = if d > 0.0 { red } else { ink };
        line(&cp, polar(piv, a, rad), polar(piv, a, rad + if long { 11.0 } else { 7.0 }), col, 1.4);
        let s = if d > 0.0 { format!("+{}", d as i32) } else { format!("{}", (d as i32).abs()) };
        txt(&cp, polar(piv, a, rad + 21.0), Align2::CENTER_CENTER, s, osw(10.0), col, 0.0);
    }
    txt(&cp, pos2(piv.x, r.bottom() - 34.0), Align2::CENTER_CENTER, "VU", osw(22.0), ink, 2.0);
    txt(&cp, pos2(piv.x, r.bottom() - 14.0), Align2::CENTER_CENTER, "KILN · OUTPUT", typ(9.0), rgb(0x5a, 0x4d, 0x3a), 1.0);
    let a = vmap(val);
    line(&cp, piv + vec2(2.0, 1.0), polar(piv, a, rad + 5.0) + vec2(2.0, 1.0), Color32::from_black_alpha(70), 2.0);
    line(&cp, piv, polar(piv, a, rad + 5.0), rgb(0x14, 0x12, 0x10), 1.6);
    // glass glare
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.left_top(), Color32::from_white_alpha(60));
    m.colored_vertex(r.left_top() + vec2(r.width() * 0.55, 0.0), Color32::TRANSPARENT);
    m.colored_vertex(r.left_bottom(), Color32::TRANSPARENT);
    m.add_triangle(0, 1, 2);
    cp.add(Shape::mesh(m));
}

fn tubes(pa: &Painter, r: Rect, warm: f32, drive01: f32, label: &str, t: f64) {
    pa.rect_filled(r.expand(8.0), 6.0, rgb(0x0b, 0x09, 0x07));
    pa.rect_stroke(r.expand(8.0), 6.0, Stroke::new(2.0_f32, rgb(0x6d, 0x68, 0x5f)), StrokeKind::Inside);
    pa.rect_filled(r, 3.0, rgb(0x12, 0x0d, 0x09));
    let fl = 0.94 + 0.06 * ((t * 37.0).sin() * (t * 11.3).sin()) as f32;
    let gw = warm * (0.55 + 0.45 * drive01) * fl;
    let cp = pa.with_clip_rect(r);
    for i in 0..10 {
        let k = i as f32 / 10.0;
        cp.circle_filled(pos2(r.center().x, r.top() + r.height() * 0.62), r.width() * 0.8 * (1.0 - k), Color32::from_rgba_unmultiplied(255, 120, 30, (gw * 9.0) as u8));
    }
    for tx in [r.left() + r.width() * 0.3, r.left() + r.width() * 0.7] {
        let tw = r.width() * 0.26;
        let ty = r.top() + 18.0;
        let th = r.height() - 30.0;
        let glass = Color32::from_rgba_unmultiplied(230, 220, 200, 90);
        let mut outline = vec![pos2(tx - tw / 2.0, ty + th)];
        outline.extend(arc_pts(pos2(tx, ty + tw / 2.0), tw / 2.0, PI, TAU, 16));
        outline.push(pos2(tx + tw / 2.0, ty + th));
        cp.add(Shape::line(outline, Stroke::new(1.2_f32, glass)));
        cp.rect_filled(Rect::from_center_size(pos2(tx, ty + th * 0.53), vec2(tw * 0.64, th * 0.42)), 0.0, Color32::from_rgba_unmultiplied(120, 115, 105, 140));
        cp.rect_filled(Rect::from_center_size(pos2(tx, ty + th * 0.53), vec2(tw * 0.52, th * 0.34)), 0.0, Color32::from_rgba_unmultiplied(30, 26, 22, 230));
        let gc = pos2(tx, ty + th * 0.53);
        for i in 0..8 {
            let k = i as f32 / 8.0;
            let col = mix_col(rgb(255, 200, 110), rgb(255, 90, 20), k);
            cp.circle_filled(gc, tw * 0.7 * (0.15 + k), Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (gw * 60.0 * (1.0 - k)) as u8));
        }
        line(&cp, pos2(tx, ty + th * 0.38), pos2(tx, ty + th * 0.68), Color32::from_rgba_unmultiplied(255, 220, 150, (gw * 255.0) as u8), 1.4);
        cp.rect_filled(Rect::from_center_size(pos2(tx, ty + 6.0), vec2(tw * 0.8, 3.0)), 0.0, Color32::from_rgba_unmultiplied(200, 195, 185, 128));
        txt(&cp, pos2(tx, ty + th * 0.25), Align2::CENTER_CENTER, label, mono(8.5), Color32::from_rgba_unmultiplied(230, 220, 200, 115), 1.0);
        cp.rect_filled(Rect::from_min_size(pos2(tx - tw / 2.0 - 3.0, ty + th), vec2(tw + 6.0, 8.0)), 0.0, rgb(0x2a, 0x25, 0x20));
    }
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.left_top(), Color32::from_white_alpha(36));
    m.colored_vertex(r.left_top() + vec2(r.width() * 0.5, 0.0), Color32::TRANSPARENT);
    m.colored_vertex(r.left_bottom(), Color32::TRANSPARENT);
    m.add_triangle(0, 1, 2);
    cp.add(Shape::mesh(m));
    worn_txt(pa, pos2(r.center().x, r.bottom() + 28.0), Align2::CENTER_CENTER, "VALVES", osw(12.0), CREAM, 3.0);
}

fn lit_button(ui: &mut egui::Ui, r: Rect, s: &str, on: bool, id: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(("litbtn", id)), Sense::click());
    let pa = ui.painter();
    let rr = r.translate(vec2(0.0, if on { 2.0 } else { 0.0 }));
    pa.rect_filled(r.translate(vec2(2.0, 3.0)), 3.0, Color32::from_black_alpha(150));
    if on {
        for i in 0..5 {
            pa.rect_filled(rr.expand(i as f32 * 2.0), 3.0 + i as f32 * 2.0, al(HEAT, 0.06));
        }
        vgrad(pa, rr, rgb(0xff, 0xcf, 0x7a), rgb(0xd9, 0x8a, 0x2b));
    } else {
        vgrad(pa, rr, if resp.hovered() { rgb(0x58, 0x51, 0x47) } else { rgb(0x4a, 0x44, 0x3c) }, rgb(0x2a, 0x26, 0x20));
    }
    pa.rect_stroke(rr, 3.0, Stroke::new(1.0_f32, Color32::from_black_alpha(180)), StrokeKind::Inside);
    txt(pa, rr.center(), Align2::CENTER_CENTER, s, osw(12.5), if on { rgb(0x2b, 0x1a, 0x08) } else { rgb(0xcf, 0xc6, 0xb6) }, 1.5);
    resp.clicked()
}

/// Recessed well that lit keys sit in.
fn bezel(pa: &Painter, r: Rect) {
    pa.rect_filled(r, 7.0, rgb(0x09, 0x08, 0x07));
    pa.rect_stroke(r, 7.0, Stroke::new(1.0_f32, Color32::from_black_alpha(220)), StrokeKind::Outside);
    line(pa, pos2(r.left() + 6.0, r.bottom() + 1.5), pos2(r.right() - 6.0, r.bottom() + 1.5), Color32::from_white_alpha(16), 1.0);
    vgrad(pa, Rect::from_min_size(r.left_top() + vec2(3.0, 2.0), vec2(r.width() - 6.0, 6.0)), Color32::from_black_alpha(140), Color32::TRANSPARENT);
}

/// Backlit glass key: smoked when off, amber and glowing when lit.
fn lit_key(ui: &mut egui::Ui, r: Rect, s: &str, on: bool, id: impl std::hash::Hash + std::fmt::Debug) -> Response {
    let resp = ui.interact(r, ui.scope_id().with(("litkey", id)), Sense::click());
    let pa = ui.painter();
    let down = if on { 1.5 } else if resp.is_pointer_button_down_on() { 1.0 } else { -0.5 };
    let k = r.translate(vec2(0.0, down));
    pa.rect_filled(r.translate(vec2(0.0, 2.5)), 4.0, rgb(0x04, 0x03, 0x03));
    if on {
        for i in 0..6 {
            pa.rect_filled(k.expand(2.0 + i as f32 * 2.5), 5.0 + i as f32 * 2.5, Color32::from_rgba_unmultiplied(255, 140, 40, (14 - i * 2) as u8));
        }
        pa.rect_filled(k, 4.0, rgb(0xb4, 0x58, 0x12));
        for i in 0..10 {
            let t = i as f32 / 10.0;
            let rr = k.shrink2(vec2(k.width() * 0.46 * t, k.height() * 0.40 * t));
            pa.rect_filled(rr, 4.0 + 8.0 * t, al(mix_col(rgb(0xe8, 0x86, 0x28), rgb(0xff, 0xe6, 0xb0), t), 0.45));
        }
        vgrad(pa, Rect::from_min_size(k.left_top() + vec2(4.0, 3.0), vec2(k.width() - 8.0, k.height() * 0.4)), Color32::from_white_alpha(70), Color32::TRANSPARENT);
    } else {
        vgrad(pa, k, rgb(0x2c, 0x28, 0x23), rgb(0x11, 0x0f, 0x0e));
        vgrad(pa, Rect::from_min_size(k.left_top() + vec2(4.0, 3.0), vec2(k.width() - 8.0, k.height() * 0.4)), Color32::from_white_alpha(if resp.hovered() { 26 } else { 14 }), Color32::TRANSPARENT);
        line(pa, k.left_top() + vec2(4.0, 0.5), k.right_top() + vec2(-4.0, 0.5), Color32::from_white_alpha(30), 1.0);
    }
    pa.rect_stroke(k, 4.0, Stroke::new(1.0_f32, rgb(0x05, 0x04, 0x04)), StrokeKind::Outside);
    let tc = if on { rgb(0x3a, 0x1c, 0x06) } else if resp.hovered() { al(CREAM, 0.6) } else { al(CREAM, 0.36) };
    if on {
        txt(pa, k.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, s, osw(12.0), Color32::from_rgba_unmultiplied(255, 236, 190, 120), 2.4);
    }
    txt(pa, k.center(), Align2::CENTER_CENTER, s, osw(12.0), tc, 2.4);
    resp
}

fn small_key(ui: &mut egui::Ui, r: Rect, s: &str, on: bool, id: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(("key", id)), Sense::click());
    let pa = ui.painter();
    pa.rect_filled(r, 2.0, if on { rgb(0xe9, 0xd9, 0xb0) } else if resp.hovered() { rgb(0x4e, 0x48, 0x40) } else { rgb(0x3e, 0x39, 0x33) });
    pa.rect_stroke(r, 2.0, Stroke::new(1.0_f32, Color32::from_black_alpha(160)), StrokeKind::Inside);
    txt(pa, r.center(), Align2::CENTER_CENTER, s, osw(11.0), if on { rgb(0x2b, 0x28, 0x23) } else { rgb(0xbd, 0xb6, 0xa8) }, 0.5);
    resp.clicked()
}

fn analog_face(ui: &mut egui::Ui, setter: &ParamSetter, st: &mut St, p: &KilnParams, sh: &Shared, o: egui::Vec2) {
    let at = |x: f32, y: f32| pos2(x, y) + o;
    let rc = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), vec2(w, h));
    let t = st.skin.time;
    let warm = (((t - st.warm_from) as f32 - 0.25) / 1.6).clamp(0.0, 1.0);
    if st.worn.is_none() {
        st.worn = Some(ui.ctx().load_texture("kiln-worn", worn_image(), TextureOptions::LINEAR));
    }
    let pa = ui.painter().clone();
    // rack behind, the unit, the unit below
    pa.rect_filled(rc(0.0, 44.0, 1000.0, 616.0), 0.0, rgb(0x0a, 0x09, 0x08));
    for x in [0.0, 960.0] {
        hgrad(&pa, rc(x, 44.0, 40.0, 616.0), rgb(0x3a, 0x38, 0x33), rgb(0x2a, 0x29, 0x25));
        let mut y = 50.0;
        while y < 660.0 {
            pa.rect_filled(rc(x + 15.0, y, 10.0, 8.0), 1.5, rgb(0x0b, 0x0b, 0x0a));
            y += 22.0;
        }
    }
    let face = rc(8.0, 50.0, 984.0, 572.0);
    for i in 0..8 {
        pa.rect_filled(face.expand(i as f32 * 2.0).translate(vec2(0.0, 4.0)), 2.0 + i as f32, Color32::from_black_alpha((60 - i * 7) as u8));
    }
    if let Some(tex) = &st.worn {
        pa.image(tex.id(), face, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 572.0 / 616.0)), Color32::WHITE);
    }
    pa.rect_filled(Rect::from_min_size(face.left_top(), vec2(face.width(), 1.0)), 0.0, Color32::from_white_alpha(20));
    pa.rect_filled(Rect::from_min_size(face.left_bottom(), vec2(face.width(), 1.0)), 0.0, Color32::from_black_alpha(160));
    let below = rc(8.0, 630.0, 984.0, 30.0);
    pa.rect_filled(below, 0.0, rgb(0x2e, 0x3a, 0x34));
    pa.rect_filled(Rect::from_min_size(below.left_top(), vec2(below.width(), 1.0)), 0.0, Color32::from_white_alpha(20));
    worn_txt(&pa, below.center() + vec2(0.0, 2.0), Align2::CENTER_CENTER, "PARAMETRIC  ·  EQ-4", osw(11.0), rgb(0xc9, 0xc3, 0xb6), 3.0);
    for (x, y, rot) in [(26.0, 80.0, 0.3), (26.0, 590.0, 1.1), (974.0, 80.0, 0.8), (974.0, 590.0, 2.0)] {
        screw(&pa, at(x, y), rot);
    }
    for hx in [56.0, 944.0] {
        pa.rect_filled(rc(hx - 5.0, 136.0, 16.0, 384.0), 7.0, Color32::from_black_alpha(120));
        hgrad(&pa, rc(hx - 7.0, 132.0, 14.0, 384.0), rgb(0x6b, 0x67, 0x5f), rgb(0xef, 0xeb, 0xe2));
        hgrad(&pa, rc(hx, 132.0, 7.0, 384.0), rgb(0xef, 0xeb, 0xe2), rgb(0x5a, 0x56, 0x4f));
        for yy in [132.0, 504.0] {
            pa.rect_filled(rc(hx - 9.0, yy, 18.0, 12.0), 0.0, rgb(0x2b, 0x28, 0x23));
        }
    }

    // --- top band: name, model, mode switch, power
    wordmark(&pa, at(92.0, 92.0), osw(46.0), CREAM, 10.0, true);
    worn_txt(&pa, at(96.0, 128.0), Align2::LEFT_CENTER, "VALVE · TAPE · IRON SATURATOR", osw(11.5), al(CREAM, 0.8), 3.0);
    // preset name on label tape (read-only; the header strip browses)
    let tape = rc(380.0, 66.0, 240.0, 36.0);
    bezel(&pa, tape.expand(5.0));
    vgrad(&pa, tape, rgb(0x30, 0x1b, 0x07), rgb(0x12, 0x0a, 0x03));
    for i in 0..5 {
        let t = i as f32 / 5.0;
        pa.rect_filled(tape.shrink2(vec2(tape.width() * 0.4 * t, tape.height() * 0.3 * t)), 3.0, Color32::from_rgba_unmultiplied(255, 140, 40, 7));
    }
    let pname = st.presets.current.to_uppercase();
    let pname: String = pname.chars().take(22).collect();
    txt(&pa, tape.center(), Align2::CENTER_CENTER, &pname, mono(16.0), Color32::from_rgba_unmultiplied(255, 160, 60, 70), 3.6);
    txt(&pa, tape.center(), Align2::CENTER_CENTER, &pname, mono(15.0), rgb(0xff, 0xb3, 0x47), 3.0);
    worn_txt(&pa, at(500.0, 118.0), Align2::CENTER_CENTER, "PROGRAM", osw(9.5), al(CREAM, 0.6), 3.0);

    // FACE: interlocked lit keys (ANALOG is lit on this face)
    bezel(&pa, rc(700.0, 66.0, 184.0, 54.0));
    lit_key(ui, rc(707.0, 73.0, 82.0, 40.0), "ANALOG", true, "aface_a");
    if lit_key(ui, rc(795.0, 73.0, 82.0, 40.0), "DIGITAL", false, "aface_d").clicked() {
        p.analog.store(false, Ordering::Relaxed);
    }
    worn_txt(&pa, at(792.0, 134.0), Align2::CENTER_CENTER, "FACE", osw(9.5), al(CREAM, 0.6), 3.0);
    let on = !p.bypass.value();
    lamp(&pa, at(920.0, 90.0), rgb(0xff, 0x4a, 0x2a), on && warm > 0.05);
    worn_txt(&pa, at(920.0, 118.0), Align2::CENTER_CENTER, "POWER", osw(10.0), CREAM, 2.0);
    if ui.interact(rc(898.0, 74.0, 44.0, 54.0), ui.scope_id().with("apower"), Sense::click()).clicked() {
        set_b(setter, &p.bypass, on);
    }

    // --- meter, valves
    let m = &sh.meters;
    let lo = m.db(2).max(m.db(3));
    let tgt = (lo + 14.0).clamp(-20.0, 3.5);
    st.vu += (tgt - st.vu) * 0.18;
    vu(&pa, rc(104.0, 170.0, 250.0, 170.0), -20.0 + (st.vu + 20.0) * warm, warm);
    let ty = p.ty.value().clamp(0, 5);
    let tl = if ty <= T_POWER && !(ty == T_PRE && p.var.value() == 2) { TYPES[ty as usize].3[p.var.value().clamp(0, 2) as usize] } else { "12AX7" };
    tubes(&pa, rc(388.0, 170.0, 170.0, 170.0), warm, p.drive.value() / 24.0, tl, t);

    // --- TYPE: pitch wheel + lit window (roll, scroll, click a half or a name)
    let n = TYPES.len() as i32;
    let step_px = 22.0;
    let step_ang = 0.36;
    worn_txt(&pa, at(703.0, 174.0), Align2::CENTER_CENTER, "TYPE", osw(14.0), CREAM, 3.0);
    let slot = rc(606.0, 196.0, 46.0, 150.0);
    let win = rc(664.0, 196.0, 140.0, 150.0);
    bezel(&pa, slot.expand(3.0));
    bezel(&pa, win.expand(3.0));
    let wresp = ui.interact(slot, ui.scope_id().with("awheel"), Sense::click_and_drag());
    let mut go = 0i32;
    if wresp.dragged() {
        st.wheel_acc += wresp.drag_delta().y;
        while st.wheel_acc > step_px { go += 1; st.wheel_acc -= step_px; }
        while st.wheel_acc < -step_px { go -= 1; st.wheel_acc += step_px; }
    } else {
        st.wheel_acc = 0.0;
    }
    if wresp.clicked() {
        if let Some(pp) = wresp.interact_pointer_pos() {
            go = if pp.y < slot.center().y { -1 } else { 1 };
        }
    }
    if wresp.hovered() || ui.rect_contains_pointer(win) {
        let sc = ui.input(|i| i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum::<f32>());
        if sc != 0.0 {
            go = if sc > 0.0 { -1 } else { 1 };
        }
    }
    let at_end = (ty + go < 0 && go < 0) || (ty + go >= n && go > 0);
    if go != 0 && !at_end {
        set_type(setter, p, (ty + go).clamp(0, n - 1));
    }
    if at_end { st.wheel_acc = st.wheel_acc.clamp(-4.0, 4.0); }
    let tgt = p.ty.value() as f32 * step_ang + st.wheel_acc / step_px * step_ang;
    st.wheel_spin += (tgt - st.wheel_spin) * 0.3;
    // the wheel: a rubber cylinder seen face-on through its slot
    let cyl = slot.shrink2(vec2(7.0, 6.0));
    let cp = pa.with_clip_rect(cyl);
    let (cy, rr) = (cyl.center().y, cyl.height() / 2.0);
    let half = Rect::from_min_size(cyl.left_top(), vec2(cyl.width(), rr));
    vgrad(&cp, half, rgb(0x04, 0x04, 0x04), rgb(0x5c, 0x56, 0x4e));
    vgrad(&cp, half.translate(vec2(0.0, rr)), rgb(0x5c, 0x56, 0x4e), rgb(0x04, 0x04, 0x04));
    hgrad(&cp, Rect::from_min_size(cyl.left_top(), vec2(8.0, cyl.height())), Color32::from_black_alpha(140), Color32::TRANSPARENT);
    hgrad(&cp, Rect::from_min_size(cyl.right_top() - vec2(8.0, 0.0), vec2(8.0, cyl.height())), Color32::TRANSPARENT, Color32::from_black_alpha(140));
    let rib = TAU / 48.0;
    let mut k = (-PI / 2.0 - st.wheel_spin) / rib;
    k = k.ceil();
    loop {
        let phi = k * rib + st.wheel_spin;
        if phi > PI / 2.0 { break; }
        let y = cy + rr * phi.sin();
        let lum = phi.cos();
        line(&cp, pos2(cyl.left(), y), pos2(cyl.right(), y), Color32::from_black_alpha((90.0 + 120.0 * lum) as u8), 1.0 + 1.6 * lum);
        line(&cp, pos2(cyl.left(), y + 1.0 + lum), pos2(cyl.right(), y + 1.0 + lum), Color32::from_white_alpha((22.0 * lum) as u8), 1.0);
        k += 1.0;
    }
    // index stripe printed on the rubber (centred when settled)
    let off = st.wheel_spin - p.ty.value() as f32 * step_ang;
    if off.abs() < PI / 2.0 {
        let y = cy + rr * off.sin();
        let w = 3.0 + 2.0 * off.cos();
        pa.with_clip_rect(cyl).rect_filled(Rect::from_center_size(pos2(cyl.center().x, y), vec2(cyl.width(), w)), 0.0, rgb(0xff, 0xb3, 0x47));
    }
    for (x, d) in [(slot.left() - 9.0, 1.0f32), (slot.right() + 9.0, -1.0)] {
        pa.add(Shape::convex_polygon(vec![pos2(x, cy - 4.0), pos2(x + 5.0 * d, cy), pos2(x, cy + 4.0)], al(CREAM, 0.6), Stroke::NONE));
    }
    // lit window
    pa.rect_filled(win, 5.0, rgb(0x0a, 0x08, 0x06));
    for i in 0..6 {
        let t = i as f32 / 6.0;
        pa.rect_filled(win.shrink2(vec2(win.width() * 0.4 * t, win.height() * 0.38 * t)), 5.0, Color32::from_rgba_unmultiplied(255, 140, 40, 6));
    }
    let cur = p.ty.value().clamp(0, n - 1);
    let rail = Rect::from_center_size(win.center() - vec2(0.0, 6.0), vec2(win.width() - 20.0, 34.0));
    for y in [rail.top(), rail.bottom()] {
        line(&pa, pos2(rail.left(), y), pos2(rail.right(), y), Color32::from_rgba_unmultiplied(255, 179, 71, 80), 1.0);
    }
    for (dy, j) in [(-44.0, cur - 1), (0.0, cur), (44.0, cur + 1)] {
        if j < 0 || j >= n { continue; }
        let c = win.center() + vec2(0.0, dy - 6.0);
        let name = TYPES[j as usize].1;
        let r = if dy == 0.0 {
            txt(&pa, c, Align2::CENTER_CENTER, name, osw(20.0), Color32::from_rgba_unmultiplied(255, 160, 60, 60), 3.2);
            txt(&pa, c, Align2::CENTER_CENTER, name, osw(19.0), rgb(0xff, 0xb3, 0x47), 3.0)
        } else {
            txt(&pa, c, Align2::CENTER_CENTER, name, osw(12.5), al(CREAM, 0.3), 2.6)
        };
        if dy != 0.0 && ui.interact(r.expand2(vec2(14.0, 8.0)), ui.scope_id().with(("tname", j)), Sense::click()).clicked() {
            set_type(setter, p, j);
        }
    }
    for i in 0..n {
        let c = pos2(win.center().x + (i as f32 - (n - 1) as f32 / 2.0) * 11.0, win.bottom() - 12.0);
        pa.circle_filled(c, 2.6, if i == cur { rgb(0xff, 0xb3, 0x47) } else { rgb(0x3a, 0x35, 0x2e) });
    }
    worn_txt(&pa, at(705.0, 364.0), Align2::CENTER_CENTER, "ROLL · SCROLL · CLICK", osw(8.5), al(CREAM, 0.5), 2.5);

    // --- VARIANT push-buttons
    worn_txt(&pa, at(882.0, 174.0), Align2::CENTER_CENTER, "VARIANT", osw(14.0), CREAM, 3.0);
    for (i, v) in TYPES[ty as usize].3.iter().enumerate() {
        if lit_button(ui, rc(838.0, 206.0 + i as f32 * 36.0, 88.0, 28.0), v, p.var.value() == i as i32, ("avar", i)) {
            set_i(setter, &p.var, i as i32);
        }
    }

    line(&pa, at(96.0, 384.0), at(904.0, 384.0), Color32::from_black_alpha(150), 1.0);
    line(&pa, at(96.0, 385.0), at(904.0, 385.0), Color32::from_white_alpha(18), 1.0);

    // --- knob row
    let before = p.drive.value();
    let dresp = chicken(ui, setter, at(160.0, 464.0), 40.0, &p.drive, "DRIVE", CREAM, false, true);
    // 0.2: with bands, the big DRIVE moves every band together
    if nbands(p) > 1 && dresp.dragged() {
        let prev = st.last_drive.unwrap_or(before);
        let d = p.drive.value() - prev;
        if d.abs() > 1e-4 {
            for prm in [&p.drive2, &p.drive3] {
                setter.begin_set_parameter(prm);
                setter.set_parameter(prm, (prm.value() + d).clamp(0.0, 24.0));
                setter.end_set_parameter(prm);
            }
        }
        st.last_drive = Some(p.drive.value());
    } else {
        st.last_drive = None;
    }
    // 0.2: lamps for what's on in DIGITAL
    let nb = nbands(p);
    lamp(&pa, at(846.0, 334.0), HEAT, nb > 1);
    worn_txt(&pa, at(858.0, 334.0), Align2::LEFT_CENTER, &if nb > 1 { format!("{nb} BANDS") } else { "1 BAND".to_string() }, osw(10.0), CREAM, 1.5);
    lamp(&pa, at(846.0, 356.0), HEAT, p.eq.value());
    worn_txt(&pa, at(858.0, 356.0), Align2::LEFT_CENTER, "CONSOLE EQ", osw(10.0), CREAM, 1.5);
    chicken(ui, setter, at(300.0, 468.0), 28.0, &p.color, "COLOR", CREAM, false, true);
    chicken(ui, setter, at(420.0, 468.0), 28.0, &p.depth, "DEPTH", CREAM, false, true);
    let tame_on = p.tame.value();
    chicken(ui, setter, at(560.0, 468.0), 28.0, &p.tamt, "TAME", COOL, !tame_on, true);
    bezel(&pa, rc(617.0, 444.0, 60.0, 48.0));
    if lit_key(ui, rc(623.0, 450.0, 48.0, 36.0), "ON", tame_on, "atame").on_hover_text("TAME on / off").clicked() {
        set_b(setter, &p.tame, !tame_on);
    }
    chicken(ui, setter, at(740.0, 468.0), 28.0, &p.mix, "MIX", CREAM, false, true);
    chicken(ui, setter, at(866.0, 468.0), 28.0, &p.out, "OUTPUT", CREAM, false, true);

    // --- bottom row
    let safe = p.safe.value();
    lamp(&pa, at(110.0, 580.0), rgb(0xff, 0x9a, 0x2a), safe);
    worn_txt(&pa, at(128.0, 580.0), Align2::LEFT_CENTER, "SAFE", osw(11.0), CREAM, 2.0);
    if ui.interact(rc(100.0, 568.0, 70.0, 24.0), ui.scope_id().with("asafe"), Sense::click()).clicked() {
        set_b(setter, &p.safe, !safe);
    }
    worn_txt(&pa, at(200.0, 580.0), Align2::LEFT_CENTER, "OVERSAMPLE", osw(11.0), CREAM, 2.0);
    for (i, s) in ["1×", "2×", "4×", "8×"].iter().enumerate() {
        if small_key(ui, rc(294.0 + i as f32 * 34.0, 571.0, 28.0, 18.0), s, p.os.value() == i as i32, ("aos", i)) {
            set_i(setter, &p.os, i as i32);
        }
    }
    let auto = p.auto.value();
    lamp(&pa, at(450.0, 580.0), rgb(0xff, 0x9a, 0x2a), auto);
    worn_txt(&pa, at(468.0, 580.0), Align2::LEFT_CENTER, "AUTO GAIN", osw(11.0), CREAM, 2.0);
    if ui.interact(rc(440.0, 568.0, 110.0, 24.0), ui.scope_id().with("aauto"), Sense::click()).clicked() {
        set_b(setter, &p.auto, !auto);
    }
    for (x, name, prm) in [(600.0, "LO", &p.sat_lo), (668.0, "HI", &p.sat_hi)] {
        worn_txt(&pa, at(x - 16.0, 580.0), Align2::RIGHT_CENTER, name, osw(10.0), CREAM, 1.5);
        chicken(ui, setter, at(x, 580.0), 12.0, prm, "", CREAM, false, false);
    }
    worn_txt(&pa, at(634.0, 552.0), Align2::CENTER_CENTER, "CLEAN", osw(9.5), al(CREAM, 0.7), 2.5);
    let plate = rc(720.0, 564.0, 180.0, 34.0);
    vgrad(&pa, plate, rgb(0xc9, 0xc4, 0xb8), rgb(0x8c, 0x87, 0x7d));
    for x in [plate.left() + 7.0, plate.right() - 7.0] {
        pa.circle_filled(pos2(x, plate.center().y), 2.5, rgb(0x5b, 0x57, 0x4f));
    }
    txt(&pa, plate.center() + vec2(0.0, -7.0), Align2::CENTER_CENTER, "AWK STUDIOS", typ(10.5), rgb(0x2b, 0x28, 0x23), 2.0);
    txt(&pa, plate.center() + vec2(0.0, 7.0), Align2::CENTER_CENTER, format!("MODEL K-6  ·  v{}", env!("CARGO_PKG_VERSION")), typ(9.5), rgb(0x3d, 0x38, 0x30), 1.0);

    // cold start: the whole unit comes up as the valves warm
    if warm < 1.0 {
        pa.rect_filled(rc(0.0, 44.0, 1000.0, 616.0), 0.0, Color32::from_black_alpha((60.0 * (1.0 - warm)) as u8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::context::gui::GuiContextInner;
    use nice_plug::plugin::PluginState;
    use egui::{epaint, Event, ImageData, Modifiers, PointerButton, RawInput, TextureId, TextureWrapMode, Vec2};
    use std::collections::HashMap;

    struct Mock;
    impl GuiContextInner for Mock {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Vst3
        }
        fn request_restart(&self) {}
        unsafe fn raw_begin_set_parameter(&self, _p: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _p: ParamPtr, _n: f32) {}
        unsafe fn raw_end_set_parameter(&self, _p: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState { version: String::new(), params: Default::default(), fields: Default::default() }
        }
        fn set_state(&self, _s: PluginState) {}
    }

    /// Both faces draw, and clicking all over them never panics.
    #[test]
    fn both_faces_draw_and_click() {
        let ctx = egui::Context::default();
        fonts(&ctx);
        let p = KilnParams::default();
        let sh = Shared::default();
        let mock = Mock;
        let setter = ParamSetter::new(&mock);
        let mut st = new_st("KilnTest");
        let mut i = 0;
        let mut run = |st: &mut St, ev: Vec<Event>| {
            let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH as f32, HEIGHT as f32))), time: Some(i as f64 * 0.04), events: ev, ..Default::default() };
            i += 1;
            let _ = ctx.run_ui(raw, |ctx| frame(ctx, &setter, st, &p, &sh));
        };
        for analog in [false, true] {
            p.analog.store(analog, Ordering::Relaxed);
            run(&mut st, vec![]);
            for y in (60..650).step_by(41) {
                for x in (20..990).step_by(57) {
                    let q = Pos2::new(x as f32, y as f32);
                    p.analog.store(analog, Ordering::Relaxed);
                    run(&mut st, vec![Event::PointerMoved(q), Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
                    run(&mut st, vec![Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
                }
            }
        }
    }
    /// Tiny software rasteriser for egui meshes, so the real window can be checked as a PNG.
    struct Tex {
        w: usize,
        h: usize,
        px: Vec<[f32; 4]>,
        repeat: bool,
    }
    fn apply_delta(texs: &mut HashMap<TextureId, Tex>, out: &egui::FullOutput) {
        for (id, d) in out.textures_delta.set.iter().flat_map(|(id, ds)| ds.iter().map(move |d| (id, d))) {
            let (w, h, px): (usize, usize, Vec<[f32; 4]>) = match &d.image {
                ImageData::Color(c) => (c.size[0], c.size[1], c.pixels.iter().map(|p| [p.r() as f32 / 255.0, p.g() as f32 / 255.0, p.b() as f32 / 255.0, p.a() as f32 / 255.0]).collect()),
            };
            let repeat = d.options.wrap_mode == TextureWrapMode::Repeat;
            if let Some(pos) = d.pos {
                let t = texs.get_mut(id).unwrap();
                for y in 0..h {
                    for x in 0..w {
                        t.px[(pos[1] + y) * t.w + pos[0] + x] = px[y * w + x];
                    }
                }
            } else {
                texs.insert(*id, Tex { w, h, px, repeat });
            }
        }
    }
    fn render(ctx: &egui::Context, out: egui::FullOutput, texs: &mut HashMap<TextureId, Tex>, path: &str) {
        apply_delta(texs, &out);
        let (w, h) = (WIDTH as usize, HEIGHT as usize);
        let mut img = vec![[0.0f32; 3]; w * h];
        let prims = ctx.tessellate(out.shapes, 1.0);
        for cp in prims {
            let epaint::Primitive::Mesh(mesh) = cp.primitive else { continue };
            let clip = cp.clip_rect;
            let tex = texs.get(&mesh.texture_id);
            for tri in mesh.indices.chunks(3) {
                let v = [mesh.vertices[tri[0] as usize], mesh.vertices[tri[1] as usize], mesh.vertices[tri[2] as usize]];
                let (x0, x1) = (v.iter().map(|q| q.pos.x).fold(f32::MAX, f32::min).max(clip.left()).max(0.0), v.iter().map(|q| q.pos.x).fold(f32::MIN, f32::max).min(clip.right()).min(w as f32));
                let (y0, y1) = (v.iter().map(|q| q.pos.y).fold(f32::MAX, f32::min).max(clip.top()).max(0.0), v.iter().map(|q| q.pos.y).fold(f32::MIN, f32::max).min(clip.bottom()).min(h as f32));
                let area = (v[1].pos.x - v[0].pos.x) * (v[2].pos.y - v[0].pos.y) - (v[2].pos.x - v[0].pos.x) * (v[1].pos.y - v[0].pos.y);
                if area.abs() < 1e-6 {
                    continue;
                }
                for py in (y0.floor() as usize)..(y1.ceil() as usize).min(h) {
                    for px in (x0.floor() as usize)..(x1.ceil() as usize).min(w) {
                        let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                        let e = |a: &epaint::Vertex, b: &epaint::Vertex| (b.pos.x - a.pos.x) * (fy - a.pos.y) - (fx - a.pos.x) * (b.pos.y - a.pos.y);
                        let (w0, w1, w2) = (e(&v[1], &v[2]) / area, e(&v[2], &v[0]) / area, e(&v[0], &v[1]) / area);
                        if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                            continue;
                        }
                        let mut c = [0.0f32; 4];
                        for i in 0..4 {
                            c[i] = (w0 * v[0].color.to_array()[i] as f32 + w1 * v[1].color.to_array()[i] as f32 + w2 * v[2].color.to_array()[i] as f32) / 255.0;
                        }
                        if let Some(t) = tex {
                            let mut u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                            let mut vv = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                            if t.repeat {
                                u = u.rem_euclid(1.0);
                                vv = vv.rem_euclid(1.0);
                            }
                            let tx = ((u * t.w as f32) as usize).min(t.w - 1);
                            let ty = ((vv * t.h as f32) as usize).min(t.h - 1);
                            let s = t.px[ty * t.w + tx];
                            for i in 0..4 {
                                c[i] *= s[i];
                            }
                        }
                        // premultiplied (gamma-space) blend
                        let d = &mut img[py * w + px];
                        for i in 0..3 {
                            d[i] = c[i] + d[i] * (1.0 - c[3]);
                        }
                    }
                }
            }
        }
        let bytes: Vec<u8> = img.iter().flat_map(|p| p.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8).collect::<Vec<_>>()).collect();
        std::fs::write(path, bytes).unwrap();
    }

    fn raw(i: usize, ev: Vec<Event>) -> RawInput {
        RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH as f32, HEIGHT as f32))), time: Some(i as f64 * 0.04), events: ev, ..Default::default() }
    }


    /// Renders both faces to raw RGB files when SHOT_DIR is set.
    #[test]
    fn screenshots() {
        let Ok(dir) = std::env::var("SHOT_DIR") else { return };
        let ctx = egui::Context::default();
        fonts(&ctx);
        let p = KilnParams::default();
        let sh = Shared::default();
        let mock = Mock;
        let setter = ParamSetter::new(&mock);
        let mut st = new_st("KilnShot");
        let mut texs = HashMap::new();
        // a second set with 3 bands + the console EQ on (fields are pub; values can't be set in tests)
        let mut q = KilnParams::default();
        q.bands = IntParam::new("Bands", 3, IntRange::Linear { min: 1, max: 3 });
        q.eq = BoolParam::new("Console EQ", true);
        q.eq_mg = FloatParam::new("EQ Mid Gain", 6.0, FloatRange::Linear { min: -18.0, max: 18.0 });
        q.eq_lg = FloatParam::new("EQ Low Gain", -4.0, FloatRange::Linear { min: -16.0, max: 16.0 });
        q.eq_hpf = IntParam::new("EQ High-Pass", 2, IntRange::Linear { min: 0, max: 4 });
        for (n, (analog, multi)) in [(false, false), (true, false), (false, true), (true, true)].iter().enumerate() {
            let p = if *multi { &q } else { &p };
            st.eq_tab = *multi;
            st.band = if *multi { 1 } else { 0 };
            p.analog.store(*analog, Ordering::Relaxed);
            let mut last = None;
            for f in 0..4 {
                last = Some(ctx.run_ui(raw(n * 10 + f, vec![]), |c| frame(c, &setter, &mut st, p, &sh)));
                if f < 3 {
                    apply_delta(&mut texs, last.as_ref().unwrap());
                }
            }
            render(&ctx, last.unwrap(), &mut texs, &format!("{}/kiln{}.rgb", dir, n));
        }
    }
}

//! Kiln parameters. Ids are short and fixed: presets and sessions depend on them.

use crate::awk_ui;
use nice_plug::prelude::*;
use nice_plug_egui::EguiEditorState;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// (type name, short name for the ANALOG rotary, one-line sound, variants)
pub const TYPES: [(&str, &str, &str, [&str; 3]); 7] = [
    ("PREAMP", "PREAMP", "warm · sweet · thick", ["12AX7", "12AT7", "CLASS A"]),
    ("TUBE POWER", "POWER", "thick · squash · edge", ["EL34", "6L6", "KT88"]),
    ("TAPE", "TAPE", "glue · round highs", ["STUDIO", "CASSETTE", "WORN"]),
    ("IRON", "IRON", "weight · bus density", ["NICKEL", "STEEL", "CONSOLE"]),
    ("SOLID", "SOLID", "bright · punchy · fuzz", ["FET", "GERMANIUM", "SILICON"]),
    ("DIGITAL", "DIGITAL", "clip · fold · crush", ["CLIP", "FOLD", "CRUSH"]),
    // ChowTape's hysteresis model (see chow.rs): reel, cassette, VHS machines
    ("SPECIAL TAPE", "SPECIAL", "chow · real hysteresis", ["STUDIO", "CASSETTE", "WORN"]),
];
pub const T_PRE: i32 = 0;
pub const T_POWER: i32 = 1;
pub const T_TAPE: i32 = 2;
pub const T_IRON: i32 = 3;
pub const T_SOLID: i32 = 4;
pub const T_DIGITAL: i32 = 5;
pub const T_CHOW: i32 = 6;

/// Multiband: up to 6 bands, numbered low to high.
pub const MAXB: usize = 6;
pub const BAND_NAMES: [&str; MAXB] = ["B1", "B2", "B3", "B4", "B5", "B6"];
/// PHASE of the band splits (CLEAN LOWS / HIGHS + BANDS). The oversampling is linear phase either way.
pub const PHASES: [&str; 2] = ["NATURAL", "LINEAR"];
/// LFO shapes and synced lengths (in quarter notes).
pub const LFO_SHAPES: [&str; 5] = ["SINE", "TRI", "SAW", "SQUARE", "RANDOM"];
pub const LFO_DIVS: [(&str, f32); 9] = [("1/16", 0.25), ("1/8", 0.5), ("1/8D", 0.75), ("1/4", 1.0), ("1/4D", 1.5), ("1/2", 2.0), ("1 BAR", 4.0), ("2 BARS", 8.0), ("4 BARS", 16.0)];
/// Modulation sources and per-band destinations (target = band * 5 + what).
pub const MOD_SRCS: [&str; 4] = ["OFF", "LFO 1", "LFO 2", "ENV"];
pub const MOD_WHAT: [&str; 5] = ["DRIVE", "TONE", "MIX", "DYN", "FDBK"];
pub const N_TGT: usize = MAXB * 5;
pub fn mod_target_name(t: i32) -> String {
    let t = t.clamp(0, N_TGT as i32 - 1) as usize;
    format!("{} {}", BAND_NAMES[t / 5], MOD_WHAT[t % 5])
}

/// Per-band extras (the band's own DYNAMICS, tone section, FEEDBACK, FREQ, MIX, LEVEL).
#[derive(Params)]
pub struct BandX {
    /// -100..100 %: + compresses (evens loud and quiet out), - expands (more punch)
    #[id = "dyn"]
    pub dynamics: FloatParam,
    /// tilt around 1 kHz (automation / modulation target; the panel shows the tone section)
    #[id = "btone"]
    pub tone: FloatParam,
    /// tone section after the drive: low shelf, wide mid bell, treble and presence shelves
    #[id = "bbass"]
    pub bass: FloatParam,
    #[id = "bmid"]
    pub mid: FloatParam,
    #[id = "btreb"]
    pub treble: FloatParam,
    #[id = "bpres"]
    pub presence: FloatParam,
    /// feedback around this band's saturator: an inverted comb tuned to FREQ
    #[id = "fb"]
    pub fb: FloatParam,
    #[id = "fbf"]
    pub fb_freq: FloatParam,
    #[id = "bmix"]
    pub mix: FloatParam,
    #[id = "blvl"]
    pub level: FloatParam,
}
impl BandX {
    fn new(i: usize) -> Self {
        let n = |s: &str| format!("B{} {}", i, s);
        Self {
            dynamics: FloatParam::new(n("Dynamics"), 0.0, FloatRange::Linear { min: -1.0, max: 1.0 }).with_unit("%").with_value_to_string(Arc::new(|v| format!("{:+.0}%", v * 100.0))),
            tone: db(&n("Tone"), 0.0, -6.0, 6.0),
            bass: db(&n("Bass"), 0.0, -24.0, 24.0),
            mid: db(&n("Mid"), 0.0, -24.0, 24.0),
            treble: db(&n("Treble"), 0.0, -24.0, 24.0),
            presence: db(&n("Presence"), 0.0, -24.0, 24.0),
            fb: pct(&n("Feedback"), 0.0),
            fb_freq: hz(&n("Feedback Freq"), 250.0, 10.0, 1000.0),
            mix: pct(&n("Mix"), 1.0),
            level: db(&n("Level"), 0.0, -18.0, 12.0),
        }
    }
}

/// One modulation slot: SOURCE -> TARGET by AMOUNT.
#[derive(Params)]
pub struct ModSlot {
    #[id = "msrc"]
    pub src: IntParam,
    #[id = "mtgt"]
    pub target: IntParam,
    #[id = "mamt"]
    pub amount: FloatParam,
}
impl ModSlot {
    fn new(i: usize) -> Self {
        Self {
            src: IntParam::new(format!("Mod {} Source", i), 0, IntRange::Linear { min: 0, max: 3 }).with_value_to_string(Arc::new(|v| MOD_SRCS[v.clamp(0, 3) as usize].to_string())),
            target: IntParam::new(format!("Mod {} Target", i), 0, IntRange::Linear { min: 0, max: N_TGT as i32 - 1 }).with_value_to_string(Arc::new(mod_target_name)),
            amount: FloatParam::new(format!("Mod {} Amount", i), 0.0, FloatRange::Linear { min: -1.0, max: 1.0 }).with_value_to_string(Arc::new(|v| format!("{:+.0}%", v * 100.0))),
        }
    }
}

/// CONSOLE EQ steps (the classic British preamp EQ: stepped frequencies, gain pots).
pub const EQ_HPF: [(&str, f32); 5] = [("OFF", 0.0), ("50", 50.0), ("80", 80.0), ("160", 160.0), ("300", 300.0)];
pub const EQ_LF: [(&str, f32); 4] = [("35", 35.0), ("60", 60.0), ("110", 110.0), ("220", 220.0)];
pub const EQ_MF: [(&str, f32); 6] = [("360", 360.0), ("700", 700.0), ("1.6k", 1600.0), ("3.2k", 3200.0), ("4.8k", 4800.0), ("7.2k", 7200.0)];
pub const EQ_HF: f32 = 12000.0;

/// What COLOR does on each type (shown under the knob).
pub const COLOR_DOES: [&str; 7] = ["BIAS", "SAG", "SPEED", "LOW IRON", "KNEE", "AMOUNT", "SPEED"];
/// What DEPTH does on each type.
pub const DEPTH_DOES: [&str; 7] = ["HUM · DRIFT", "HUM · DRIFT", "WOW · HISS", "NOISE", "NOISE", "NOISE", "WOW · HISS"];

fn pct(name: &str, def: f32) -> FloatParam {
    FloatParam::new(name, def, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(30.0))
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}
fn db(name: &str, def: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, def, FloatRange::Linear { min, max })
        .with_smoother(SmoothingStyle::Linear(30.0))
        .with_unit(" dB")
        .with_value_to_string(Arc::new(|v| format!("{v:+.1}")))
}
fn hz(name: &str, def: f32, min: f32, max: f32) -> FloatParam {
    // (no unit: the formatter already says Hz / kHz)
    FloatParam::new(name, def, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) })
        .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
        .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

#[derive(Params)]
pub struct KilnParams {
    /// window size (hosts keep track of it; not persisted)
    pub editor_state: Arc<EguiEditorState>,
    /// DIGITAL (false) or ANALOG (true) face. Saved with the session, not in presets.
    #[persist = "analog"]
    pub analog: Arc<AtomicBool>,

    #[id = "gmix"]
    pub mix: FloatParam,
    #[id = "bypass"]
    pub bypass: BoolParam,
    /// Window size (1.0 = 100%), saved with the project. 0 = not set: open at the default.
    #[persist = "ui-scale"]
    pub ui_scale: Arc<std::sync::RwLock<f32>>,
    #[id = "safe"]
    pub safe: BoolParam,
    #[id = "os"]
    pub os: IntParam,

    #[id = "type"]
    pub ty: IntParam,
    #[id = "var"]
    pub var: IntParam,
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "color"]
    pub color: FloatParam,
    #[id = "depth"]
    pub depth: FloatParam,
    #[id = "tone"]
    pub tone: FloatParam,
    #[id = "out"]
    pub out: FloatParam,
    #[id = "auto"]
    pub auto: BoolParam,
    /// Below this the signal goes around the drive untouched (20 Hz = off).
    #[id = "slo"]
    pub sat_lo: FloatParam,
    /// Above this the signal goes around the drive untouched (20 kHz = off).
    #[id = "shi"]
    pub sat_hi: FloatParam,

    #[id = "tame"]
    pub tame: BoolParam,
    #[id = "tamt"]
    pub tamt: FloatParam,
    #[id = "tsharp"]
    pub tsharp: FloatParam,
    #[id = "tspeed"]
    pub tspeed: FloatParam,
    #[id = "tlo"]
    pub tlo: FloatParam,
    #[id = "thi"]
    pub thi: FloatParam,
    #[id = "delta"]
    pub delta: BoolParam,

    // ---- 0.2: multiband. Band 1 uses type/var/drive/color above.
    /// 1, 2 or 3 bands
    #[id = "bands"]
    pub bands: IntParam,
    #[id = "x1"]
    pub x1: FloatParam,
    #[id = "x2"]
    pub x2: FloatParam,
    #[id = "type2"]
    pub ty2: IntParam,
    #[id = "var2"]
    pub var2: IntParam,
    #[id = "drive2"]
    pub drive2: FloatParam,
    #[id = "color2"]
    pub color2: FloatParam,
    #[id = "type3"]
    pub ty3: IntParam,
    #[id = "var3"]
    pub var3: IntParam,
    #[id = "drive3"]
    pub drive3: FloatParam,
    #[id = "color3"]
    pub color3: FloatParam,
    #[id = "solo1"]
    pub solo1: BoolParam,
    #[id = "solo2"]
    pub solo2: BoolParam,
    #[id = "solo3"]
    pub solo3: BoolParam,
    #[id = "byp1"]
    pub byp1: BoolParam,
    #[id = "byp2"]
    pub byp2: BoolParam,
    #[id = "byp3"]
    pub byp3: BoolParam,

    // ---- 0.4: up to 6 bands, per-band extras, PHASE, modulation
    #[id = "phase"]
    pub phase: IntParam,
    #[id = "x3"]
    pub x3: FloatParam,
    #[id = "x4"]
    pub x4: FloatParam,
    #[id = "x5"]
    pub x5: FloatParam,
    #[id = "type4"]
    pub ty4: IntParam,
    #[id = "var4"]
    pub var4: IntParam,
    #[id = "drive4"]
    pub drive4: FloatParam,
    #[id = "color4"]
    pub color4: FloatParam,
    #[id = "type5"]
    pub ty5: IntParam,
    #[id = "var5"]
    pub var5: IntParam,
    #[id = "drive5"]
    pub drive5: FloatParam,
    #[id = "color5"]
    pub color5: FloatParam,
    #[id = "type6"]
    pub ty6: IntParam,
    #[id = "var6"]
    pub var6: IntParam,
    #[id = "drive6"]
    pub drive6: FloatParam,
    #[id = "color6"]
    pub color6: FloatParam,
    #[id = "solo4"]
    pub solo4: BoolParam,
    #[id = "solo5"]
    pub solo5: BoolParam,
    #[id = "solo6"]
    pub solo6: BoolParam,
    #[id = "byp4"]
    pub byp4: BoolParam,
    #[id = "byp5"]
    pub byp5: BoolParam,
    #[id = "byp6"]
    pub byp6: BoolParam,
    #[nested(array, group = "Band")]
    pub bx: [BandX; MAXB],
    #[id = "l1rate"]
    pub l1_rate: FloatParam,
    #[id = "l1sync"]
    pub l1_sync: BoolParam,
    #[id = "l1div"]
    pub l1_div: IntParam,
    #[id = "l1shape"]
    pub l1_shape: IntParam,
    #[id = "l2rate"]
    pub l2_rate: FloatParam,
    #[id = "l2sync"]
    pub l2_sync: BoolParam,
    #[id = "l2div"]
    pub l2_div: IntParam,
    #[id = "l2shape"]
    pub l2_shape: IntParam,
    #[id = "eatt"]
    pub env_att: FloatParam,
    #[id = "erel"]
    pub env_rel: FloatParam,
    #[nested(array, group = "Mod")]
    pub ms: [ModSlot; 4],

    // ---- 0.2: CONSOLE EQ
    #[id = "eq"]
    pub eq: BoolParam,
    /// 0 = before the drive (shapes what gets driven), 1 = after
    #[id = "eqpos"]
    pub eq_pos: IntParam,
    #[id = "eqhpf"]
    pub eq_hpf: IntParam,
    #[id = "eqlf"]
    pub eq_lf: IntParam,
    #[id = "eqlg"]
    pub eq_lg: FloatParam,
    #[id = "eqmf"]
    pub eq_mf: IntParam,
    #[id = "eqmg"]
    pub eq_mg: FloatParam,
    #[id = "eqhg"]
    pub eq_hg: FloatParam,
}

fn type_param(name: &str, def: i32) -> IntParam {
    IntParam::new(name, def, IntRange::Linear { min: 0, max: 6 }).with_value_to_string(Arc::new(|v| TYPES[v.clamp(0, 6) as usize].0.to_string()))
}
fn var_param(name: &str) -> IntParam {
    IntParam::new(name, 0, IntRange::Linear { min: 0, max: 2 }).with_value_to_string(Arc::new(|v| ["I", "II", "III"][v.clamp(0, 2) as usize].to_string()))
}
fn steps(name: &str, def: i32, list: &'static [(&'static str, f32)]) -> IntParam {
    IntParam::new(name, def, IntRange::Linear { min: 0, max: list.len() as i32 - 1 }).with_value_to_string(Arc::new(move |v| list[v.clamp(0, list.len() as i32 - 1) as usize].0.to_string()))
}

impl Default for KilnParams {
    fn default() -> Self {
        Self {
            editor_state: EguiEditorState::from_size(nice_plug::editor::dpi::LogicalSize::new((crate::editor::WIDTH) as f64, (crate::editor::HEIGHT) as f64), 1.0),
            analog: Arc::new(AtomicBool::new(false)),
            mix: awk_ui::mix_param(),
            bypass: awk_ui::bypass_param(),
            ui_scale: Arc::new(std::sync::RwLock::new(0.0)),
            safe: awk_ui::safe_param(),
            os: IntParam::new("Oversampling", 1, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| awk_ui::OS_NAMES[v.clamp(0, 3) as usize].to_string()))
                .with_string_to_value(Arc::new(|s| awk_ui::OS_NAMES.iter().position(|n| n.eq_ignore_ascii_case(s.trim())).map(|i| i as i32))),
            ty: IntParam::new("Type", 0, IntRange::Linear { min: 0, max: 6 }).with_value_to_string(Arc::new(|v| TYPES[v.clamp(0, 6) as usize].0.to_string())),
            var: IntParam::new("Variant", 0, IntRange::Linear { min: 0, max: 2 }).with_value_to_string(Arc::new(|v| ["I", "II", "III"][v.clamp(0, 2) as usize].to_string())),
            drive: db("Drive", 9.0, 0.0, 24.0),
            color: pct("Color", 0.35),
            depth: pct("Depth", 0.0),
            tone: db("Tone", 0.0, -6.0, 6.0),
            out: db("Output", 0.0, -12.0, 12.0),
            auto: BoolParam::new("Auto Gain", true),
            sat_lo: hz("Clean Lows Below", 20.0, 20.0, 1000.0).with_value_to_string(Arc::new(|v| if v <= 20.5 { "OFF".into() } else { format!("{v:.0} Hz") })),
            sat_hi: hz("Clean Highs Above", 20000.0, 1500.0, 20000.0).with_value_to_string(Arc::new(|v| if v >= 19900.0 { "OFF".into() } else { format!("{:.1} kHz", v / 1000.0) })),
            tame: BoolParam::new("Tame", false),
            tamt: pct("Tame Amount", 0.5),
            tsharp: pct("Tame Sharpness", 0.6),
            tspeed: pct("Tame Speed", 0.5),
            tlo: hz("Tame Low", 120.0, 40.0, 8000.0),
            thi: hz("Tame High", 14000.0, 2000.0, 20000.0),
            delta: BoolParam::new("Tame Delta", false),
            bands: IntParam::new("Bands", 1, IntRange::Linear { min: 1, max: MAXB as i32 }),
            x1: hz("Split Low", 200.0, 20.0, 20000.0),
            x2: hz("Split High", 3000.0, 20.0, 20000.0),
            ty2: type_param("Mid Type", 2),
            var2: var_param("Mid Variant"),
            drive2: db("Mid Drive", 6.0, 0.0, 24.0),
            color2: pct("Mid Color", 0.35),
            ty3: type_param("High Type", 4),
            var3: var_param("High Variant"),
            drive3: db("High Drive", 3.0, 0.0, 24.0),
            color3: pct("High Color", 0.35),
            solo1: BoolParam::new("Low Solo", false),
            solo2: BoolParam::new("Mid Solo", false),
            solo3: BoolParam::new("High Solo", false),
            byp1: BoolParam::new("Low Bypass", false),
            byp2: BoolParam::new("Mid Bypass", false),
            byp3: BoolParam::new("High Bypass", false),
            phase: IntParam::new("Phase", 0, IntRange::Linear { min: 0, max: 1 }).with_value_to_string(Arc::new(|v| PHASES[v.clamp(0, 1) as usize].to_string())),
            x3: hz("Split 3", 900.0, 20.0, 20000.0),
            x4: hz("Split 4", 7000.0, 20.0, 20000.0),
            x5: hz("Split 5", 60.0, 20.0, 20000.0),
            ty4: type_param("B4 Type", 1),
            var4: var_param("B4 Variant"),
            drive4: db("B4 Drive", 3.0, 0.0, 24.0),
            color4: pct("B4 Color", 0.35),
            ty5: type_param("B5 Type", 6),
            var5: var_param("B5 Variant"),
            drive5: db("B5 Drive", 3.0, 0.0, 24.0),
            color5: pct("B5 Color", 0.35),
            ty6: type_param("B6 Type", 5),
            var6: var_param("B6 Variant"),
            drive6: db("B6 Drive", 3.0, 0.0, 24.0),
            color6: pct("B6 Color", 0.35),
            solo4: BoolParam::new("B4 Solo", false),
            solo5: BoolParam::new("B5 Solo", false),
            solo6: BoolParam::new("B6 Solo", false),
            byp4: BoolParam::new("B4 Bypass", false),
            byp5: BoolParam::new("B5 Bypass", false),
            byp6: BoolParam::new("B6 Bypass", false),
            bx: std::array::from_fn(|i| BandX::new(i + 1)),
            l1_rate: hz("LFO 1 Rate", 1.0, 0.05, 20.0).with_value_to_string(Arc::new(|v| format!("{v:.2} Hz"))),
            l1_sync: BoolParam::new("LFO 1 Sync", true),
            l1_div: IntParam::new("LFO 1 Length", 3, IntRange::Linear { min: 0, max: LFO_DIVS.len() as i32 - 1 }).with_value_to_string(Arc::new(|v| LFO_DIVS[v.clamp(0, 8) as usize].0.to_string())),
            l1_shape: IntParam::new("LFO 1 Shape", 0, IntRange::Linear { min: 0, max: 4 }).with_value_to_string(Arc::new(|v| LFO_SHAPES[v.clamp(0, 4) as usize].to_string())),
            l2_rate: hz("LFO 2 Rate", 0.25, 0.05, 20.0).with_value_to_string(Arc::new(|v| format!("{v:.2} Hz"))),
            l2_sync: BoolParam::new("LFO 2 Sync", false),
            l2_div: IntParam::new("LFO 2 Length", 6, IntRange::Linear { min: 0, max: LFO_DIVS.len() as i32 - 1 }).with_value_to_string(Arc::new(|v| LFO_DIVS[v.clamp(0, 8) as usize].0.to_string())),
            l2_shape: IntParam::new("LFO 2 Shape", 4, IntRange::Linear { min: 0, max: 4 }).with_value_to_string(Arc::new(|v| LFO_SHAPES[v.clamp(0, 4) as usize].to_string())),
            env_att: FloatParam::new("Env Attack", 10.0, FloatRange::Skewed { min: 0.5, max: 200.0, factor: FloatRange::skew_factor(-1.5) }).with_unit(" ms").with_value_to_string(Arc::new(|v| format!("{v:.0} ms"))),
            env_rel: FloatParam::new("Env Release", 150.0, FloatRange::Skewed { min: 10.0, max: 2000.0, factor: FloatRange::skew_factor(-1.5) }).with_unit(" ms").with_value_to_string(Arc::new(|v| format!("{v:.0} ms"))),
            ms: std::array::from_fn(|i| ModSlot::new(i + 1)),
            eq: BoolParam::new("Console EQ", false),
            eq_pos: IntParam::new("EQ Position", 0, IntRange::Linear { min: 0, max: 1 }).with_value_to_string(Arc::new(|v| if v == 0 { "PRE".into() } else { "POST".into() })),
            eq_hpf: steps("EQ High-Pass", 0, &EQ_HPF),
            eq_lf: steps("EQ Low Freq", 1, &EQ_LF),
            eq_lg: db("EQ Low Gain", 0.0, -16.0, 16.0),
            eq_mf: steps("EQ Mid Freq", 2, &EQ_MF),
            eq_mg: db("EQ Mid Gain", 0.0, -18.0, 18.0),
            eq_hg: db("EQ High Gain", 0.0, -16.0, 16.0),
        }
    }
}

impl KilnParams {
    /// band b (0..3): (type, variant, drive, color, solo, bypass)
    pub fn band(&self, b: usize) -> (&IntParam, &IntParam, &FloatParam, &FloatParam, &BoolParam, &BoolParam) {
        match b {
            0 => (&self.ty, &self.var, &self.drive, &self.color, &self.solo1, &self.byp1),
            1 => (&self.ty2, &self.var2, &self.drive2, &self.color2, &self.solo2, &self.byp2),
            2 => (&self.ty3, &self.var3, &self.drive3, &self.color3, &self.solo3, &self.byp3),
            3 => (&self.ty4, &self.var4, &self.drive4, &self.color4, &self.solo4, &self.byp4),
            4 => (&self.ty5, &self.var5, &self.drive5, &self.color5, &self.solo5, &self.byp5),
            _ => (&self.ty6, &self.var6, &self.drive6, &self.color6, &self.solo6, &self.byp6),
        }
    }
    /// the split frequency params in slot order (the engine sorts the ones in use)
    pub fn splits(&self) -> [&FloatParam; MAXB - 1] {
        [&self.x1, &self.x2, &self.x3, &self.x4, &self.x5]
    }
    /// Snap every smoother to its target (tests, and the first block).
    pub fn reset_smoothers(&self) {
        for f in [&self.mix, &self.drive, &self.color, &self.depth, &self.tone, &self.out, &self.tamt, &self.tsharp, &self.tspeed, &self.drive2, &self.drive3, &self.color2, &self.color3, &self.eq_lg, &self.eq_mg, &self.eq_hg] {
            f.smoothed.reset(f.value());
        }
    }
}

//! Kiln parameters. Ids are short and fixed: presets and sessions depend on them.

use crate::awk_ui;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// (type name, short name for the ANALOG rotary, one-line sound, variants)
pub const TYPES: [(&str, &str, &str, [&str; 3]); 6] = [
    ("PREAMP", "PREAMP", "warm · sweet · thick", ["12AX7", "12AT7", "CLASS A"]),
    ("TUBE POWER", "POWER", "thick · squash · edge", ["EL34", "6L6", "KT88"]),
    ("TAPE", "TAPE", "glue · round highs", ["STUDIO", "CASSETTE", "WORN"]),
    ("IRON", "IRON", "weight · bus density", ["NICKEL", "STEEL", "CONSOLE"]),
    ("SOLID", "SOLID", "bright · punchy · fuzz", ["FET", "GERMANIUM", "SILICON"]),
    ("DIGITAL", "DIGITAL", "clip · fold · crush", ["CLIP", "FOLD", "CRUSH"]),
];
pub const T_PRE: i32 = 0;
pub const T_POWER: i32 = 1;
pub const T_TAPE: i32 = 2;
pub const T_IRON: i32 = 3;
pub const T_SOLID: i32 = 4;
pub const T_DIGITAL: i32 = 5;

/// Multiband: band names (1 band = the whole range).
pub const BAND_NAMES: [&str; 3] = ["LOW", "MID", "HIGH"];

/// CONSOLE EQ steps (the classic British preamp EQ: stepped frequencies, gain pots).
pub const EQ_HPF: [(&str, f32); 5] = [("OFF", 0.0), ("50", 50.0), ("80", 80.0), ("160", 160.0), ("300", 300.0)];
pub const EQ_LF: [(&str, f32); 4] = [("35", 35.0), ("60", 60.0), ("110", 110.0), ("220", 220.0)];
pub const EQ_MF: [(&str, f32); 6] = [("360", 360.0), ("700", 700.0), ("1.6k", 1600.0), ("3.2k", 3200.0), ("4.8k", 4800.0), ("7.2k", 7200.0)];
pub const EQ_HF: f32 = 12000.0;

/// What COLOR does on each type (shown under the knob).
pub const COLOR_DOES: [&str; 6] = ["BIAS", "SAG", "SPEED", "LOW IRON", "KNEE", "AMOUNT"];
/// What DEPTH does on each type.
pub const DEPTH_DOES: [&str; 6] = ["HUM · DRIFT", "HUM · DRIFT", "WOW · HISS", "NOISE", "NOISE", "NOISE"];

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
    FloatParam::new(name, def, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) })
        .with_unit(" Hz")
        .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
        .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

#[derive(Params)]
pub struct KilnParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,
    /// DIGITAL (false) or ANALOG (true) face. Saved with the session, not in presets.
    #[persist = "analog"]
    pub analog: Arc<AtomicBool>,

    #[id = "gmix"]
    pub mix: FloatParam,
    #[id = "bypass"]
    pub bypass: BoolParam,
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
    IntParam::new(name, def, IntRange::Linear { min: 0, max: 5 }).with_value_to_string(Arc::new(|v| TYPES[v.clamp(0, 5) as usize].0.to_string()))
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
            editor_state: EguiState::from_size(crate::editor::WIDTH, crate::editor::HEIGHT),
            analog: Arc::new(AtomicBool::new(false)),
            mix: awk_ui::mix_param(),
            bypass: awk_ui::bypass_param(),
            safe: awk_ui::safe_param(),
            os: IntParam::new("Oversampling", 1, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| awk_ui::OS_NAMES[v.clamp(0, 3) as usize].to_string()))
                .with_string_to_value(Arc::new(|s| awk_ui::OS_NAMES.iter().position(|n| n.eq_ignore_ascii_case(s.trim())).map(|i| i as i32))),
            ty: IntParam::new("Type", 0, IntRange::Linear { min: 0, max: 5 }).with_value_to_string(Arc::new(|v| TYPES[v.clamp(0, 5) as usize].0.to_string())),
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
            bands: IntParam::new("Bands", 1, IntRange::Linear { min: 1, max: 3 }),
            x1: hz("Split Low", 200.0, 40.0, 2000.0),
            x2: hz("Split High", 3000.0, 600.0, 14000.0),
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
            _ => (&self.ty3, &self.var3, &self.drive3, &self.color3, &self.solo3, &self.byp3),
        }
    }
    /// Snap every smoother to its target (tests, and the first block).
    pub fn reset_smoothers(&self) {
        for f in [&self.mix, &self.drive, &self.color, &self.depth, &self.tone, &self.out, &self.tamt, &self.tsharp, &self.tspeed, &self.drive2, &self.drive3, &self.color2, &self.color3, &self.eq_lg, &self.eq_mg, &self.eq_hg] {
            f.smoothed.reset(f.value());
        }
    }
}

//! Kiln — awk studios. Saturator with its own de-saturator.
//!
//! in -> TAME (when on, zero latency) -> drive + type (oversampled) -> TONE
//!    -> AUTO GAIN -> MIX with the dry -> OUTPUT -> bypass ramp -> SAFE ceiling.
//! TAME DELTA plays only what TAME removes. Latency = the oversampler's only.

use nice_plug::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

mod analyzer;
mod awk_ui;

mod editor;
mod nice_app;
mod oversample;
pub mod params;
mod presets;
pub mod sat;
pub mod tame;

pub use params::*;
use sat::{Bands3, Blk, ConsoleEq, EqSet, Sat, Split};
use tame::{Tame, BANDS};

/// What the editor reads.
pub struct Shared {
    pub meters: awk_ui::Meters,
    /// TAME cut per band (dB, <= 0)
    pub cut: [AtomicU32; BANDS],
    pub ag_db: AtomicU32,
    pub spec_in: awk_ui::SpecRing,
    pub spec_out: awk_ui::SpecRing,
}
impl Default for Shared {
    fn default() -> Self {
        Self {
            meters: awk_ui::Meters::default(),
            cut: std::array::from_fn(|_| AtomicU32::new(0)),
            ag_db: AtomicU32::new(0),
            spec_in: awk_ui::SpecRing::new(analyzer::FFT),
            spec_out: awk_ui::SpecRing::new(analyzer::FFT),
        }
    }
}
pub fn get(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

const MAX_LAT: usize = 64;

/// One block's parameter values.
#[derive(Clone, Copy, Debug)]
pub struct Vals {
    pub ty: i32,
    pub var: i32,
    pub drive: f32,
    pub color: f32,
    pub depth: f32,
    pub tone: f32,
    pub out: f32,
    pub mix: f32,
    pub auto: bool,
    pub safe: bool,
    pub bypass: bool,
    pub os: i32,
    pub tame: bool,
    pub delta: bool,
    pub tamt: f32,
    pub tsharp: f32,
    pub tspeed: f32,
    pub tlo: f32,
    pub thi: f32,
    pub slo: f32,
    pub shi: f32,
    // 0.2
    pub bands: i32,
    pub x1: f32,
    pub x2: f32,
    pub ty2: i32,
    pub var2: i32,
    pub drive2: f32,
    pub color2: f32,
    pub ty3: i32,
    pub var3: i32,
    pub drive3: f32,
    pub color3: f32,
    pub solo: [bool; 3],
    pub byp: [bool; 3],
    pub eq: bool,
    pub eq_pos: i32,
    pub eq_hpf: i32,
    pub eq_lf: i32,
    pub eq_lg: f32,
    pub eq_mf: i32,
    pub eq_mg: f32,
    pub eq_hg: f32,
}

pub struct Kiln {
    params: Arc<KilnParams>,
    shared: Arc<Shared>,
    sr: f32,
    tame: Tame,
    sat: Sat,
    sat2: Sat,
    sat3: Sat,
    bands: Bands3,
    band_d: [[awk_ui::DryDelay; 2]; 3],
    eq: ConsoleEq,
    dry: [awk_ui::DryDelay; 2],
    split: Split,
    /// the same crossover on the dry, so MIX lines up in phase
    split_dry: Split,
    clean_d: [awk_ui::DryDelay; 2],
    lat: usize,
    byp: awk_ui::BypassRamp,
    ceil: awk_ui::Ceiling,
    meter: awk_ui::PeakFollower,
    tame_fade: f32,
    sm: [f32; 12],
    sm_init: bool,
    /// Tests: change the parameter values each block.
    pub tweak: Option<Box<dyn Fn(&mut Vals) + Send>>,
}

impl Default for Kiln {
    fn default() -> Self {
        let sr = 48000.0;
        Self {
            params: Arc::new(KilnParams::default()),
            shared: Arc::new(Shared::default()),
            sr,
            tame: Tame::new(sr),
            sat: Sat::new(sr, 2),
            sat2: Sat::new(sr, 2),
            sat3: Sat::new(sr, 2),
            bands: Bands3::new(sr),
            band_d: std::array::from_fn(|_| [awk_ui::DryDelay::new(MAX_LAT), awk_ui::DryDelay::new(MAX_LAT)]),
            eq: ConsoleEq::new(sr),
            dry: [awk_ui::DryDelay::new(MAX_LAT), awk_ui::DryDelay::new(MAX_LAT)],
            split: Split::new(sr),
            split_dry: Split::new(sr),
            clean_d: [awk_ui::DryDelay::new(MAX_LAT), awk_ui::DryDelay::new(MAX_LAT)],
            lat: oversample::latency(2),
            byp: awk_ui::BypassRamp::default(),
            ceil: awk_ui::Ceiling::default(),
            meter: awk_ui::PeakFollower::default(),
            tame_fade: 0.0,
            sm: [0.0; 12],
            sm_init: false,
            tweak: None,
        }
    }
}

impl Kiln {
    fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        let f = oversample::factor_for(self.vals().os);
        self.tame = Tame::new(sr);
        self.sat = Sat::new(sr, f);
        self.sat2 = Sat::new(sr, f);
        self.sat3 = Sat::new(sr, f);
        self.bands = Bands3::new(sr);
        self.band_d.iter_mut().flatten().for_each(|d| d.clear());
        self.eq = ConsoleEq::new(sr);
        self.lat = oversample::latency(f);
        self.dry.iter_mut().for_each(|d| d.clear());
        self.split = Split::new(sr);
        self.split_dry = Split::new(sr);
        self.clean_d.iter_mut().for_each(|d| d.clear());
        self.byp.set_sr(sr);
        self.ceil.set_sr(sr);
        self.meter.set_sr(sr);
        self.shared.spec_in.sr.store(sr.to_bits(), Ordering::Relaxed);
        self.shared.spec_out.sr.store(sr.to_bits(), Ordering::Relaxed);
    }

    /// For tests: ready to run at `sr` with smoothers settled.
    pub fn for_test(sr: f32) -> Self {
        let mut k = Self::default();
        k.prepare(sr);
        k
    }
    pub fn params_arc(&self) -> Arc<KilnParams> {
        self.params.clone()
    }
    pub fn latency(&self) -> usize {
        self.lat
    }
    /// Tests: rebuild for a new oversampling setting (the host does this via `initialize`).
    pub fn reprepare(&mut self) {
        let sr = self.sr;
        self.prepare(sr);
    }

    /// The parameter values for this block (tests can change them with `tweak`).
    pub fn vals(&self) -> Vals {
        let p = &self.params;
        let mut v = Vals {
            ty: p.ty.value(),
            var: p.var.value(),
            drive: p.drive.value(),
            color: p.color.value(),
            depth: p.depth.value(),
            tone: p.tone.value(),
            out: p.out.value(),
            mix: p.mix.value(),
            auto: p.auto.value(),
            safe: p.safe.value(),
            bypass: p.bypass.value(),
            os: p.os.value(),
            tame: p.tame.value(),
            delta: p.delta.value(),
            tamt: p.tamt.value(),
            tsharp: p.tsharp.value(),
            tspeed: p.tspeed.value(),
            tlo: p.tlo.value(),
            thi: p.thi.value(),
            slo: p.sat_lo.value(),
            shi: p.sat_hi.value(),
            bands: p.bands.value(),
            x1: p.x1.value(),
            x2: p.x2.value(),
            ty2: p.ty2.value(),
            var2: p.var2.value(),
            drive2: p.drive2.value(),
            color2: p.color2.value(),
            ty3: p.ty3.value(),
            var3: p.var3.value(),
            drive3: p.drive3.value(),
            color3: p.color3.value(),
            solo: [p.solo1.value(), p.solo2.value(), p.solo3.value()],
            byp: [p.byp1.value(), p.byp2.value(), p.byp3.value()],
            eq: p.eq.value(),
            eq_pos: p.eq_pos.value(),
            eq_hpf: p.eq_hpf.value(),
            eq_lf: p.eq_lf.value(),
            eq_lg: p.eq_lg.value(),
            eq_mf: p.eq_mf.value(),
            eq_mg: p.eq_mg.value(),
            eq_hg: p.eq_hg.value(),
        };
        if let Some(t) = &self.tweak {
            t(&mut v);
        }
        v
    }

    /// Runs the whole plugin on plain slices (used by `process` and the tests).
    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let v = self.vals();
        let blk = [Blk::new(v.ty, v.var, v.color, v.depth), Blk::new(v.ty2, v.var2, v.color2, v.depth), Blk::new(v.ty3, v.var3, v.color3, v.depth)];
        let tame_on = v.tame;
        let delta = tame_on && v.delta;
        let fade_c = 1.0 / (0.02 * self.sr);
        self.split.set(v.slo, v.shi);
        self.split_dry.set(v.slo, v.shi);
        let split_on = self.split.active();
        let nb = v.bands.clamp(1, 3) as usize;
        self.bands.set(nb, v.x1, v.x2);
        let any_solo = (0..nb).any(|b| v.solo[b]);
        let eq_pre = v.eq && v.eq_pos == 0;
        let eq_post = v.eq && v.eq_pos != 0;
        let eq_steps = (sat::EQ_HPF_HZ(v.eq_hpf), EQ_LF[v.eq_lf.clamp(0, 3) as usize].1, EQ_MF[v.eq_mf.clamp(0, 5) as usize].1);
        // Smooth the continuous values over ~10 ms (own smoothing so tests can drive it).
        let target = [v.drive, v.tone, if any_solo { 1.0 } else { v.mix }, v.out, v.tamt, v.tsharp, v.tspeed, v.drive2, v.drive3, v.eq_lg, v.eq_mg, v.eq_hg];
        if !self.sm_init {
            self.sm = target;
            self.sm_init = true;
        }
        let a = 1.0 - (-1.0 / (0.01 * self.sr)).exp();
        let lat = self.lat;
        for i in 0..l.len() {
            for k in 0..12 {
                self.sm[k] += a * (target[k] - self.sm[k]);
            }
            let [drive_db, tone, mix, out_db, tamt, tsharp, tspeed, drive2_db, drive3_db, lg, mg, hg] = self.sm;
            if v.eq {
                // gains quantised to 0.05 dB so the filters only recompute when it matters
                let q = |x: f32| (x * 20.0).round() / 20.0;
                self.eq.configure(EqSet { hpf: eq_steps.0, lf: eq_steps.1, lg: q(lg), mf: eq_steps.2, mg: q(mg), hg: q(hg) });
            }
            let (il, ir) = (l[i], r[i]);
            let ts = tame::Settings { amount: tamt, sharp: tsharp, speed: tspeed, lo: v.tlo, hi: v.thi };
            // TAME first (crossfaded in/out so switching never clicks)
            let tgt = if tame_on { 1.0 } else { 0.0 };
            if self.tame_fade != tgt {
                self.tame_fade = if tgt > self.tame_fade { (self.tame_fade + fade_c).min(1.0) } else { (self.tame_fade - fade_c).max(0.0) };
            }
            let (tl, tr) = if self.tame_fade > 0.0 {
                let (a, b) = self.tame.tick(il, ir, &ts);
                (il + (a - il) * self.tame_fade, ir + (b - ir) * self.tame_fade)
            } else {
                (il, ir)
            };
            // CONSOLE EQ before the drive: shapes what gets driven
            let (el, er) = if eq_pre { (self.eq.tick(0, tl), self.eq.tick(1, tr)) } else { (tl, tr) };
            // CLEAN LOWS / HIGHS go around the drive (delayed to line up with it)
            let (ml, cl) = self.split.tick(0, el);
            let (mr, cr) = self.split.tick(1, er);
            // bands: each its own type / drive; bypassed bands pass through (delayed)
            let bl = self.bands.tick(0, ml);
            let br = self.bands.tick(1, mr);
            let drives = [drive_db, drive2_db, drive3_db];
            let (mut wl, mut wr) = (0.0, 0.0);
            for b in 0..nb {
                let audible = !any_solo || v.solo[b];
                let (x, y) = if v.byp[b] {
                    (self.band_d[b][0].tick(bl[b], lat), self.band_d[b][1].tick(br[b], lat))
                } else {
                    let d = 10f32.powf(drives[b] / 20.0);
                    let sat = match b {
                        0 => &mut self.sat,
                        1 => &mut self.sat2,
                        _ => &mut self.sat3,
                    };
                    sat.tick(bl[b], br[b], &blk[b], d, tone, v.auto)
                };
                if audible {
                    wl += x;
                    wr += y;
                }
            }
            if !any_solo {
                wl += self.clean_d[0].tick(cl, lat);
                wr += self.clean_d[1].tick(cr, lat);
            } else {
                self.clean_d[0].tick(cl, lat);
                self.clean_d[1].tick(cr, lat);
            }
            if eq_post {
                wl = self.eq.tick(0, wl);
                wr = self.eq.tick(1, wr);
            }
            let (dl, dr) = if split_on {
                let (a, b) = self.split_dry.tick(0, il);
                let (c, d) = self.split_dry.tick(1, ir);
                (a + b, c + d)
            } else {
                (il, ir)
            };
            let dl = self.dry[0].tick(dl, lat);
            let dr = self.dry[1].tick(dr, lat);
            let out_g = 10f32.powf(out_db / 20.0);
            let (mut ol, mut or) = if delta {
                // only what TAME takes out (no saturation)
                ((il - tl) * out_g, (ir - tr) * out_g)
            } else {
                ((dl + (wl - dl) * mix) * out_g, (dr + (wr - dr) * mix) * out_g)
            };
            let wet = self.byp.next(v.bypass);
            ol = dl + (ol - dl) * wet;
            or = dr + (or - dr) * wet;
            let (ol, or) = self.ceil.tick(ol, or, v.safe);
            self.meter.feed(dl, dr, ol, or);
            self.shared.spec_in.push(0.5 * (il + ir));
            self.shared.spec_out.push(0.5 * (ol + or));
            l[i] = ol;
            r[i] = or;
        }
        for k in 0..BANDS {
            self.shared.cut[k].store(if self.tame_fade > 0.0 { self.tame.cut[k] } else { 0.0 }.to_bits(), Ordering::Relaxed);
        }
        self.shared.ag_db.store(self.sat.ag_db.to_bits(), Ordering::Relaxed);
        self.meter.store(&self.shared.meters);
        self.ceil.store(&self.shared.meters);
    }

}

impl Plugin for Kiln {
    const NAME: &'static str = "Kiln";
    const VENDOR: &'static str = "awk studios";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() }];
    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;
    type Editor = crate::nice_app::AwkEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.shared.clone())
    }

    fn activate(&mut self, _layout: &AudioIOLayout, cfg: &BufferConfig, ctx: &mut impl ActivateContext<Self>) -> bool {
        self.prepare(cfg.sample_rate);
        ctx.set_latency_samples(self.lat as u32);
        true
    }

    fn reset(&mut self) {
        self.tame.clear();
        self.sat.clear();
        self.sat2.clear();
        self.sat3.clear();
        self.bands.clear();
        self.band_d.iter_mut().flatten().for_each(|d| d.clear());
        self.eq.clear();
        self.dry.iter_mut().for_each(|d| d.clear());
        self.split.clear();
        self.split_dry.clear();
        self.clean_d.iter_mut().for_each(|d| d.clear());
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let f = oversample::factor_for(self.vals().os);
        if f != self.sat.factor() {
            // Rebuild for the new oversampling factor (allocates a little; only on change).
            let sr = self.sr;
            self.prepare(sr);
            context.set_latency_samples(self.lat as u32);
        }
        let sl = buffer.as_slice();
        if sl.len() < 2 {
            return ProcessStatus::Normal;
        }
        let (a, b) = sl.split_at_mut(1);
        self.run(&mut *a[0], &mut *b[0]);
        ProcessStatus::Normal
    }
}

impl ClapPlugin for Kiln {
    const CLAP_ID: &'static str = "com.awknode.kiln";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Saturator with a de-resonance layer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Distortion, ClapFeature::Stereo];
}

impl Vst3Plugin for Kiln {
    const VST3_CLASS_ID: [u8; 16] = *b"AwknodeKilnSat01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nice_export_clap!(Kiln);
nice_export_vst3!(Kiln);

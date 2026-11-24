//! Kiln — awk studios. Saturator with its own de-saturator.
//!
//! in -> TAME (when on, zero latency) -> drive + type (oversampled) -> TONE
//!    -> AUTO GAIN -> MIX with the dry -> OUTPUT -> bypass ramp -> SAFE ceiling.
//! TAME DELTA plays only what TAME removes. Latency = the oversampler's (+ the LINEAR split's).
//! 0.4: up to 6 bands, each with DYNAMICS / TILT / FEEDBACK / MIX / LEVEL; PHASE NATURAL or
//! LINEAR; two LFOs + an envelope follower into 4 modulation slots.

use nice_plug::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

mod analyzer;
mod awk_ui;

mod chow;
mod editor;
mod nice_app;
mod oversample;
pub mod params;
mod presets;
pub mod sat;
pub mod tame;

pub use params::*;
use sat::{BandsN, Blk, ConsoleEq, EqSet, LinSplit, Sat, Split, LIN_LAT};
use tame::{Tame, BANDS};

/// What the editor reads.
pub struct Shared {
    pub meters: awk_ui::Meters,
    /// TAME cut per band (dB, <= 0)
    pub cut: [AtomicU32; BANDS],
    pub ag_db: AtomicU32,
    pub spec_in: awk_ui::SpecRing,
    pub spec_out: awk_ui::SpecRing,
    /// modulation now: per target (offset, -1..1 scaled by amount), LFOs, ENV
    pub modv: [AtomicU32; N_TGT],
    pub lfo: [AtomicU32; 2],
    pub env: AtomicU32,
}
impl Default for Shared {
    fn default() -> Self {
        Self {
            meters: awk_ui::Meters::default(),
            cut: std::array::from_fn(|_| AtomicU32::new(0)),
            ag_db: AtomicU32::new(0),
            spec_in: awk_ui::SpecRing::new(analyzer::FFT),
            spec_out: awk_ui::SpecRing::new(analyzer::FFT),
            modv: std::array::from_fn(|_| AtomicU32::new(0)),
            lfo: std::array::from_fn(|_| AtomicU32::new(0)),
            env: AtomicU32::new(0),
        }
    }
}
pub fn get(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

const MAX_LAT: usize = 64;
/// smoothed values: 9 global, then 6 per band
const NG: usize = 9;
const NBV: usize = 6;
const NSM: usize = NG + MAXB * NBV;

/// One block's parameter values (band arrays are low to high in slot order).
#[derive(Clone, Copy, Debug)]
pub struct Vals {
    pub ty: [i32; MAXB],
    pub var: [i32; MAXB],
    pub drive: [f32; MAXB],
    pub color: [f32; MAXB],
    pub solo: [bool; MAXB],
    pub byp: [bool; MAXB],
    pub dynm: [f32; MAXB],
    pub btone: [f32; MAXB],
    /// tone section (dB): bass, mid, treble, presence
    pub tsec: [[f32; 4]; MAXB],
    pub fb: [f32; MAXB],
    pub fbf: [f32; MAXB],
    pub bmix: [f32; MAXB],
    pub blvl: [f32; MAXB],
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
    pub bands: i32,
    pub xs: [f32; MAXB - 1],
    pub phase: i32,
    pub eq: bool,
    pub eq_pos: i32,
    pub eq_hpf: i32,
    pub eq_lf: i32,
    pub eq_lg: f32,
    pub eq_mf: i32,
    pub eq_mg: f32,
    pub eq_hg: f32,
    /// LFO (rate Hz, sync, length index, shape)
    pub lfo: [(f32, bool, i32, i32); 2],
    pub env_att: f32,
    pub env_rel: f32,
    /// (source, target, amount)
    pub mods: [(i32, i32, f32); 4],
}

/// Tone section shapes, fitted to the classic multiband-saturator tone stack
/// (low shelf, wide mid bell, treble shelf, presence shelf): (kind, Hz, Q or shelf slope).
/// kind 0 = low shelf, 1 = peak, 2 = high shelf.
const TONE_SEC: [(u8, f32, f32); 4] = [(0, 118.0, 0.58), (1, 603.0, 0.33), (2, 2505.0, 0.756), (2, 6977.0, 0.993)];

#[derive(Clone, Copy)]
struct Bq {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}
impl Default for Bq {
    fn default() -> Self {
        Self { b: [1.0, 0.0, 0.0], a: [0.0, 0.0], z: [0.0; 2] }
    }
}
impl Bq {
    /// RBJ cookbook peak / shelves (q = shelf slope S for shelves)
    fn set(&mut self, kind: u8, f0: f32, gain_db: f32, q: f32, sr: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * std::f32::consts::PI * (f0 / sr).min(0.49);
        let (s, c) = w.sin_cos();
        let (b0, b1, b2, a0, a1, a2) = if kind == 1 {
            let al = s / (2.0 * q);
            (1.0 + al * a, -2.0 * c, 1.0 - al * a, 1.0 + al / a, -2.0 * c, 1.0 - al / a)
        } else {
            let al = s / 2.0 * ((a + 1.0 / a) * (1.0 / q - 1.0) + 2.0).max(0.0).sqrt();
            let sq = 2.0 * a.sqrt() * al;
            if kind == 0 {
                (a * ((a + 1.0) - (a - 1.0) * c + sq), 2.0 * a * ((a - 1.0) - (a + 1.0) * c), a * ((a + 1.0) - (a - 1.0) * c - sq), (a + 1.0) + (a - 1.0) * c + sq, -2.0 * ((a - 1.0) + (a + 1.0) * c), (a + 1.0) + (a - 1.0) * c - sq)
            } else {
                (a * ((a + 1.0) + (a - 1.0) * c + sq), -2.0 * a * ((a - 1.0) + (a + 1.0) * c), a * ((a + 1.0) + (a - 1.0) * c - sq), (a + 1.0) - (a - 1.0) * c + sq, 2.0 * ((a - 1.0) - (a + 1.0) * c), (a + 1.0) - (a - 1.0) * c - sq)
            }
        };
        self.b = [b0 / a0, b1 / a0, b2 / a0];
        self.a = [a1 / a0, a2 / a0];
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// FEEDBACK delay (one per band and channel): the comb's period is 1 / FREQ.
struct FbLine {
    buf: Vec<f32>,
    pos: usize,
}
impl FbLine {
    fn new(sr: f32) -> Self {
        Self { buf: vec![0.0; (0.11 * sr) as usize + 8], pos: 0 }
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }
    /// the sample from `d` (fractional) samples ago
    #[inline]
    fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 3) as f32);
        let i = d.floor() as usize;
        let fr = d - i as f32;
        let a = self.buf[(self.pos + n - i) % n];
        let b = self.buf[(self.pos + n - i - 1) % n];
        a + (b - a) * fr
    }
    #[inline]
    fn write(&mut self, x: f32) {
        self.pos = (self.pos + 1) % self.buf.len();
        self.buf[self.pos] = x;
    }
}

pub struct Kiln {
    params: Arc<KilnParams>,
    shared: Arc<Shared>,
    sr: f32,
    tame: Tame,
    sats: Vec<Sat>,
    bands: BandsN,
    lin: LinSplit,
    band_d: [[awk_ui::DryDelay; 2]; MAXB],
    eq: ConsoleEq,
    dry: [awk_ui::DryDelay; 2],
    split: Split,
    /// the same crossover on the dry, so MIX lines up in phase
    split_dry: Split,
    clean_d: [awk_ui::DryDelay; 2],
    /// oversampler latency, and the whole plugin's (+ the LINEAR split)
    lat_os: usize,
    lat: usize,
    linear: bool,
    byp: awk_ui::BypassRamp,
    ceil: awk_ui::Ceiling,
    meter: awk_ui::PeakFollower,
    tame_fade: f32,
    sm: [f32; NSM],
    sm_init: bool,
    // per band: envelope + its slow average (DYNAMICS), feedback lines, tone section
    benv: [f32; MAXB],
    bref: [f32; MAXB],
    bms: [[f32; 2]; MAXB],
    fb_l: Vec<FbLine>,
    tsec_f: [[[Bq; 4]; 2]; MAXB],
    tsec_have: [[f32; 4]; MAXB],
    /// FEEDBACK limiter: the band input's slow envelope and the feedback's own
    fb_in: [f32; MAXB],
    fb_lvl: [f32; MAXB],
    // modulation
    lfo_ph: [f32; 2],
    lfo_rnd: [(f32, f32); 2],
    lfo_out: [f32; 2],
    env: f32,
    /// host tempo and position (beats) when playing
    pub tempo: f32,
    pub beat: Option<f64>,
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
            sats: (0..MAXB).map(|_| Sat::new(sr, 2)).collect(),
            bands: BandsN::new(sr),
            lin: LinSplit::new(sr),
            band_d: std::array::from_fn(|_| [awk_ui::DryDelay::new(MAX_LAT), awk_ui::DryDelay::new(MAX_LAT)]),
            eq: ConsoleEq::new(sr),
            dry: [awk_ui::DryDelay::new(MAX_LAT + LIN_LAT), awk_ui::DryDelay::new(MAX_LAT + LIN_LAT)],
            split: Split::new(sr),
            split_dry: Split::new(sr),
            clean_d: [awk_ui::DryDelay::new(MAX_LAT), awk_ui::DryDelay::new(MAX_LAT)],
            lat_os: oversample::latency(2),
            lat: oversample::latency(2),
            linear: false,
            byp: awk_ui::BypassRamp::default(),
            ceil: awk_ui::Ceiling::default(),
            meter: awk_ui::PeakFollower::default(),
            tame_fade: 0.0,
            sm: [0.0; NSM],
            sm_init: false,
            benv: [0.0; MAXB],
            bref: [-30.0; MAXB],
            bms: [[0.0; 2]; MAXB],
            fb_l: (0..MAXB * 2).map(|_| FbLine::new(sr)).collect(),
            tsec_f: [[[Bq::default(); 4]; 2]; MAXB],
            tsec_have: [[0.0; 4]; MAXB],
            fb_in: [0.0; MAXB],
            fb_lvl: [0.0; MAXB],
            lfo_ph: [0.0; 2],
            lfo_rnd: [(0.0, 0.0); 2],
            lfo_out: [0.0; 2],
            env: 0.0,
            tempo: 120.0,
            beat: None,
            tweak: None,
        }
    }
}

impl Kiln {
    fn prepare(&mut self, sr: f32) {
        self.sr = sr;
        let v = self.vals();
        let f = oversample::factor_for(v.os);
        self.tame = Tame::new(sr);
        self.sats = (0..MAXB).map(|_| Sat::new(sr, f)).collect();
        self.bands = BandsN::new(sr);
        self.lin = LinSplit::new(sr);
        self.band_d.iter_mut().flatten().for_each(|d| d.clear());
        self.eq = ConsoleEq::new(sr);
        self.lat_os = oversample::latency(f);
        self.linear = v.phase == 1;
        self.lat = self.lat_os + if self.linear { LIN_LAT } else { 0 };
        self.dry.iter_mut().for_each(|d| d.clear());
        self.split = Split::new(sr);
        self.split_dry = Split::new(sr);
        self.clean_d.iter_mut().for_each(|d| d.clear());
        self.benv = [0.0; MAXB];
        self.bref = [-30.0; MAXB];
        self.bms = [[0.0; 2]; MAXB];
        self.fb_l = (0..MAXB * 2).map(|_| FbLine::new(sr)).collect();
        self.tsec_f = [[[Bq::default(); 4]; 2]; MAXB];
        self.tsec_have = [[0.0; 4]; MAXB];
        self.fb_in = [0.0; MAXB];
        self.fb_lvl = [0.0; MAXB];
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
    /// Tests: rebuild for a new oversampling / PHASE setting (the host does this via `initialize`).
    pub fn reprepare(&mut self) {
        let sr = self.sr;
        self.prepare(sr);
    }

    /// The parameter values for this block (tests can change them with `tweak`).
    pub fn vals(&self) -> Vals {
        let p = &self.params;
        let bnd = |f: &dyn Fn(usize) -> f32| -> [f32; MAXB] { std::array::from_fn(|b| f(b)) };
        let mut v = Vals {
            ty: std::array::from_fn(|b| p.band(b).0.value()),
            var: std::array::from_fn(|b| p.band(b).1.value()),
            drive: bnd(&|b| p.band(b).2.value()),
            color: bnd(&|b| p.band(b).3.value()),
            solo: std::array::from_fn(|b| p.band(b).4.value()),
            byp: std::array::from_fn(|b| p.band(b).5.value()),
            dynm: bnd(&|b| p.bx[b].dynamics.value()),
            btone: bnd(&|b| p.bx[b].tone.value()),
            tsec: std::array::from_fn(|b| [p.bx[b].bass.value(), p.bx[b].mid.value(), p.bx[b].treble.value(), p.bx[b].presence.value()]),
            fb: bnd(&|b| p.bx[b].fb.value()),
            fbf: bnd(&|b| p.bx[b].fb_freq.value()),
            bmix: bnd(&|b| p.bx[b].mix.value()),
            blvl: bnd(&|b| p.bx[b].level.value()),
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
            xs: std::array::from_fn(|j| p.splits()[j].value()),
            phase: p.phase.value(),
            eq: p.eq.value(),
            eq_pos: p.eq_pos.value(),
            eq_hpf: p.eq_hpf.value(),
            eq_lf: p.eq_lf.value(),
            eq_lg: p.eq_lg.value(),
            eq_mf: p.eq_mf.value(),
            eq_mg: p.eq_mg.value(),
            eq_hg: p.eq_hg.value(),
            lfo: [(p.l1_rate.value(), p.l1_sync.value(), p.l1_div.value(), p.l1_shape.value()), (p.l2_rate.value(), p.l2_sync.value(), p.l2_div.value(), p.l2_shape.value())],
            env_att: p.env_att.value(),
            env_rel: p.env_rel.value(),
            mods: std::array::from_fn(|i| (p.ms[i].src.value(), p.ms[i].target.value(), p.ms[i].amount.value())),
        };
        if let Some(t) = &self.tweak {
            t(&mut v);
        }
        v
    }

    /// The splits in use, sorted low to high (band k = the k-th region).
    pub fn sorted_splits(n: usize, xs: &[f32; MAXB - 1]) -> [f32; MAXB - 1] {
        let mut s = [20000.0f32; MAXB - 1];
        let m = n.clamp(1, MAXB) - 1;
        s[..m].copy_from_slice(&xs[..m]);
        s[..m].sort_by(|a, b| a.total_cmp(b));
        s
    }

    fn lfo_shape(shape: i32, ph: f32, rnd: f32) -> f32 {
        use std::f32::consts::TAU;
        match shape {
            0 => (TAU * ph).sin(),
            1 => 1.0 - 4.0 * (ph - 0.5).abs(),
            2 => 2.0 * ph - 1.0,
            3 => if ph < 0.5 { 1.0 } else { -1.0 },
            _ => rnd,
        }
    }

    /// Runs the whole plugin on plain slices (used by `process` and the tests).
    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let v = self.vals();
        let nb = v.bands.clamp(1, MAXB as i32) as usize;
        let blk: [Blk; MAXB] = std::array::from_fn(|b| Blk::new(v.ty[b], v.var[b], v.color[b], v.depth));
        let tame_on = v.tame;
        let delta = tame_on && v.delta;
        let sr = self.sr;
        let fade_c = 1.0 / (0.02 * sr);
        let linear = self.linear;
        let splits = Self::sorted_splits(nb, &v.xs);
        if linear {
            self.lin.set(nb, &splits, v.slo, v.shi);
        } else {
            self.split.set(v.slo, v.shi);
            self.bands.set(nb, &splits);
        }
        self.split_dry.set(v.slo, v.shi);
        let split_on = self.split.active();
        let any_solo = (0..nb).any(|b| v.solo[b]);
        let eq_pre = v.eq && v.eq_pos == 0;
        let eq_post = v.eq && v.eq_pos != 0;
        let eq_steps = (sat::EQ_HPF_HZ(v.eq_hpf), EQ_LF[v.eq_lf.clamp(0, 3) as usize].1, EQ_MF[v.eq_mf.clamp(0, 5) as usize].1);
        // FEEDBACK comb period (the oversampler's latency is already in the loop)
        let fbin_r = 1.0 - (-1.0 / (0.15 * sr)).exp();
        let fbl_a = 1.0 - (-1.0 / (0.003 * sr)).exp();
        let fbd: [f32; MAXB] = std::array::from_fn(|b| (sr / v.fbf[b].clamp(10.0, 1000.0) - self.lat_os as f32).max(1.0));
        let ref_a = 1.0 - (-1.0 / (1.0 * sr)).exp();
        // tone section coefficients (only when they move)
        for b in 0..nb {
            for k in 0..4 {
                let g = (v.tsec[b][k] * 20.0).round() / 20.0;
                if g != self.tsec_have[b][k] {
                    self.tsec_have[b][k] = g;
                    let (kind, f0, q) = TONE_SEC[k];
                    for c in 0..2 {
                        self.tsec_f[b][c][k].set(kind, f0, g, q, sr);
                    }
                }
            }
        }
        // modulation: which targets are used, LFO speeds
        let lfo_hz: [f32; 2] = std::array::from_fn(|i| {
            let (rate, sync, div, _) = v.lfo[i];
            if sync { self.tempo.max(20.0) / 60.0 / LFO_DIVS[div.clamp(0, 8) as usize].1 } else { rate }
        });
        if let Some(beat) = self.beat {
            // follow the host's bar when synced and playing
            for i in 0..2 {
                let (_, sync, div, _) = v.lfo[i];
                if sync {
                    self.lfo_ph[i] = ((beat / LFO_DIVS[div.clamp(0, 8) as usize].1 as f64).fract() as f32).abs();
                }
            }
        }
        let env_a = 1.0 - (-1.0 / (v.env_att * 0.001 * sr)).exp();
        let env_r = 1.0 - (-1.0 / (v.env_rel * 0.001 * sr)).exp();
        let rnd_a = 1.0 - (-1.0 / (0.004 * sr)).exp();
        let any_mod = v.mods.iter().any(|m| m.0 > 0 && m.2 != 0.0);
        let benv_a = 1.0 - (-1.0 / (0.002 * sr)).exp();
        let benv_r = 1.0 - (-1.0 / (0.03 * sr)).exp();
        // Smooth the continuous values over ~10 ms (own smoothing so tests can drive it).
        let mut target = [0.0f32; NSM];
        target[..NG].copy_from_slice(&[v.tone, if any_solo { 1.0 } else { v.mix }, v.out, v.tamt, v.tsharp, v.tspeed, v.eq_lg, v.eq_mg, v.eq_hg]);
        for b in 0..MAXB {
            let o = NG + b * NBV;
            target[o..o + NBV].copy_from_slice(&[v.drive[b], v.btone[b], v.fb[b], v.bmix[b], v.blvl[b], v.dynm[b]]);
        }
        if !self.sm_init {
            self.sm = target;
            self.sm_init = true;
        }
        let a = 1.0 - (-1.0 / (0.01 * sr)).exp();
        let (lat, lat_os) = (self.lat, self.lat_os);
        let mut modo = [0.0f32; N_TGT];
        for i in 0..l.len() {
            for k in 0..NSM {
                self.sm[k] += a * (target[k] - self.sm[k]);
            }
            let tone = self.sm[0];
            let (mix, out_db, tamt, tsharp, tspeed, lg, mg, hg) = (self.sm[1], self.sm[2], self.sm[3], self.sm[4], self.sm[5], self.sm[6], self.sm[7], self.sm[8]);
            if v.eq {
                // gains quantised to 0.05 dB so the filters only recompute when it matters
                let q = |x: f32| (x * 20.0).round() / 20.0;
                self.eq.configure(EqSet { hpf: eq_steps.0, lf: eq_steps.1, lg: q(lg), mf: eq_steps.2, mg: q(mg), hg: q(hg) });
            }
            let (il, ir) = (l[i], r[i]);
            // ---- modulation sources
            let pk = il.abs().max(ir.abs());
            self.env += if pk > self.env { env_a } else { env_r } * (pk - self.env);
            for k in 0..2 {
                let ph = self.lfo_ph[k] + lfo_hz[k] / sr;
                if ph >= 1.0 {
                    self.lfo_rnd[k].0 = awk_ui_rand(&mut self.lfo_rnd[k].1);
                }
                self.lfo_ph[k] = ph.fract();
                let tgt = Self::lfo_shape(v.lfo[k].3, self.lfo_ph[k], self.lfo_rnd[k].0);
                // a touch of smoothing so SQUARE / RANDOM don't click
                self.lfo_out[k] += rnd_a * (tgt - self.lfo_out[k]);
            }
            if any_mod {
                modo = [0.0; N_TGT];
                let env01 = ((20.0 * (self.env + 1e-9).log10() + 60.0) / 60.0).clamp(0.0, 1.0);
                for &(src, tgt, amt) in v.mods.iter() {
                    let s = match src {
                        1 => self.lfo_out[0],
                        2 => self.lfo_out[1],
                        3 => env01,
                        _ => continue,
                    };
                    modo[tgt.clamp(0, N_TGT as i32 - 1) as usize] += s * amt;
                }
            }
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
            // split: bands + CLEAN LOWS / HIGHS (natural crossovers, or LINEAR phase by FFT)
            let (bl, br, cl, cr) = if linear {
                let a = self.lin.tick(0, el);
                let b = self.lin.tick(1, er);
                let mut bl = [0.0; MAXB];
                let mut br = [0.0; MAXB];
                bl.copy_from_slice(&a[..MAXB]);
                br.copy_from_slice(&b[..MAXB]);
                (bl, br, a[MAXB], b[MAXB])
            } else {
                let (ml, cl) = self.split.tick(0, el);
                let (mr, cr) = self.split.tick(1, er);
                (self.bands.tick(0, ml), self.bands.tick(1, mr), cl, cr)
            };
            let (mut wl, mut wr) = (0.0, 0.0);
            for b in 0..nb {
                let audible = !any_solo || v.solo[b];
                let o = NG + b * NBV;
                let mo = b * 5;
                let (x, y) = (bl[b], br[b]);
                let (dx, dy) = (self.band_d[b][0].tick(x, lat_os), self.band_d[b][1].tick(y, lat_os));
                let (ox, oy) = if v.byp[b] {
                    (dx, dy)
                } else {
                    let drive_db = self.sm[o] + modo[mo] * 24.0;
                    let btone = (tone + self.sm[o + 1] + modo[mo + 1] * 6.0).clamp(-12.0, 12.0);
                    let fb = (self.sm[o + 2] + modo[mo + 4]).clamp(0.0, 1.0);
                    let bmix = (self.sm[o + 3] + modo[mo + 2]).clamp(0.0, 1.0);
                    let lvl = 10f32.powf(self.sm[o + 4] / 20.0);
                    let dynv = (self.sm[o + 5] + modo[mo + 3]).clamp(-1.0, 1.0);
                    // DYNAMICS: + compresses (loud parts down, quiet parts up, into the drive),
                    // - expands. Around the band's own slow average level, so the level holds.
                    let e = x.abs().max(y.abs());
                    self.benv[b] += if e > self.benv[b] { benv_a } else { benv_r } * (e - self.benv[b]);
                    let e_db = 20.0 * (self.benv[b] + 1e-9).log10();
                    if e_db > -70.0 {
                        self.bref[b] += ref_a * (e_db - self.bref[b]);
                    }
                    let dyn_db = if dynv != 0.0 {
                        let k = if dynv > 0.0 { 0.5 } else { 0.45 };
                        (-dynv * k * (e_db.max(-70.0) - self.bref[b])).clamp(-18.0, 18.0)
                    } else {
                        0.0
                    };
                    // make-up: match the band's power before / after DYNAMICS (1 s), so the level holds
                    let dg0 = 10f32.powf(dyn_db / 20.0);
                    let p_in = x * x + y * y;
                    self.bms[b][0] += ref_a * (p_in - self.bms[b][0]);
                    self.bms[b][1] += ref_a * (p_in * dg0 * dg0 - self.bms[b][1]);
                    let mk = if dynv != 0.0 && self.bms[b][1] > 1e-12 { (self.bms[b][0] / self.bms[b][1]).sqrt().clamp(0.125, 8.0) } else { 1.0 };
                    let dg = dg0 * mk;
                    let d = 10f32.powf(drive_db.clamp(-24.0, 36.0) / 20.0);
                    let (mut ux, mut uy) = (x * dg, y * dg);
                    // FEEDBACK: the band's output, inverted, back into its input one period
                    // (1 / FREQ) later: a resonant comb, never louder than twice the band's own input
                    self.fb_in[b] = if e > self.fb_in[b] { e } else { self.fb_in[b] + fbin_r * (e - self.fb_in[b]) };
                    if fb > 0.0 {
                        let norm = -1.1 * fb / (d * self.sats[b].ag()).max(0.25);
                        let soft = |s: f32| s / (1.0 + s.abs());
                        let mut fx = soft(norm * self.fb_l[b * 2].read(fbd[b]));
                        let mut fy = soft(norm * self.fb_l[b * 2 + 1].read(fbd[b]));
                        let fe = fx.abs().max(fy.abs());
                        self.fb_lvl[b] += if fe > self.fb_lvl[b] { 1.0 } else { fbl_a } * (fe - self.fb_lvl[b]);
                        let lim = (2.0 * self.fb_in[b] / (self.fb_lvl[b] + 1e-9)).min(1.0);
                        fx *= lim;
                        fy *= lim;
                        ux += fx;
                        uy += fy;
                    }
                    let (mut sx, mut sy) = self.sats[b].tick(ux, uy, &blk[b], d, btone, v.auto);
                    self.fb_l[b * 2].write(sx);
                    self.fb_l[b * 2 + 1].write(sy);
                    // tone section (after the drive)
                    if self.tsec_have[b] != [0.0; 4] {
                        for k in 0..4 {
                            if self.tsec_have[b][k] != 0.0 {
                                sx = self.tsec_f[b][0][k].tick(sx);
                                sy = self.tsec_f[b][1][k].tick(sy);
                            }
                        }
                    }
                    ((dx + (sx * lvl - dx) * bmix), (dy + (sy * lvl - dy) * bmix))
                };
                if audible {
                    wl += ox;
                    wr += oy;
                }
            }
            let (c0, c1) = (self.clean_d[0].tick(cl, lat_os), self.clean_d[1].tick(cr, lat_os));
            if !any_solo {
                wl += c0;
                wr += c1;
            }
            if eq_post {
                wl = self.eq.tick(0, wl);
                wr = self.eq.tick(1, wr);
            }
            let (dl, dr) = if split_on && !linear {
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
        if let Some(b) = self.beat.as_mut() {
            *b += l.len() as f64 / sr as f64 * self.tempo as f64 / 60.0;
        }
        for k in 0..BANDS {
            self.shared.cut[k].store(if self.tame_fade > 0.0 { self.tame.cut[k] } else { 0.0 }.to_bits(), Ordering::Relaxed);
        }
        for k in 0..N_TGT {
            self.shared.modv[k].store(modo[k].to_bits(), Ordering::Relaxed);
        }
        for k in 0..2 {
            self.shared.lfo[k].store(self.lfo_out[k].to_bits(), Ordering::Relaxed);
        }
        self.shared.env.store(self.env.to_bits(), Ordering::Relaxed);
        self.shared.ag_db.store(self.sats[0].ag_db.to_bits(), Ordering::Relaxed);
        self.meter.store(&self.shared.meters);
        self.ceil.store(&self.shared.meters);
    }
}

/// xorshift -> -1..1
fn awk_ui_rand(s: &mut f32) -> f32 {
    let mut x = s.to_bits() ^ 0x9e37_79b9;
    if x == 0 {
        x = 1;
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *s = f32::from_bits(x & 0x007f_ffff | 0x3f80_0000);
    (*s - 1.5) * 2.0
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
        self.sats.iter_mut().for_each(|s| s.clear());
        self.bands.clear();
        self.lin.clear();
        self.fb_l.iter_mut().for_each(|f| f.clear());
        for b in self.tsec_f.iter_mut() {
            for c in b.iter_mut() {
                for f in c.iter_mut() {
                    f.z = [0.0; 2];
                }
            }
        }
        self.band_d.iter_mut().flatten().for_each(|d| d.clear());
        self.eq.clear();
        self.dry.iter_mut().for_each(|d| d.clear());
        self.split.clear();
        self.split_dry.clear();
        self.clean_d.iter_mut().for_each(|d| d.clear());
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let tr = context.transport();
        self.tempo = tr.tempo.unwrap_or(120.0) as f32;
        self.beat = if tr.playing { tr.pos_beats() } else { None };
        let v = self.vals();
        let f = oversample::factor_for(v.os);
        if f != self.sats[0].factor() || (v.phase == 1) != self.linear {
            // Rebuild for the new oversampling factor / PHASE (allocates a little; only on change).
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

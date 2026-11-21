//! Kiln engine tests: levels, silence, safety, latency, TAME, the clean split.
use kiln::*;
use std::f32::consts::PI;

const SR: f32 = 48000.0;

fn sine(f: f32, a: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| a * (2.0 * PI * f * i as f32 / SR).sin()).collect()
}
fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}
fn run(k: &mut Kiln, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (x.to_vec(), x.to_vec());
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        k.run(a, b);
    }
    (l, r)
}
fn kiln(f: impl Fn(&mut Vals) + Send + 'static) -> Kiln {
    let mut k = Kiln::for_test(SR);
    k.tweak = Some(Box::new(f));
    k.reprepare();
    k
}
/// Every type and variant, at max drive and every DEPTH: finite, under the
/// ceiling, and silence in gives silence out (no noise from nowhere).
#[test]
fn all_types_bounded_and_silent_when_silent() {
    for ty in 0..6 {
        for var in 0..3 {
            for os in [0, 1] {
                let mut k = kiln(move |v| {
                    v.ty = ty;
                    v.var = var;
                    v.os = os;
                    v.drive = 24.0;
                    v.depth = 1.0;
                    v.color = 0.8;
                });
                let mut x = sine(110.0, 0.9, 24000);
                x.extend(vec![0.0; 48000]);
                let (l, r) = run(&mut k, &x);
                let pk = l.iter().chain(r.iter()).fold(0.0f32, |a, v| a.max(v.abs()));
                assert!(pk.is_finite() && pk <= 0.9886, "type {ty}/{var} os {os}: peak {pk}");
                let tail = &l[l.len() - 4800..];
                let t = tail.iter().fold(0.0f32, |a, v| a.max(v.abs()));
                assert!(t < 1e-4, "type {ty}/{var}: {t} left in silence");
            }
        }
    }
}

/// AUTO GAIN keeps the level close to the input's, from low to high drive.
#[test]
fn auto_gain_holds_level() {
    for ty in 0..5 {
        for drive in [3.0, 12.0, 24.0] {
            let mut k = kiln(move |v| {
                v.ty = ty;
                v.safe = false;
                v.drive = drive;
            });
            let x = sine(220.0, 0.25, 96000 * 2);
            let (l, _) = run(&mut k, &x);
            let ratio = 20.0 * (rms(&l[l.len() - 24000..]) / rms(&x[x.len() - 24000..])).log10();
            assert!(ratio.abs() < 2.0, "type {ty} drive {drive}: {ratio:.1} dB");
        }
    }
}

/// MIX 0 = the dry, delayed by exactly the reported latency.
#[test]
fn mix_zero_is_dry_and_latency_is_right() {
    for os in 0..4 {
        let mut k = kiln(move |v| {
            v.os = os;
            v.mix = 0.0;
        });
        let x = sine(300.0, 0.5, 4800);
        let (l, _) = run(&mut k, &x);
        let lat = k.latency();
        for i in lat + 10..x.len() {
            assert!((l[i] - x[i - lat]).abs() < 1e-5, "os {os} at {i}");
        }
    }
}

/// CLEAN LOWS: a low note below the split comes out the same at full drive as
/// with no drive at all (it went around the saturation).
#[test]
fn clean_split_keeps_the_lows_clean() {
    let x = sine(50.0, 0.5, 48000);
    let go = |drive: f32| {
        let mut k = kiln(move |v| {
            v.ty = 4;
            v.drive = drive;
            v.auto = false;
            v.safe = false;
            v.slo = 300.0;
        });
        run(&mut k, &x).0
    };
    let (hot, cold) = (go(24.0), go(0.0));
    let d = hot[24000..].iter().zip(&cold[24000..]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(d < 0.03, "lows changed by {d}");
    // and without the split the same note is crushed
    let mut k = kiln(|v| {
        v.ty = 4;
        v.drive = 24.0;
        v.auto = false;
        v.safe = false;
    });
    let full = run(&mut k, &x).0;
    let d2 = full[24000..].iter().zip(&cold[24000..]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(d2 > 0.2, "no split should saturate: {d2}");
}

/// The crossover itself is flat in level (only phase moves): checked on the dry
/// path, which runs through the same crossover.
#[test]
fn clean_split_is_flat() {
    for f in [40.0, 120.0, 400.0, 1500.0, 6000.0, 12000.0] {
        let x = sine(f, 0.3, 48000);
        let mut k = kiln(|v| {
            v.mix = 0.0;
            v.slo = 150.0;
            v.shi = 5000.0;
        });
        let (l, _) = run(&mut k, &x);
        let db = 20.0 * (rms(&l[24000..]) / rms(&x[24000..])).log10();
        assert!(db.abs() < 0.5, "{f} Hz: {db:.2} dB");
    }
}

/// TAME catches a whistle in noise and turns it down.
#[test]
fn tame_turns_down_a_whistle() {
    let mut seed = 7u32;
    let mut noise = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 8388608.0 - 1.0
    };
    let x: Vec<f32> = (0..96000).map(|i| 0.05 * noise() + 0.2 * (2.0 * PI * 2900.0 * i as f32 / SR).sin()).collect();
    let level_at = |on: bool| {
        let mut k = kiln(move |v| {
            v.drive = 0.0;
            v.auto = false;
            v.tame = on;
            v.tamt = 0.8;
        });
        let (l, _) = run(&mut k, &x);
        // correlate with the whistle over the last half second
        let n = l.len();
        let (mut s, mut c) = (0.0f32, 0.0f32);
        for i in n - 24000..n {
            let ph = 2.0 * PI * 2900.0 * i as f32 / SR;
            s += l[i] * ph.sin();
            c += l[i] * ph.cos();
        }
        (s * s + c * c).sqrt()
    };
    let (off, on) = (level_at(false), level_at(true));
    let db = 20.0 * (on / off).log10();
    assert!(db < -4.0, "whistle only down {db:.1} dB");
}

/// The TAME range really limits it: a whistle outside the range is left alone.
#[test]
fn tame_range_is_respected() {
    let mut seed = 7u32;
    let mut noise = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 8388608.0 - 1.0
    };
    let x: Vec<f32> = (0..96000).map(|i| 0.05 * noise() + 0.2 * (2.0 * PI * 2900.0 * i as f32 / SR).sin()).collect();
    let level = |tame: bool, lo: f32, hi: f32| {
        let mut k = kiln(move |v| {
            v.drive = 0.0;
            v.auto = false;
            v.tame = tame;
            v.tamt = 0.8;
            v.tlo = lo;
            v.thi = hi;
        });
        let (l, _) = run(&mut k, &x);
        let n = l.len();
        let (mut s, mut c) = (0.0f32, 0.0f32);
        for i in n - 24000..n {
            let ph = 2.0 * PI * 2900.0 * i as f32 / SR;
            s += l[i] * ph.sin();
            c += l[i] * ph.cos();
        }
        (s * s + c * c).sqrt()
    };
    let off = level(false, 120.0, 14000.0);
    let above = 20.0 * (level(true, 5000.0, 16000.0) / off).log10();
    let below = 20.0 * (level(true, 120.0, 1500.0) / off).log10();
    let inside = 20.0 * (level(true, 1000.0, 6000.0) / off).log10();
    assert!(above.abs() < 1.0 && below.abs() < 1.0, "outside the range it moved: {above:.1} / {below:.1} dB");
    assert!(inside < -4.0, "inside the range only {inside:.1} dB");
}

/// CPU: default settings, 10 s of audio must run well faster than real time.
#[test]
fn cpu_budget() {
    let mut k = kiln(|v| v.tame = true);
    let x = sine(200.0, 0.3, 48000 * 10);
    let t0 = std::time::Instant::now();
    let _ = run(&mut k, &x);
    let el = t0.elapsed().as_secs_f32();
    eprintln!("10 s with TAME + 2x: {:.3} s ({:.2}% of one core)", el, el / 10.0 * 100.0);
    assert!(el < 2.0);
}

fn tone_at(x: &[f32], f: f32, from: usize) -> f32 {
    let (mut s, mut c) = (0.0f32, 0.0f32);
    for (i, v) in x.iter().enumerate().skip(from) {
        let ph = 2.0 * PI * f * i as f32 / SR;
        s += v * ph.sin();
        c += v * ph.cos();
    }
    2.0 * (s * s + c * c).sqrt() / (x.len() - from) as f32
}
fn three_tones() -> Vec<f32> {
    (0..48000).map(|i| {
        let t = i as f32 / SR;
        0.1 * ((2.0 * PI * 80.0 * t).sin() + (2.0 * PI * 1000.0 * t).sin() + (2.0 * PI * 8000.0 * t).sin())
    }).collect()
}

/// 0.2 multiband: 3 bands, all bypassed = the input (crossovers add up flat).
#[test]
fn multiband_bypassed_is_flat() {
    let mut k = kiln(|v| {
        v.bands = 3;
        v.byp = [true; 3];
        v.auto = false;
    });
    let (l, _) = run(&mut k, &three_tones());
    for f in [80.0, 1000.0, 8000.0] {
        let d = 20.0 * (tone_at(&l, f, 12000) / 0.1).log10();
        assert!(d.abs() < 0.3, "{f} Hz moved {d:.2} dB");
    }
}

/// SOLO on the low band plays only the lows.
#[test]
fn solo_low_band() {
    let mut k = kiln(|v| {
        v.bands = 3;
        v.solo = [true, false, false];
        v.byp = [true; 3];
    });
    let (l, _) = run(&mut k, &three_tones());
    let lo = 20.0 * (tone_at(&l, 80.0, 12000) / 0.1).log10();
    let hi = 20.0 * (tone_at(&l, 8000.0, 12000) / 0.1).log10();
    assert!(lo.abs() < 0.5 && hi < -40.0, "low {lo:.1} dB, high {hi:.1} dB");
}

/// CONSOLE EQ: +12 dB on the mid at 1.6k boosts a 1.6k tone by about 12 dB.
#[test]
fn console_eq_mid_boost() {
    let x: Vec<f32> = (0..48000).map(|i| 0.05 * (2.0 * PI * 1600.0 * i as f32 / SR).sin()).collect();
    let mut k = kiln(|v| {
        v.eq = true;
        v.eq_pos = 1;
        v.eq_mf = 2;
        v.eq_mg = 12.0;
        v.byp = [true; 3];
    });
    let (l, _) = run(&mut k, &x);
    let d = 20.0 * (tone_at(&l, 1600.0, 12000) / 0.05).log10();
    assert!((d - 12.0).abs() < 1.0, "boost {d:.1} dB");
}

#[test]
fn cpu_three_bands() {
    let mut k = kiln(|v| {
        v.bands = 3;
        v.tame = true;
        v.eq = true;
    });
    let x: Vec<f32> = (0..480000).map(|i| 0.3 * (i as f32 * 0.01).sin()).collect();
    let t = std::time::Instant::now();
    run(&mut k, &x);
    let pct = t.elapsed().as_secs_f32() / 10.0 * 100.0;
    println!("3 bands + TAME + EQ at 2x: {pct:.2}% of one core");
    assert!(pct < 15.0);
}

//! Factory presets. `cargo test --test factory_presets` checks every preset only
//! names real params with legal values; with PRESET_OUT=dir it also writes the
//! .awkp files there (anything a preset doesn't mention loads at its default).
use kiln::KilnParams;
use nih_plug::prelude::*;
use serde_json::{json, Map, Value};

type P = &'static [(&'static str, f64)];
const TAME: (&str, f64) = ("tame", 1.0);

const PRESETS: &[(&str, P)] = &[
    ("VOCAL - Warm Tube Glow", &[("type", 0.0), ("var", 0.0), ("drive", 8.0), ("color", 0.3), TAME, ("tamt", 0.45)]),
    ("VOCAL - Console Pre Thick", &[("type", 0.0), ("var", 2.0), ("drive", 12.0), ("color", 0.4), TAME, ("tamt", 0.4)]),
    ("VOCAL - De-Harsh Then Tape", &[("type", 2.0), ("var", 0.0), ("drive", 7.0), TAME, ("tamt", 0.7), ("tsharp", 0.7)]),
    ("VOCAL - Silky 12AT7", &[("type", 0.0), ("var", 1.0), ("drive", 10.0), ("tone", 1.0)]),
    ("VOCAL - Edge, Lows Clean", &[("type", 4.0), ("var", 0.0), ("drive", 12.0), ("slo", 250.0), ("gmix", 0.5)]),
    ("DRUMS - Bus Glue Tape", &[("type", 2.0), ("var", 0.0), ("drive", 10.0), ("color", 0.5)]),
    ("DRUMS - Kick Crunch, Sub Clean", &[("type", 4.0), ("var", 2.0), ("drive", 14.0), ("slo", 120.0)]),
    ("DRUMS - Snare Fuzz", &[("type", 4.0), ("var", 1.0), ("drive", 16.0), ("gmix", 0.5)]),
    ("DRUMS - Smash Clip", &[("type", 5.0), ("var", 0.0), ("drive", 12.0), ("color", 0.3), ("gmix", 0.6)]),
    ("DRUMS - Cassette Crunch", &[("type", 2.0), ("var", 1.0), ("drive", 14.0), ("depth", 0.4)]),
    ("DRUMS - Harsh Cymbals Tamed", &[("type", 3.0), ("var", 0.0), ("drive", 2.0), TAME, ("tamt", 0.7), ("tsharp", 0.8), ("tlo", 3000.0), ("thi", 16000.0)]),
    ("808 - Grit Keeps The Sub", &[("type", 1.0), ("var", 0.0), ("drive", 14.0), ("slo", 90.0)]),
    ("808 - Fold Growl", &[("type", 5.0), ("var", 1.0), ("drive", 10.0), ("color", 0.4), ("slo", 70.0), ("gmix", 0.7)]),
    ("BASS - Iron Weight", &[("type", 3.0), ("var", 1.0), ("drive", 10.0), ("color", 0.5)]),
    ("BASS - Germanium Bite", &[("type", 4.0), ("var", 1.0), ("drive", 11.0), ("slo", 100.0), ("shi", 6000.0)]),
    ("MELODY - Worn Tape Haze", &[("type", 2.0), ("var", 2.0), ("drive", 9.0), ("depth", 0.6), ("tone", -2.0)]),
    ("MELODY - EL34 Crunch Keys", &[("type", 1.0), ("var", 0.0), ("drive", 12.0), ("color", 0.5)]),
    ("MELODY - 6L6 Clean Push", &[("type", 1.0), ("var", 1.0), ("drive", 9.0)]),
    ("MELODY - Bitcrush Lo-Fi", &[("type", 5.0), ("var", 2.0), ("drive", 6.0), ("color", 0.6), ("gmix", 0.7)]),
    ("MELODY - Pad Tamer", &[("type", 3.0), ("var", 0.0), ("drive", 3.0), TAME, ("tamt", 0.8), ("tsharp", 0.4)]),
    ("MELODY - Valve Hum + Drift", &[("type", 0.0), ("var", 0.0), ("drive", 10.0), ("depth", 0.5), ("color", 0.5)]),
    ("BUS - Console Iron", &[("type", 3.0), ("var", 2.0), ("drive", 6.0), ("color", 0.3)]),
    ("BUS - KT88 Headroom", &[("type", 1.0), ("var", 2.0), ("drive", 8.0), ("gmix", 0.6)]),
    ("MASTER - Gentle Tape", &[("type", 2.0), ("var", 0.0), ("drive", 4.0), ("color", 0.6), ("gmix", 0.5), TAME, ("tamt", 0.3)]),
    ("MASTER - Nickel Polish", &[("type", 3.0), ("var", 0.0), ("drive", 4.0), ("gmix", 0.5)]),
    // 0.2: multiband + CONSOLE EQ
    ("DRUMS - 3-Band Bus (Tube, Tape, Clean Top)", &[("bands", 3.0), ("x1", 150.0), ("x2", 4000.0), ("type", 0.0), ("drive", 10.0), ("type2", 2.0), ("drive2", 8.0), ("byp3", 1.0)]),
    ("808 - Sub Clean, Grit On Top (2 bands)", &[("bands", 2.0), ("x1", 110.0), ("byp1", 1.0), ("type2", 4.0), ("var2", 1.0), ("drive2", 14.0)]),
    ("BASS - Console Pre + Iron", &[("type", 3.0), ("var", 2.0), ("drive", 8.0), ("eq", 1.0), ("eqhpf", 1.0), ("eqlf", 2.0), ("eqlg", 4.0), ("eqmf", 1.0), ("eqmg", -3.0)]),
    ("VOCAL - Console Strip (Class A + EQ)", &[("type", 0.0), ("var", 2.0), ("drive", 10.0), ("eq", 1.0), ("eqhpf", 2.0), ("eqmf", 3.0), ("eqmg", 3.0), ("eqhg", 4.0), TAME, ("tamt", 0.4)]),
    ("VOCAL - Warm Low-Mid, Airy Top (3 bands)", &[("bands", 3.0), ("x1", 300.0), ("x2", 6000.0), ("type", 0.0), ("drive", 9.0), ("type2", 0.0), ("var2", 2.0), ("drive2", 7.0), ("type3", 2.0), ("drive3", 4.0)]),
    ("MELODY - Keys Through The Desk", &[("type", 0.0), ("var", 2.0), ("drive", 12.0), ("eq", 1.0), ("eqpos", 1.0), ("eqlf", 1.0), ("eqlg", 3.0), ("eqmf", 2.0), ("eqmg", -2.0), ("eqhg", 2.0)]),
    ("BUS - Glue Lows, Clip Mids (3 bands)", &[("bands", 3.0), ("x1", 180.0), ("x2", 3500.0), ("type", 3.0), ("drive", 8.0), ("type2", 5.0), ("drive2", 6.0), ("color2", 0.2), ("byp3", 1.0), ("gmix", 0.7)]),
    ("MASTER - Console Polish", &[("type", 3.0), ("var", 0.0), ("drive", 4.0), ("gmix", 0.6), ("eq", 1.0), ("eqpos", 1.0), ("eqlf", 0.0), ("eqlg", 1.5), ("eqhg", 1.5)]),
    ("FX - Destroyed (SAFE holds it)", &[("type", 5.0), ("var", 1.0), ("drive", 24.0), ("color", 1.0), ("gmix", 0.5)]),
];

fn build(params: &KilnParams, over: P) -> Value {
    let map = params.param_map();
    let mut out = Map::new();
    for (id, v) in over {
        let ptr = map.iter().find(|(i, _, _)| i == id).unwrap_or_else(|| panic!("no param {id}")).1;
        let val = unsafe {
            match ptr {
                ParamPtr::FloatParam(p) => {
                    let p = &*p;
                    let n = p.preview_normalized(*v as f32);
                    assert!((p.preview_plain(n) - *v as f32).abs() < 1e-3 * (1.0 + v.abs() as f32), "{id} out of range: {v}");
                    json!({ "f32": *v as f32 })
                }
                ParamPtr::IntParam(p) => {
                    let p = &*p;
                    let n = p.preview_normalized(*v as i32);
                    assert_eq!(p.preview_plain(n), *v as i32, "{id} out of range: {v}");
                    json!({ "i32": *v as i32 })
                }
                ParamPtr::BoolParam(_) => json!({ "bool": *v != 0.0 }),
                _ => panic!("{id}: unexpected param kind"),
            }
        };
        out.insert(id.to_string(), val);
    }
    json!({ "version": env!("CARGO_PKG_VERSION"), "params": out, "fields": {} })
}

#[test]
fn factory_presets_are_valid() {
    let params = KilnParams::default();
    let dir = std::env::var("PRESET_OUT").ok();
    for (name, over) in PRESETS {
        let v = build(&params, over);
        let _: nih_plug::wrapper::state::PluginState = serde_json::from_value(v.clone()).unwrap();
        if let Some(d) = &dir {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(format!("{d}/{name}.awkp"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
        }
    }
    assert!(PRESETS.len() >= 25);
}

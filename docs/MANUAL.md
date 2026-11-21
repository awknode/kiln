# KILN manual

## Signal flow

```
in ─► TAME (zero latency) ─► [CONSOLE EQ pre] ─► DRIVE + TYPE (oversampled) ─► [CONSOLE EQ post] ─► TONE ─► AUTO GAIN ─► MIX ─► OUTPUT ─► SAFE
```

## Types and variants

| Type | Character | Variants | COLOR does | DEPTH does |
|---|---|---|---|---|
| PREAMP | warm, sweet, thick | 12AX7, 12AT7, CLASS A | BIAS | HUM, DRIFT |
| TUBE POWER | thick, squash, edge | EL34, 6L6, KT88 | SAG | HUM, DRIFT |
| TAPE | glue, round highs | STUDIO, CASSETTE, WORN | SPEED | WOW, HISS |
| IRON | weight, bus density | NICKEL, STEEL, CONSOLE | LOW IRON | NOISE |
| SOLID | bright, punchy, fuzz | FET, GERMANIUM, SILICON | KNEE | NOISE |
| DIGITAL | clip, fold, crush | CLIP, FOLD, CRUSH | AMOUNT | NOISE |

## Saturation

- **DRIVE**: how hard the type is pushed.
- **COLOR** and **DEPTH**: their job changes with the type (see the table).
- **TONE**: tilt after the drive.
- **LO / HI range**: below LO and above HI the signal goes around the drive untouched. At 20 Hz and 20 kHz the range is off.
- **AUTO GAIN**: keeps the output level close to the input, so you judge tone and not loudness.

## TAME

- **AMOUNT, SHARP, SPEED**: how much, how narrow and how fast the resonances are pulled down before the drive.
- Drag the TAME range on the spectrum to limit where it works.
- **TAME DELTA**: hear only what TAME removes.

## Bands

Set **BANDS** to 1, 2 or 3. Each band gets its own type, variant, drive and colour. **Solo** lets you hear one band, and **bypass** lets a band through without saturation.

## CONSOLE EQ

A stepped high-pass (OFF / 50 / 80 / 160 / 300 Hz), a low shelf, a mid bell and a high shelf, modelled on a classic British preamp EQ. **EQ POSITION** puts it before the drive (shaping what gets driven) or after.

## Faces

Switch between DIGITAL and ANALOG with the face switch. The choice is saved with the session, not in presets.

//! Curated "spectacular" chaser presets that demonstrate combinations of
//! pattern + colour mode the user might not stumble into on their own. All
//! presets ship `enabled: false` so flipping them on is an explicit act —
//! nobody likes opening the app to find their rig already strobing.

use uuid::Uuid;

use crate::show::fixture::FixtureInstance;

use super::{
    AmbientChaser, Cadence, ChaserSlot, ColorMode, FadeConfig, PaletteRotation, Pattern, Rgb,
    Subdivision, TempoSource,
};

/// Build the example chasers, populating each one's slots from the show's
/// current fixtures. Slots default to `use_intensity = use_color = true`;
/// fixtures without colour channels gracefully no-op on the colour writes
/// inside the engine, so dimmer-only rigs still get the intensity dance.
pub fn example_chasers(fixtures: &[FixtureInstance]) -> Vec<AmbientChaser> {
    let slots: Vec<ChaserSlot> = fixtures
        .iter()
        .map(|f| ChaserSlot {
            fixture_id: f.id.clone(),
            use_intensity: true,
            use_color: true,
        })
        .collect();
    let new_id = || Uuid::new_v4().to_string();

    vec![
        // Centre-out pulse with a rainbow that drifts a quarter-turn every
        // few beats. Looks great with 6+ fixtures arranged in a row.
        AmbientChaser {
            id: new_id(),
            name: "Cyber Pulse".to_string(),
            enabled: false,
            slots: slots.clone(),
            pattern: Pattern::CenterOut,
            color_mode: ColorMode::Rainbow {
                speed: 30.0,
                spread: 1.0,
            },
            tempo: TempoSource::Fixed { bpm: 120.0 },
            subdivision: Subdivision::One,
            master: 1.0,
            background: 0,
            fade: FadeConfig::default(),
        },
        // Hard cyan/magenta strobe at house-music tempo. Subdivision Half
        // gives ~234 ms per flash at 128 BPM — fast enough to read as a
        // strobe, slow enough not to give the photosensitive crowd a fit.
        AmbientChaser {
            id: new_id(),
            name: "Cyan / Magenta Flash".to_string(),
            enabled: false,
            slots: slots.clone(),
            pattern: Pattern::AllTogether,
            color_mode: ColorMode::TwoColorCadence {
                color_a: Rgb { r: 0, g: 200, b: 255 },
                color_b: Rgb { r: 255, g: 0, b: 200 },
                cadence: Cadence::EveryStep,
            },
            tempo: TempoSource::Fixed { bpm: 128.0 },
            subdivision: Subdivision::Half,
            master: 1.0,
            background: 0,
            fade: FadeConfig::default(),
        },
        // Slow comet that sweeps with a barely-shifting rainbow. Subtle
        // ambient layer for chill sections — keep `background` at ~30 so
        // the off-slots glow softly rather than going pitch-black.
        AmbientChaser {
            id: new_id(),
            name: "Rainbow Wave".to_string(),
            enabled: false,
            slots: slots.clone(),
            pattern: Pattern::Wave,
            color_mode: ColorMode::Rainbow {
                speed: 5.0,
                spread: 1.5,
            },
            tempo: TempoSource::Fixed { bpm: 110.0 },
            subdivision: Subdivision::Half,
            master: 1.0,
            background: 30,
            fade: FadeConfig::default(),
        },
        // Two converging chases that cycle through six saturated colours
        // every cycle. Best with even fixture counts (the centre slots get
        // the on-beat hit).
        AmbientChaser {
            id: new_id(),
            name: "Symmetric Spectrum".to_string(),
            enabled: false,
            slots: slots.clone(),
            pattern: Pattern::Symmetric,
            color_mode: ColorMode::Palette {
                colors: vec![
                    Rgb { r: 255, g: 0, b: 0 },
                    Rgb { r: 255, g: 0, b: 200 },
                    Rgb { r: 0, g: 0, b: 255 },
                    Rgb { r: 0, g: 255, b: 255 },
                    Rgb { r: 0, g: 255, b: 0 },
                    Rgb { r: 255, g: 255, b: 0 },
                ],
                rotation: PaletteRotation::PerCycle,
            },
            tempo: TempoSource::Fixed { bpm: 120.0 },
            subdivision: Subdivision::One,
            master: 1.0,
            background: 0,
            fade: FadeConfig::default(),
        },
        // Classic theatre marquee: warm yellow chase, then warm white, then
        // back. ChasePerColor swaps the colour every full lap so it reads
        // as a single light moving forever rather than alternating dots.
        AmbientChaser {
            id: new_id(),
            name: "Theater Marquee".to_string(),
            enabled: false,
            slots: slots.clone(),
            pattern: Pattern::Chase,
            color_mode: ColorMode::TwoColorCadence {
                color_a: Rgb { r: 255, g: 200, b: 80 },
                color_b: Rgb { r: 255, g: 230, b: 200 },
                cadence: Cadence::ChasePerColor,
            },
            tempo: TempoSource::Fixed { bpm: 110.0 },
            subdivision: Subdivision::One,
            master: 1.0,
            background: 20,
            fade: FadeConfig::default(),
        },
        // Build-up that lights one extra slot per sixteenth-note, blacks out
        // for one frame, and restarts. Drop a beat on the fill and you're
        // syncing the lights to the music without thinking about it.
        AmbientChaser {
            id: new_id(),
            name: "Build & Drop".to_string(),
            enabled: false,
            slots,
            pattern: Pattern::Build,
            color_mode: ColorMode::Single {
                color: Rgb {
                    r: 255,
                    g: 100,
                    b: 0,
                },
            },
            tempo: TempoSource::Fixed { bpm: 128.0 },
            subdivision: Subdivision::Quarter,
            master: 1.0,
            background: 0,
            fade: FadeConfig::default(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_have_unique_ids() {
        let presets = example_chasers(&[]);
        let ids: Vec<&str> = presets.iter().map(|c| c.id.as_str()).collect();
        let unique = ids.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), unique.len(), "duplicate ids in presets");
    }

    #[test]
    fn examples_attach_one_slot_per_fixture() {
        let fixtures = vec![FixtureInstance {
            id: "f1".into(),
            definition_id: "d1".into(),
            mode_index: 0,
            universe: 0,
            address: 1,
            label: None,
            position: [0.0, 0.0],
        }];
        let presets = example_chasers(&fixtures);
        for p in presets {
            assert_eq!(p.slots.len(), 1, "{}: slot count", p.name);
            assert_eq!(p.slots[0].fixture_id, "f1");
        }
    }

    #[test]
    fn examples_ship_disabled() {
        for c in example_chasers(&[]) {
            assert!(!c.enabled, "{} ships enabled", c.name);
        }
    }
}

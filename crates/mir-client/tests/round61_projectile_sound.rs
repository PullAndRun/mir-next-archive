//! R61 D2 evidence: 125 projectile sound policy + a real offline `rodio` mixing chain.
//!
//! # What this file proves
//!
//! * M3 (门下评审 high): Crystal `SoundManager.PlaySound` creates a new `OneShotProvider`
//!   for every call and feeds it into the mixer (`Client/MirSounds/SoundManager.cs:94-105`),
//!   with no same-file 100 ms cooldown. The 125 `S.ObjectProjectile` spawn/impact cues must
//!   therefore be submitted every time (2 and 33 copies at the same instant included),
//!   while the generic `play`/`play_event`/`play_spell` entries keep their 100 ms cooldown
//!   and neither policy pollutes the other.
//! * M4 (门下评审 high): the cue submission chain (`SoundLibrary::resolve` ->
//!   decoder/amplify -> output submission) is sampled from a **real** `rodio` mixer
//!   (`rodio::mixer::mixer`, 2 channels / 48 kHz) through its `MixerSource`; no test
//!   counter stands in for samples. Assertions cover both owned `M34-1.mna` /
//!   `M34-2.mna` clips (non-empty/finite/non-zero samples, mixer channel/rate metadata,
//!   sane peak/energy), gain scaling (0.25 -> quarter amplitude), linear overlap
//!   (N simultaneous cues sum), skipped paths (disabled/silent/missing/undecodable ->
//!   `false`, nothing submitted) and a bounded pull that must not panic.
//!
//! # Environment / PARTIAL
//!
//! The acceptance host has no guaranteed audio device, so this file injects an offline
//! `rodio::Mixer` instead of a device sink (no new dependency, no runtime config, no
//! device required). Offline samples do **not** prove audible playback on a device or in
//! Live play (P-1 stays PARTIAL); see `target/round61/d2/attempt-1/00-summary.md`.
//! When the `M34-*` assets are absent the asset-dependent gates print `SKIP` and return
//! (R-4, no substituted `M34-0`).
//!
//! Run: `cargo test -p mir-client --test round61_projectile_sound -- --nocapture`

use std::path::Path;

use mir_client::audio::{SoundLibrary, SoundPlayer, SoundThrottle};
use mir_core::enums::Spell;
use mir_render::sound::{ProjectileSoundPhase, SOUND_COOLDOWN_MS, projectile_sound};
use rodio::Source;

/// Offline evidence mixer channels (same layout as a typical device stream).
const MIXER_CHANNELS: u16 = 2;
/// Offline evidence mixer sample rate.
const MIXER_SAMPLE_RATE: u32 = 48_000;
/// Bounded pull used by the "finite pull must not panic" gate.
const FINITE_PULL_SAMPLES: usize = 4_096;
/// Safety cap for a full clip drain: a source that never terminates must fail, not hang.
const MAX_CLIP_SAMPLES: usize = 2_000_000;
/// Gain used by the 0.25 scaling gate; the shared `amplify` path must scale every sample.
const QUARTER_GAIN: f32 = 0.25;

/// Build a real `rodio` mixer plus the `MixerSource` that samples it (no device).
fn offline_mixer() -> (rodio::mixer::Mixer, rodio::mixer::MixerSource) {
    let channels = rodio::ChannelCount::new(MIXER_CHANNELS).expect("mixer channel count");
    let sample_rate = rodio::SampleRate::new(MIXER_SAMPLE_RATE).expect("mixer sample rate");
    rodio::mixer::mixer(channels, sample_rate)
}

/// `SoundPlayer` whose output is a caller-owned offline mixer (real sampling, no device).
fn offline_player(
    library: SoundLibrary,
    enabled: bool,
    volume: f32,
) -> (SoundPlayer, rodio::mixer::MixerSource) {
    let (mixer, source) = offline_mixer();
    (
        SoundPlayer::with_offline_mixer(library, enabled, volume, mixer),
        source,
    )
}

/// Repo-root anchored library, built like `main.rs` does (covers `game-assets/audio*`).
fn repo_audio_library() -> SoundLibrary {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut library = SoundLibrary::from_repo_root(&repo);
    library.push_back(&SoundLibrary::with_roots(vec![
        repo.join("game-assets/audio"),
        repo.join("game-assets/audio/sfx"),
    ]));
    library
}

/// R-4: both owned M34 clips must resolve; otherwise record SKIP and mark the gate partial.
fn library_with_m34_clips() -> Option<SoundLibrary> {
    let library = repo_audio_library();
    for file in ["M34-1.wav", "M34-2.wav"] {
        if library.resolve(file).is_none() {
            eprintln!(
                "SKIP: missing {} under {:?} (asset evidence is PARTIAL; no M34-0 substitution)",
                file.trim_end_matches(".wav"),
                library.roots()
            );
            return None;
        }
    }
    Some(library)
}

/// Pull at most `max` samples (the bounded pull used by the finite gate).
fn pull_finite(source: &mut rodio::mixer::MixerSource, max: usize) -> Vec<f32> {
    let mut samples = Vec::with_capacity(max.min(16_384));
    for _ in 0..max {
        match source.next() {
            Some(sample) => samples.push(sample),
            None => break,
        }
    }
    samples
}

/// Drain the whole mixed clip (bounded by [`MAX_CLIP_SAMPLES`]).
fn pull_clip(source: &mut rodio::mixer::MixerSource) -> Vec<f32> {
    let mut samples = Vec::new();
    for sample in source.by_ref() {
        samples.push(sample);
        assert!(
            samples.len() <= MAX_CLIP_SAMPLES,
            "offline mixer source must terminate: more than {MAX_CLIP_SAMPLES} samples"
        );
    }
    samples
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

/// Sum of squared samples (the raw "sample energy" behind the RMS figure).
fn energy(samples: &[f32]) -> f64 {
    samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum()
}

/// RMS amplitude; this is the quantity that scales linearly with gain (energy scales gain^2).
fn rms(samples: &[f32]) -> f64 {
    assert!(!samples.is_empty(), "RMS of an empty clip is undefined");
    let len = u32::try_from(samples.len()).expect("clip length fits in u32");
    (energy(samples) / f64::from(len)).sqrt()
}

/// Assert the mixed clip is real, audible signal (not silence, not a runaway gain).
fn assert_playable(file: &str, samples: &[f32]) {
    assert!(
        !samples.is_empty(),
        "{file}: offline mixer produced no samples"
    );
    assert!(
        samples.iter().all(|sample| sample.is_finite()),
        "{file}: every mixed sample must be finite"
    );
    assert!(
        samples.iter().any(|sample| sample.abs() > 1e-4),
        "{file}: mixed samples must not be digital silence"
    );
    let peak = peak(samples);
    assert!(
        peak > 1e-3,
        "{file}: peak {peak} is too quiet for a decoded Crystal SFX"
    );
    // Overlapping cues deliberately sum without normalization, so 33 copies of a ~0.73-peak
    // clip legitimately reach ~24; anything far beyond that would be a runaway gain.
    assert!(
        peak < 64.0,
        "{file}: peak {peak} suggests a runaway gain (linear overlap stays far below this)"
    );
}

/// Assert `mixed` is `count` overlapping, phase-aligned copies of `single`.
///
/// Locks the "one cue per event" and "linear overlap" requirements: dropping a cue or
/// normalizing/clipping the mixer changes the per-sample ratio, the energy ratio or the
/// clip length.
fn assert_stacked(mixed: &[f32], single: &[f32], count: f32, label: &str) {
    assert_eq!(
        mixed.len(),
        single.len(),
        "{label}: mixing must not change the clip length"
    );
    assert!(
        !single.is_empty(),
        "{label}: baseline clip must not be empty"
    );
    for (index, (mixed, single)) in mixed.iter().zip(single).enumerate() {
        let expected = f64::from(*single) * f64::from(count);
        let tolerance = expected.abs() * 1e-4 + 1e-6;
        assert!(
            (f64::from(*mixed) - expected).abs() <= tolerance,
            "{label}: sample {index} is {mixed}, expected {count} x {single} = {expected}"
        );
    }
    let energy_ratio = energy(mixed) / energy(single);
    let expected_energy = f64::from(count) * f64::from(count);
    assert!(
        (energy_ratio - expected_energy).abs() <= expected_energy * 1e-3,
        "{label}: summed-square energy ratio {energy_ratio} must be ~{expected_energy} \
         (the mixer sums without normalization)"
    );
}

/// Assert `scaled` is `base` after the shared `amplify(gain)` path (per-sample check).
fn assert_scaled(scaled: &[f32], base: &[f32], factor: f32, label: &str) {
    assert_eq!(
        scaled.len(),
        base.len(),
        "{label}: the same clip must decode to the same length"
    );
    assert!(!base.is_empty(), "{label}: baseline clip must not be empty");
    for (index, (scaled, base)) in scaled.iter().zip(base).enumerate() {
        let expected = f64::from(*base) * f64::from(factor);
        let tolerance = expected.abs() * 1e-4 + 1e-6;
        assert!(
            (f64::from(*scaled) - expected).abs() <= tolerance,
            "{label}: sample {index} is {scaled}, expected {factor} x {base} = {expected}"
        );
    }
}

/// M3: every 125 cue is submitted at the same instant, even with the same file, and the
/// real mixer receives 2 and 33 overlapping sources (no fake counter).
#[test]
fn projectile_cue_submits_every_copy_at_the_same_instant() {
    let Some(library) = library_with_m34_clips() else {
        return;
    };

    // Two identical cues at the same instant: both must reach the mixer.
    let (mut player, mut source) = offline_player(library.clone(), true, 1.0);
    assert!(
        player.play_projectile_cue("M34-1.wav"),
        "first 125 cue must be submitted"
    );
    assert!(
        player.play_projectile_cue("M34-1.wav"),
        "second 125 cue at the same instant and file must also be submitted"
    );
    assert!(
        player.throttle().is_empty(),
        "the 125 path must not write the shared 100 ms throttle"
    );
    let pair = pull_clip(&mut source);
    assert_playable("M34-1.wav x2", &pair);

    // 33 identical cues at once: all must be submitted.
    let (mut crowded_player, mut crowded_source) = offline_player(library.clone(), true, 1.0);
    assert_eq!(
        crowded_player.play_projectile_cues(vec!["M34-2.wav"; 33]),
        33,
        "all 33 same-instant 125 cues must be submitted"
    );
    assert!(
        crowded_player.throttle().is_empty(),
        "33 cues must not write the shared throttle"
    );
    let crowded = pull_clip(&mut crowded_source);
    assert_playable("M34-2.wav x33", &crowded);

    // Baselines from fresh mixers (same format, same start point) prove the stacking is real.
    let (mut single_player, mut single_source) = offline_player(library.clone(), true, 1.0);
    assert!(single_player.play_projectile_cue("M34-1.wav"));
    let single_pair = pull_clip(&mut single_source);
    assert_stacked(&pair, &single_pair, 2.0, "two same-instant M34-1 cues");

    let (mut single_player, mut single_source) = offline_player(library, true, 1.0);
    assert!(single_player.play_projectile_cue("M34-2.wav"));
    let single_crowded = pull_clip(&mut single_source);
    assert_stacked(
        &crowded,
        &single_crowded,
        33.0,
        "33 same-instant M34-2 cues",
    );

    eprintln!(
        "EVIDENCE m3 same_instant submitted=2,33 single_cues=2/33 mixer_samples={}/{} peak={:.6}/{:.6}",
        pair.len(),
        crowded.len(),
        peak(&pair),
        peak(&crowded)
    );
}

/// M3: the generic `play` entry keeps the 100 ms same-file cooldown at its 0/99/100 ms
/// boundaries while 125 traffic neither feeds nor drains it.
#[test]
fn normal_entry_keeps_its_one_hundred_millisecond_cooldown() {
    let Some(library) = library_with_m34_clips() else {
        return;
    };
    assert_eq!(
        SOUND_COOLDOWN_MS, 100,
        "Crystal 125 bypass is about a 100 ms cooldown"
    );

    let (mut player, mut source) = offline_player(library.clone(), true, 1.0);
    let start = 1_000u64;
    assert!(
        player.play("M34-1.wav", start),
        "first normal play is submitted"
    );
    assert!(
        !player.play("M34-1.wav", start),
        "0 ms later: the same file is still cooling down"
    );
    assert!(
        !player.play("M34-1.wav", start + SOUND_COOLDOWN_MS - 1),
        "99 ms later: still cooling down"
    );
    assert!(
        player.play("M34-1.wav", start + SOUND_COOLDOWN_MS),
        "exactly 100 ms later: allowed again"
    );
    assert_eq!(player.throttle().len(), 1, "one file is cooling down");
    let mixed = pull_clip(&mut source);
    assert_playable("M34-1.wav (100 ms cooldown)", &mixed);

    // Interleaved 125 traffic: not swallowed by the normal cooldown and does not refresh it.
    let (mut player, _source) = offline_player(library, true, 1.0);
    assert!(
        player.play("M34-1.wav", 2_000),
        "normal entry records the cooldown"
    );
    assert!(
        player.play_projectile_cue("M34-1.wav"),
        "125 cue must not be swallowed by the normal cooldown"
    );
    assert!(
        !player.play("M34-1.wav", 2_050),
        "the 125 cue must not refresh the normal cooldown"
    );
    assert!(
        player.play("M34-1.wav", 2_000 + SOUND_COOLDOWN_MS),
        "normal entry is still allowed exactly 100 ms after its own play"
    );
    assert!(
        player.play_projectile_cue("M34-2.wav"),
        "125 cue for another file is always submitted"
    );
    assert!(
        player.play("M34-2.wav", 4_000),
        "125 traffic must not pre-throttle the normal entry for that file"
    );
    assert_eq!(
        player.throttle().len(),
        2,
        "only the two normal plays recorded"
    );
}

/// M3/M4: skipped paths return `false` and submit nothing into the real mixer.
#[test]
fn skipped_projectile_cues_return_false_and_submit_nothing() {
    // Disabled: deterministic no-op even with a working offline mixer.
    let (mut disabled, mut source) =
        offline_player(SoundLibrary::with_roots(Vec::new()), false, 1.0);
    assert!(!disabled.play_projectile_cue("M34-1.wav"));
    assert!(pull_finite(&mut source, FINITE_PULL_SAMPLES).is_empty());

    // Sound volume 0: gain 0, skipped before decoding.
    let (mut silent, mut source) = offline_player(SoundLibrary::with_roots(Vec::new()), true, 0.0);
    assert!(!silent.play_projectile_cue("M34-1.wav"));
    assert!(pull_finite(&mut source, FINITE_PULL_SAMPLES).is_empty());

    // Master volume 0 with non-zero sound volume: still silent.
    let (mut master_muted, mut source) =
        offline_player(SoundLibrary::with_roots(Vec::new()), true, 1.0);
    master_muted.set_master_volume(0.0);
    assert!(!master_muted.play_projectile_cue("M34-1.wav"));
    assert!(pull_finite(&mut source, FINITE_PULL_SAMPLES).is_empty());

    // Runtime toggle off after construction.
    let (mut toggled, mut source) = offline_player(SoundLibrary::with_roots(Vec::new()), true, 1.0);
    toggled.set_enabled(false);
    assert!(!toggled.play_projectile_cue("M34-1.wav"));
    assert!(pull_finite(&mut source, FINITE_PULL_SAMPLES).is_empty());

    // Missing asset and undecodable asset: quiet skip, not an error, nothing submitted.
    let (mut missing, mut source) = offline_player(SoundLibrary::with_roots(Vec::new()), true, 1.0);
    assert!(!missing.play_projectile_cue("M34-404.wav"));
    assert!(
        missing.throttle().is_empty(),
        "a failed cue never touches the throttle"
    );
    assert!(pull_finite(&mut source, FINITE_PULL_SAMPLES).is_empty());

    let dir = std::env::temp_dir().join(format!(
        "mir2-r61-d2-bad-decode-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join("BAD-1.mna"), b"not a real audio stream").expect("write bad mna");
    let (mut broken, mut source) =
        offline_player(SoundLibrary::with_roots(vec![dir.clone()]), true, 1.0);
    assert!(
        !broken.play_projectile_cue("BAD-1.wav"),
        "undecodable cue must be skipped"
    );
    assert!(pull_finite(&mut source, FINITE_PULL_SAMPLES).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

/// M4: both owned M34 clips resolve to `.mna`, decode, and play through the real offline
/// mixer with mixer metadata, finite/non-zero samples, sane peak and energy.
#[test]
fn both_m34_clips_play_through_the_offline_mixer() {
    let Some(library) = library_with_m34_clips() else {
        return;
    };

    for file in ["M34-1.wav", "M34-2.wav"] {
        let path = library.resolve(file).expect("checked above");
        assert_eq!(
            path.extension().and_then(std::ffi::OsStr::to_str),
            Some("mna"),
            "{file} must resolve to the owned .mna format"
        );
        // Shared decoder path (same as `decoder_for`): owned clip -> rodio vorbis decoder.
        let clip = mir_asset::AudioClip::open(&path)
            .unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
        assert!(
            !clip.data.is_empty(),
            "{} must carry a compressed stream",
            path.display()
        );
        let direct = rodio::Decoder::new(std::io::Cursor::new(clip.data))
            .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()));
        let clip_channels = direct.channels().get();
        let clip_rate = direct.sample_rate().get();
        let decoded: Vec<f32> = direct.collect();
        assert!(!decoded.is_empty(), "{file} must decode to samples");
        assert!(
            decoded.iter().all(|sample| sample.is_finite()),
            "{file} decoded samples must be finite"
        );
        assert!(
            decoded.iter().any(|sample| sample.abs() > 1e-4),
            "{file} decoded samples must not be digital silence"
        );

        let (mut player, mut source) = offline_player(library.clone(), true, 1.0);
        assert!(
            player.play_projectile_cue(file),
            "{file} cue must be submitted"
        );
        assert_eq!(
            source.channels().get(),
            MIXER_CHANNELS,
            "offline mixer channels"
        );
        assert_eq!(
            source.sample_rate().get(),
            MIXER_SAMPLE_RATE,
            "offline mixer rate"
        );
        let mixed = pull_clip(&mut source);
        assert_playable(file, &mixed);
        assert!(
            energy(&mixed) > 0.0,
            "{file}: mixed sample energy must be positive"
        );
        eprintln!(
            "EVIDENCE m4 clip={file} resolved={} clip_channels={clip_channels} \
             clip_rate={clip_rate} decoded_samples={} mixer_channels={MIXER_CHANNELS} \
             mixer_rate={MIXER_SAMPLE_RATE} mixed_samples={} peak={:.6} energy={:.6} rms={:.6}",
            path.display(),
            decoded.len(),
            mixed.len(),
            peak(&mixed),
            energy(&mixed),
            rms(&mixed)
        );
    }
}

/// M4/G4: the shared `amplify` path scales every sample by 0.25 and overlapping cues sum
/// linearly; removing the amplify step or dropping one submitted cue fails this test.
#[test]
fn gain_and_overlap_are_linear_in_the_offline_mixer() {
    let Some(library) = library_with_m34_clips() else {
        return;
    };

    // gain = 1.0 baseline.
    let (mut full, mut source) = offline_player(library.clone(), true, 1.0);
    assert!(full.play_projectile_cue("M34-1.wav"));
    let full_samples = pull_clip(&mut source);
    assert_playable("M34-1.wav gain=1.0", &full_samples);

    // gain = 0.25: quarter amplitude, 1/16 summed-square energy (RMS scales as gain).
    let (mut quarter, mut source) = offline_player(library.clone(), true, QUARTER_GAIN);
    assert!(quarter.play_projectile_cue("M34-1.wav"));
    let quarter_samples = pull_clip(&mut source);
    assert_scaled(
        &quarter_samples,
        &full_samples,
        QUARTER_GAIN,
        "gain=0.25 cue",
    );
    let rms_ratio = rms(&quarter_samples) / rms(&full_samples);
    assert!(
        (rms_ratio - f64::from(QUARTER_GAIN)).abs() <= 1e-3,
        "RMS ratio {rms_ratio} must be ~0.25"
    );
    let energy_ratio = energy(&quarter_samples) / energy(&full_samples);
    assert!(
        (energy_ratio - f64::from(QUARTER_GAIN).powi(2)).abs() <= 1e-3,
        "summed-square energy ratio {energy_ratio} must be ~0.0625 (gain^2)"
    );

    // Three overlapping cues: the mixer deliberately sums without normalization.
    let (mut overlay, mut source) = offline_player(library, true, 1.0);
    assert_eq!(overlay.play_projectile_cues(vec!["M34-1.wav"; 3]), 3);
    let overlay_samples = pull_clip(&mut source);
    assert_stacked(
        &overlay_samples,
        &full_samples,
        3.0,
        "three overlapping cues",
    );
    eprintln!(
        "EVIDENCE m4 gain_ratio samples={} quarter_rms_ratio={:.6} quarter_energy_ratio={:.6} \
         overlap_rms_ratio={:.6} overlap_energy_ratio={:.6}",
        full_samples.len(),
        rms_ratio,
        energy_ratio,
        rms(&overlay_samples) / rms(&full_samples),
        energy(&overlay_samples) / energy(&full_samples)
    );
}

/// M4: a bounded pull from a real cue returns finite samples and does not panic.
#[test]
fn finite_pull_from_a_real_cue_does_not_panic() {
    let Some(library) = library_with_m34_clips() else {
        return;
    };
    let (mut player, mut source) = offline_player(library, true, 1.0);
    assert!(player.play_projectile_cue("M34-1.wav"));
    let head = pull_finite(&mut source, FINITE_PULL_SAMPLES);
    assert!(!head.is_empty(), "a finite pull must return samples");
    assert!(head.len() <= FINITE_PULL_SAMPLES);
    assert!(head.iter().all(|sample| sample.is_finite()));
    assert_eq!(source.channels().get(), MIXER_CHANNELS);
    assert_eq!(source.sample_rate().get(), MIXER_SAMPLE_RATE);
    let tail = pull_clip(&mut source);
    eprintln!(
        "EVIDENCE m4 finite_pull requested={FINITE_PULL_SAMPLES} first={} rest={} total={}",
        head.len(),
        tail.len(),
        head.len() + tail.len()
    );
}

/// Policy: Crystal 125 `FireBounce` spawn/impact maps to `M34-1.wav` / `M34-2.wav`.
#[test]
fn crystal_125_policy_maps_fire_bounce_to_the_two_m34_files() {
    assert_eq!(ProjectileSoundPhase::Spawn.file(), "M34-1.wav");
    assert_eq!(ProjectileSoundPhase::Hit.file(), "M34-2.wav");
    assert_eq!(ProjectileSoundPhase::Spawn.index(), 20_341);
    assert_eq!(ProjectileSoundPhase::Hit.index(), 20_342);
    assert_eq!(
        projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Spawn),
        Some("M34-1.wav")
    );
    assert_eq!(
        projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Hit),
        Some("M34-2.wav")
    );
    assert_eq!(
        projectile_sound(Spell::FireBall, ProjectileSoundPhase::Spawn),
        None,
        "only FireBounce has a projectile cue in Crystal"
    );
}

/// Public throttle policy stays at exactly 100 ms per file (0/99/100 boundaries).
#[test]
fn sound_throttle_boundary_is_one_hundred_milliseconds() {
    let mut throttle = SoundThrottle::new();
    assert!(throttle.allow("M34-1.wav", 0), "first play is allowed");
    assert!(!throttle.allow("M34-1.wav", 0), "0 ms: same instant");
    assert!(!throttle.allow("M34-1.wav", 99), "99 ms: still blocked");
    assert!(throttle.allow("M34-1.wav", 100), "100 ms boundary: allowed");
    assert!(
        throttle.allow("M34-2.wav", 100),
        "per-file: other file unaffected"
    );
    assert_eq!(throttle.len(), 2);
    assert_eq!(SOUND_COOLDOWN_MS, 100);
}

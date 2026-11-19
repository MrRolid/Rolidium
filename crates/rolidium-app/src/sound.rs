// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The soft tone played when an operation fails. The tone is synthesised here and handed to a
//! command-line player (`paplay`, `pw-play` or `aplay`) that is started on a separate thread, so
//! nothing blocks the UI and nothing has to be installed to build Rolidium. If no player or no
//! sound device exists the call does nothing.

use std::io::Write;
use std::process::{Command, Stdio};

const RATE: u32 = 22_050;

/// A short, quiet two-note falling tone as a 16-bit mono WAV file. `volume` is 0..=1.
pub fn error_tone_wav(volume: f64) -> Vec<u8> {
    let volume = volume.clamp(0.0, 1.0);
    // Notes (Hz, seconds); each has a slow attack and a long smooth decay so it sounds soft.
    let notes = [(392.0_f64, 0.16_f64), (293.66, 0.26)];
    let mut samples: Vec<i16> = Vec::new();
    for (freq, len) in notes {
        let n = (len * RATE as f64) as usize;
        for i in 0..n {
            let t = i as f64 / RATE as f64;
            let attack = (t / 0.025).min(1.0);
            let decay = (1.0 - t / len).powi(2);
            let env = attack * decay;
            // fundamental plus a very quiet octave for warmth
            let s = (std::f64::consts::TAU * freq * t).sin() * 0.85
                + (std::f64::consts::TAU * freq * 2.0 * t).sin() * 0.15;
            samples.push((s * env * volume * 0.3 * i16::MAX as f64) as i16);
        }
    }
    let data_len = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// Plays the error tone without blocking. Silently does nothing if there is no player.
pub fn play_error(volume: f64) {
    if volume <= 0.0 || std::env::var_os("ROLIDIUM_NO_SOUND").is_some() {
        return;
    }
    // Windows: silent for now (a native player is planned)
    if cfg!(windows) {
        return;
    }
    let wav = error_tone_wav(volume);
    let _ = std::thread::Builder::new()
        .name("error-tone".into())
        .spawn(move || {
            for (player, args) in [
                ("paplay", &[][..]),
                ("pw-play", &["-"][..]),
                ("aplay", &["-q"][..]),
            ] {
                let Ok(mut child) = Command::new(player)
                    .args(args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                else {
                    continue;
                };
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(&wav);
                }
                if child.wait().is_ok_and(|s| s.success()) {
                    return;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_is_a_valid_quiet_wav() {
        let w = error_tone_wav(1.0);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        let data_len = u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize;
        assert_eq!(w.len(), 44 + data_len);
        let samples: Vec<i16> = w[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 1000, "audible");
        assert!(
            peak < (i16::MAX as f64 * 0.35) as u16,
            "soft, far from clipping: {peak}"
        );
        let secs = samples.len() as f64 / RATE as f64;
        assert!((0.2..0.7).contains(&secs), "short: {secs}");
        // Starts and ends at silence, so there is no click.
        assert!(samples[0].abs() < 50 && samples[samples.len() - 1].abs() < 50);
        // Volume scales the level; zero volume is silent.
        let quiet = error_tone_wav(0.2);
        let level = |w: &[u8]| {
            w[44..]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs() as u64)
                .max()
                .unwrap()
        };
        assert!(level(&quiet) < level(&w));
        assert!(error_tone_wav(0.0)[44..].iter().all(|b| *b == 0));
    }

    #[test]
    fn playing_does_not_block() {
        std::env::remove_var("ROLIDIUM_NO_SOUND");
        let t = std::time::Instant::now();
        play_error(0.5);
        assert!(t.elapsed().as_millis() < 100, "returns at once");
    }
}

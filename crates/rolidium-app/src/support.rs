// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The "Support development" part of the About window: one Bitcoin address per installation, with a QR code.
//! No network access, no pop-ups, no reminders.

use crate::settings::Settings;

/// The addresses, exactly as given by the author. Do not edit.
pub const ADDRESSES: [&str; 3] = [
    "bc1qx4reeuandqagn5wrtw0vm9y8kxz4h6h6c8ecrk",
    "bc1qugeextjuaaan2erm5r5rfwdck24dmx59rjyuay",
    "bc1qu5qj4q5r5z7ru7wym8ng8xw0huqtwj7rszv873",
];

/// A fresh random choice (the clock is random enough for picking one of three).
pub fn random_index() -> usize {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as u64 ^ d.as_secs().rotate_left(17));
    // mix the bits a little so consecutive starts do not alternate predictably
    let x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 33;
    (x % ADDRESSES.len() as u64) as usize
}

/// The index stored in the settings, or a new random one if it is missing or invalid. Returns the index and
/// whether it is new (and has to be saved).
pub fn chosen_index(s: &Settings) -> (usize, bool) {
    match s.donate_index {
        Some(i) if i < ADDRESSES.len() => (i, false),
        _ => (random_index(), true),
    }
}

/// QR modules (dark = true) of the address, as `(width, row-major modules)`.
pub fn qr_modules(address: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::new(address.as_bytes()).ok()?;
    let w = code.width();
    let m = code
        .to_colors()
        .into_iter()
        .map(|c| c == qrcode::Color::Dark)
        .collect();
    Some((w, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

    fn polymod(values: &[u32]) -> u32 {
        const G: [u32; 5] = [0x3b6a_57b2, 0x2650_8e6d, 0x1ea1_19fa, 0x3d42_33dd, 0x2a14_62b3];
        let mut chk = 1u32;
        for v in values {
            let b = chk >> 25;
            chk = ((chk & 0x01ff_ffff) << 5) ^ v;
            for (i, g) in G.iter().enumerate() {
                if (b >> i) & 1 == 1 {
                    chk ^= g;
                }
            }
        }
        chk
    }

    /// bech32 (BIP-173) checksum of a native SegWit v0 address.
    fn bech32_ok(addr: &str) -> bool {
        let (hrp, data) = addr.rsplit_once('1').unwrap();
        let mut v: Vec<u32> = hrp.bytes().map(|b| (b >> 5) as u32).collect();
        v.push(0);
        v.extend(hrp.bytes().map(|b| (b & 31) as u32));
        for c in data.chars() {
            match CHARSET.find(c) {
                Some(i) => v.push(i as u32),
                None => return false,
            }
        }
        polymod(&v) == 1
    }

    #[test]
    fn every_address_has_a_valid_bech32_checksum() {
        for a in ADDRESSES {
            assert!(a.starts_with("bc1q") && a.len() == 42, "{a}");
            assert!(bech32_ok(a), "bad checksum: {a}");
        }
        // the checker itself rejects a damaged address
        let mut bad = ADDRESSES[0].to_string();
        bad.replace_range(10..11, if &bad[10..11] == "q" { "p" } else { "q" });
        assert!(!bech32_ok(&bad));
    }

    #[test]
    fn the_shown_address_is_always_one_of_the_three_and_stays_the_same() {
        for i in 0..200 {
            let (idx, _) = chosen_index(&Settings {
                donate_index: Some(i % 7),
                ..Settings::default()
            });
            assert!(idx < 3);
            assert!(ADDRESSES.contains(&ADDRESSES[idx]));
        }
        // missing or invalid: a new choice; valid: kept
        assert!(chosen_index(&Settings::default()).1);
        assert!(
            chosen_index(&Settings {
                donate_index: Some(9),
                ..Settings::default()
            })
            .1
        );
        assert_eq!(
            chosen_index(&Settings {
                donate_index: Some(2),
                ..Settings::default()
            }),
            (2, false)
        );
        // all three get chosen over many fresh starts
        let mut seen = [false; 3];
        for _ in 0..2000 {
            seen[random_index()] = true;
            std::thread::sleep(std::time::Duration::from_micros(3));
        }
        assert!(seen.iter().all(|s| *s), "{seen:?}");
    }

    #[test]
    fn the_choice_survives_a_restart() {
        let _g = crate::settings::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tmp")
            .join(format!("donate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let mut s = Settings::load();
        assert_eq!(s.donate_index, None);
        let (i, new) = chosen_index(&s);
        assert!(new);
        s.donate_index = Some(i);
        s.save();
        let again = Settings::load();
        assert_eq!(chosen_index(&again), (i, false));
        // a damaged number means a new choice
        let text = std::fs::read_to_string(dir.join("rolidium/settings.conf"))
            .unwrap()
            .replace(&format!("donate_index={i}"), "donate_index=banana");
        std::fs::write(dir.join("rolidium/settings.conf"), text).unwrap();
        assert!(chosen_index(&Settings::load()).1);
    }

    #[test]
    fn the_qr_code_decodes_to_the_same_address() {
        for a in ADDRESSES {
            let (w, m) = qr_modules(a).unwrap();
            // render like the window: 6 px per module with a quiet zone of 4 modules
            let scale = 6usize;
            let size = (w + 8) * scale;
            let px = |x: usize, y: usize| -> u8 {
                let (mx, my) = (x / scale, y / scale);
                if mx < 4 || my < 4 || mx >= w + 4 || my >= w + 4 {
                    255
                } else if m[(my - 4) * w + (mx - 4)] {
                    0
                } else {
                    255
                }
            };
            let mut img = rqrr::PreparedImage::prepare_from_greyscale(size, size, px);
            let grids = img.detect_grids();
            assert_eq!(grids.len(), 1, "{a}");
            let (_, content) = grids[0].decode().unwrap();
            assert_eq!(content, a);
        }
    }
}

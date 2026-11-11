//! AKP153E wire protocol. Pure functions, no I/O. Spec: docs/research/hid-protocol.md.
//! Every packet returned here is the payload (no report ID); `wire()` prepends 0x00.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceSpec {
    pub name: &'static str,
    pub manifest_id: &'static str, /* the DeviceUUID in profiles */
    pub vid: u16,
    pub pid: u16,
    pub lcd_keys: u8,
    pub side_keys: u8,
    pub key_px: u32,
    pub side_px: u32,
    pub max_jpeg: usize,
    pub verified: bool,
}

impl DeviceSpec {
    #[allow(dead_code)]
    pub const fn total_keys(&self) -> u8 {
        self.lcd_keys + self.side_keys
    }

    pub const fn image_px(&self, key: u8) -> u32 {
        if key > self.lcd_keys {
            self.side_px
        } else {
            self.key_px
        }
    }
}

pub const DEVICES: &[DeviceSpec] = &[
    // Primary hardware target: verified on real AKP153E physical device.
    // Source: docs/research/hid/hid-protocol.md §0, §3.1, §4.
    DeviceSpec {
        name: "AKP153E",
        manifest_id: "AKP153E",
        vid: 0x0300,
        pid: 0x1010,
        lcd_keys: 15,
        side_keys: 3,
        key_px: 85,
        side_px: 80,
        max_jpeg: 9000,
        verified: true,
    },
    // AKP153E rev. 2 (pv3 / firmware V3 "293SV3"): 15 LCD keys + 3 side cells, 95x95 keys / 82x82 side cells.
    // Source: docs/research/community/community-ideas.md §1.2, §1.4, §7; docs/research/hid/keystate-modes.md §3.
    DeviceSpec {
        name: "AKP153E-REV2",
        manifest_id: "APK153E_293SV3",
        vid: 0x0300,
        pid: 0x3010,
        lcd_keys: 15,
        side_keys: 3,
        key_px: 95,
        side_px: 82,
        max_jpeg: 9000,
        verified: false,
    },
    // AKP153 international / Mirabox 293S analyzed in Deckhand: 15 LCD keys + 3 side cells, 85x85 / 80x80.
    // Source: docs/research/community/community-ideas.md §1.2, §1.5.
    DeviceSpec {
        name: "AKP153",
        manifest_id: "AKP153",
        vid: 0x5548,
        pid: 0x6674,
        lcd_keys: 15,
        side_keys: 3,
        key_px: 85,
        side_px: 80,
        max_jpeg: 9000,
        verified: false,
    },
];

pub const DEFAULT_DEVICE: &DeviceSpec = &DEVICES[0];
#[allow(dead_code)]
pub const VID: u16 = DEFAULT_DEVICE.vid;
#[allow(dead_code)]
pub const PID: u16 = DEFAULT_DEVICE.pid;
pub const HEARTBEAT_SECS: u64 = 10;
pub const MAX_JPEG: usize = DEFAULT_DEVICE.max_jpeg;
pub const KEY_PX: u32 = DEFAULT_DEVICE.key_px;
pub const SIDE_PX: u32 = DEFAULT_DEVICE.side_px;

pub fn select_device() -> Result<&'static DeviceSpec, String> {
    if let Ok(name) = std::env::var("ORCADECK_DEVICE") {
        let name = name.trim();
        if let Some(spec) = DEVICES.iter().find(|d| d.name.eq_ignore_ascii_case(name)) {
            Ok(spec)
        } else {
            let known: Vec<&str> = DEVICES.iter().map(|d| d.name).collect();
            Err(format!(
                "unknown device '{name}' in ORCADECK_DEVICE; known: {}",
                known.join(", ")
            ))
        }
    } else {
        // default selection = the AKP153E (verified)
        DEVICES
            .iter()
            .find(|d| d.verified)
            .ok_or_else(|| "no verified device in spec table".into())
    }
}

pub fn find_spec(vid: u16, pid: u16) -> Option<&'static DeviceSpec> {
    DEVICES.iter().find(|d| d.vid == vid && d.pid == pid)
}

/// Hardware key index for profile coordinates (col 0..4 keys, col 5 side cells; row 0..2).
pub fn key_index(col: u8, row: u8) -> Option<u8> {
    if row > 2 {
        return None;
    }
    match col {
        0..=4 => Some(13 + row - 3 * col),
        5 => Some(16 + row),
        _ => None,
    }
}

pub fn image_px(key: u8) -> u32 {
    DEFAULT_DEVICE.image_px(key)
}

pub fn image_px_for(spec: &DeviceSpec, key: u8) -> u32 {
    spec.image_px(key)
}

/// `CRT\0\0` + verb, zero-padded to `n` bytes; `extra` = (offset, value) pairs.
pub fn crt(n: usize, verb: &str, extra: &[(usize, u8)]) -> Vec<u8> {
    let mut p = vec![0u8; n];
    p[..3].copy_from_slice(b"CRT");
    p[5..5 + verb.len()].copy_from_slice(verb.as_bytes());
    for &(o, v) in extra {
        p[o] = v;
    }
    p
}

pub fn wake(n: usize) -> Vec<u8> {
    crt(n, "DIS", &[])
}
pub fn brightness(n: usize, pct: u8) -> Vec<u8> {
    crt(n, "LIG", &[(0x0a, pct.min(100))])
}
pub fn clear_key(n: usize, key: u8) -> Vec<u8> {
    crt(n, "CLE", &[(0x0b, key)])
}
pub fn clear_all(n: usize) -> Vec<u8> {
    clear_key(n, 0xFF)
}
pub fn flush(n: usize) -> Vec<u8> {
    crt(n, "STP", &[])
}
pub fn heartbeat(n: usize) -> Vec<u8> {
    crt(n, "CONNECT", &[])
}

/// Reply to the device's `HANLDER` (no CRT prefix).
pub fn handshake_reply(n: usize) -> Vec<u8> {
    let mut p = vec![0u8; n];
    p[..3].copy_from_slice(b"HAN");
    p
}

/// BAT header + JPEG chunks (each exactly `n` bytes, last zero-padded).
pub fn image_packets(n: usize, key: u8, jpeg: &[u8]) -> Vec<Vec<u8>> {
    let len = jpeg.len();
    let mut out = vec![crt(
        n,
        "BAT",
        &[(0x0a, (len >> 8) as u8), (0x0b, len as u8), (0x0c, key)],
    )];
    for c in jpeg.chunks(n) {
        let mut p = vec![0u8; n];
        p[..c.len()].copy_from_slice(c);
        out.push(p);
    }
    out
}

pub fn wire(payload: &[u8]) -> Vec<u8> {
    let mut w = Vec::with_capacity(payload.len() + 1);
    w.push(0);
    w.extend_from_slice(payload);
    w
}

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Handshake,
    Ack,
    Error,
    Mode(u8),
    /// key 1..18, raw state byte (press/release meaning unconfirmed on AKP153E)
    Key {
        key: u8,
        state: u8,
    },
}

pub fn parse_input(b: &[u8]) -> Input {
    let at = |i: usize| b.get(i).copied().unwrap_or(0);
    if b.starts_with(b"HANLDER") {
        Input::Handshake
    } else if b.starts_with(b"error") {
        Input::Error
    } else if b.starts_with(b"ACK") && b.get(5..7) == Some(b"OK") && at(9) == 0 {
        Input::Ack
    } else if b.starts_with(b"CRT") && b.get(5..8) == Some(b"MOD") {
        Input::Mode(at(0x0a))
    } else {
        Input::Key {
            key: at(9),
            state: at(10),
        }
    }
}

/// Output report payload size N (without report ID) from a HID report descriptor:
/// sum of bits of all Output items (0x91) in the first report, / 8.
pub fn output_len_from_descriptor(d: &[u8]) -> Option<usize> {
    let (mut size, mut count, mut bits, mut i) = (0usize, 0usize, 0usize, 0);
    while i < d.len() {
        let h = d[i];
        if h == 0xFE {
            i += 3 + d.get(i + 1).copied().unwrap_or(0) as usize;
            continue;
        }
        let n = match h & 3 {
            3 => 4,
            k => k as usize,
        };
        let v = (0..n).fold(0usize, |a, k| {
            a | (*d.get(i + 1 + k).unwrap_or(&0) as usize) << (8 * k)
        });
        match h & 0xFC {
            0x74 => size = v,             // Report Size
            0x94 => count = v,            // Report Count
            0x90 => bits += size * count, // Output
            _ => {}
        }
        i += 1 + n;
    }
    (bits >= 8).then_some(bits / 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_map_matches_spec() {
        assert_eq!(key_index(0, 0), Some(13));
        assert_eq!(key_index(4, 0), Some(1));
        assert_eq!(key_index(0, 2), Some(15));
        assert_eq!(key_index(4, 2), Some(3));
        assert_eq!(key_index(2, 1), Some(8));
        assert_eq!(key_index(5, 0), Some(16));
        assert_eq!(key_index(5, 2), Some(18));
        assert_eq!(key_index(6, 0), None);
        // all 18 indices unique and in 1..=18
        let mut s: Vec<u8> = (0..6)
            .flat_map(|c| (0..3).filter_map(move |r| key_index(c, r)))
            .collect();
        s.sort();
        assert_eq!(s, (1..=18).collect::<Vec<u8>>());
    }

    #[test]
    fn commands_layout() {
        let p = brightness(64, 250);
        assert_eq!(&p[..10], b"CRT\0\0LIG\0\0");
        assert_eq!(p[0x0a], 100);
        assert_eq!(clear_all(64)[0x0b], 0xFF);
        assert_eq!(&heartbeat(64)[5..12], b"CONNECT");
        assert_eq!(wire(&flush(64)).len(), 65);
        assert_eq!(&handshake_reply(64)[..3], b"HAN");
    }

    #[test]
    fn image_chunks() {
        let jpeg = vec![7u8; 1300];
        let pk = image_packets(512, 13, &jpeg);
        assert_eq!(pk.len(), 1 + 3);
        assert_eq!((pk[0][0x0a], pk[0][0x0b], pk[0][0x0c]), (0x05, 0x14, 13));
        assert!(pk.iter().all(|p| p.len() == 512));
        assert_eq!(pk[3][276], 0); // zero padded tail
        assert_eq!(pk[3][275], 7);
    }

    #[test]
    fn input_parse() {
        let mut b = vec![0u8; 64];
        b[..7].copy_from_slice(b"HANLDER");
        assert_eq!(parse_input(&b), Input::Handshake);
        let mut a = vec![0u8; 64];
        a[..3].copy_from_slice(b"ACK");
        a[5..7].copy_from_slice(b"OK");
        assert_eq!(parse_input(&a), Input::Ack);
        a[9] = 5;
        a[10] = 1;
        assert_eq!(parse_input(&a), Input::Key { key: 5, state: 1 });
    }

    #[test]
    fn descriptor_output_len() {
        // vendor page, Output: size 8 x count 512 (0x75 0x08, 0x96 0x00 0x02, 0x91 0x02)
        let d = [
            0x06, 0xA0, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x96, 0x00, 0x02, 0x91, 0x02,
            0xC0,
        ];
        assert_eq!(output_len_from_descriptor(&d), Some(512));
    }

    #[test]
    fn akp153e_byte_identical_packets() {
        let n = 512;

        // 1. Handshake reply: starts with b"HAN" and zero-padded to 512 bytes
        let mut expected_han = vec![0u8; n];
        expected_han[0..3].copy_from_slice(b"HAN");
        assert_eq!(handshake_reply(n), expected_han);

        // 2. Heartbeat: starts with b"CRT\0\0CONNECT" and zero-padded to 512 bytes
        let mut expected_hb = vec![0u8; n];
        expected_hb[0..3].copy_from_slice(b"CRT");
        expected_hb[5..12].copy_from_slice(b"CONNECT");
        assert_eq!(heartbeat(n), expected_hb);

        // 3. BAT header for key 1 with 1000-byte JPEG (0x03E8)
        let mut expected_bat1 = vec![0u8; n];
        expected_bat1[0..3].copy_from_slice(b"CRT");
        expected_bat1[5..8].copy_from_slice(b"BAT");
        expected_bat1[0x0a] = 0x03;
        expected_bat1[0x0b] = 0xE8;
        expected_bat1[0x0c] = 1;
        let dummy_jpeg = vec![0x42u8; 1000];
        let packets_key1 = image_packets(n, 1, &dummy_jpeg);
        assert_eq!(packets_key1[0], expected_bat1);

        // 4. BAT header for key 15 with 1000-byte JPEG (0x03E8)
        let mut expected_bat15 = vec![0u8; n];
        expected_bat15[0..3].copy_from_slice(b"CRT");
        expected_bat15[5..8].copy_from_slice(b"BAT");
        expected_bat15[0x0a] = 0x03;
        expected_bat15[0x0b] = 0xE8;
        expected_bat15[0x0c] = 15;
        let packets_key15 = image_packets(n, 15, &dummy_jpeg);
        assert_eq!(packets_key15[0], expected_bat15);

        // 5. STP (flush): starts with b"CRT\0\0STP" and zero-padded to 512 bytes
        let mut expected_stp = vec![0u8; n];
        expected_stp[0..3].copy_from_slice(b"CRT");
        expected_stp[5..8].copy_from_slice(b"STP");
        assert_eq!(flush(n), expected_stp);

        // 6. image_px returns 85 for keys 1..=15 and 80 for keys 16..=18
        for k in 1..=15 {
            assert_eq!(image_px(k), 85, "key {k} should be 85px");
            assert_eq!(DEFAULT_DEVICE.image_px(k), 85);
            assert_eq!(image_px_for(DEFAULT_DEVICE, k), 85);
        }
        for k in 16..=18 {
            assert_eq!(image_px(k), 80, "key {k} should be 80px");
            assert_eq!(DEFAULT_DEVICE.image_px(k), 80);
            assert_eq!(image_px_for(DEFAULT_DEVICE, k), 80);
        }
        // 7. Verify device constants and geometry helpers
        assert_eq!(DEFAULT_DEVICE.total_keys(), 18);
        assert_eq!(VID, 0x0300);
        assert_eq!(PID, 0x1010);
        assert_eq!(KEY_PX, 85);
        assert_eq!(SIDE_PX, 80);
        assert_eq!(MAX_JPEG, 9000);
        assert_eq!(HEARTBEAT_SECS, 10);
    }

    #[test]
    fn select_device_env_and_defaults() {
        let def = select_device().expect("default device");
        assert_eq!(def.name, "AKP153E");
        assert!(def.verified);

        assert_eq!(find_spec(0x0300, 0x1010).map(|s| s.name), Some("AKP153E"));
        assert_eq!(
            find_spec(0x0300, 0x3010).map(|s| s.name),
            Some("AKP153E-REV2")
        );
        assert_eq!(find_spec(0x5548, 0x6674).map(|s| s.name), Some("AKP153"));
        assert_eq!(find_spec(0x1234, 0x5678), None);

        // Case-insensitive lookup and unverified specs
        let rev2 = DEVICES
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case("akp153e-rev2"))
            .unwrap();
        assert_eq!(rev2.pid, 0x3010);
        assert!(!rev2.verified);
        assert_eq!(rev2.image_px(1), 95);
        assert_eq!(rev2.image_px(16), 82);

        let intl = DEVICES
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case("akp153"))
            .unwrap();
        assert_eq!(intl.vid, 0x5548);
        assert_eq!(intl.pid, 0x6674);
        assert!(!intl.verified);
    }
}

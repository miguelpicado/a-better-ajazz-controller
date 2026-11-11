//! AKP153E wire protocol. Pure functions, no I/O. Spec: docs/research/hid-protocol.md.
//! Every packet returned here is the payload (no report ID); `wire()` prepends 0x00.

pub const VID: u16 = 0x0300;
pub const PID: u16 = 0x1010;
pub const HEARTBEAT_SECS: u64 = 10;
pub const MAX_JPEG: usize = 9000;
pub const KEY_PX: u32 = 85;
pub const SIDE_PX: u32 = 80;

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
    if key >= 16 {
        SIDE_PX
    } else {
        KEY_PX
    }
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
}

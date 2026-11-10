//! Read-only reader for one text column of a small SQLite table, enough to take Stream Dock's per-device
//! scene order from `config\DataCache.db` (`SELECT Profile FROM "<Model>"`) without linking SQLite.
//! Rows come back in rowid order. Records that spill to overflow pages are skipped (scene ids are short).
//! ponytail: no WAL support; Stream Dock uses a rollback journal (header bytes 18/19 = 1). Add if that changes.

fn varint(b: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for n in 0..9 {
        let c = *b.get(i)?;
        i += 1;
        if n == 8 {
            return Some(((v << 8) | c as u64, i));
        }
        v = (v << 7) | (c & 0x7f) as u64;
        if c < 0x80 {
            return Some((v, i));
        }
    }
    None
}

#[derive(Debug, PartialEq)]
enum Val {
    Text(String),
    Int(i64),
    Other,
}

fn record(p: &[u8]) -> Option<Vec<Val>> {
    let (hlen, mut i) = varint(p, 0)?;
    let mut types = vec![];
    while i < hlen as usize {
        let (t, j) = varint(p, i)?;
        types.push(t);
        i = j;
    }
    let mut off = hlen as usize;
    let mut out = vec![];
    for t in types {
        let size = match t {
            0 | 8 | 9 => 0,
            1..=4 => t as usize,
            5 => 6,
            6 | 7 => 8,
            t if t >= 12 => ((t - 12) / 2) as usize,
            _ => return None,
        };
        let raw = p.get(off..off + size)?;
        out.push(match t {
            1..=6 => Val::Int(raw.iter().fold(if raw[0] & 0x80 != 0 { -1i64 } else { 0 }, |a, &c| (a << 8) | c as i64)),
            8 => Val::Int(0),
            9 => Val::Int(1),
            t if t >= 13 && t % 2 == 1 => Val::Text(String::from_utf8_lossy(raw).into_owned()),
            _ => Val::Other,
        });
        off += size;
    }
    Some(out)
}

/// In-order walk of a table b-tree rooted at `page` (1-based).
fn walk(db: &[u8], psize: usize, page: usize, depth: u8, rows: &mut Vec<Vec<Val>>) -> Option<()> {
    if depth > 20 || page == 0 {
        return None;
    }
    let base = (page - 1) * psize;
    let pg = db.get(base..base + psize)?;
    let h = if page == 1 { 100 } else { 0 };
    let kind = pg[h];
    let ncells = u16::from_be_bytes([pg[h + 3], pg[h + 4]]) as usize;
    let hsize = if kind == 0x05 { 12 } else { 8 };
    let usable = psize - db[20] as usize;
    for c in 0..ncells {
        let ptr = h + hsize + c * 2;
        let off = u16::from_be_bytes([*pg.get(ptr)?, *pg.get(ptr + 1)?]) as usize;
        match kind {
            0x05 => {
                let child = u32::from_be_bytes(pg.get(off..off + 4)?.try_into().ok()?) as usize;
                walk(db, psize, child, depth + 1, rows)?;
            }
            0x0d => {
                let (len, i) = varint(pg, off)?;
                let (_rowid, i) = varint(pg, i)?;
                if len as usize <= usable - 35 {
                    if let Some(r) = pg.get(i..i + len as usize).and_then(record) {
                        rows.push(r);
                    }
                }
            }
            _ => return None,
        }
    }
    if kind == 0x05 {
        let right = u32::from_be_bytes(pg[h + 8..h + 12].try_into().ok()?) as usize;
        walk(db, psize, right, depth + 1, rows)?;
    }
    Some(())
}

/// Text values of column `col` of table `table`, in rowid order. `None` if the file or table is unreadable.
pub fn text_column(db: &[u8], table: &str, col: usize) -> Option<Vec<String>> {
    if !db.starts_with(b"SQLite format 3\0") || db.len() < 100 {
        return None;
    }
    let psize = match u16::from_be_bytes([db[16], db[17]]) {
        1 => 65536,
        n => n as usize,
    };
    let mut master = vec![];
    walk(db, psize, 1, 0, &mut master)?;
    let root = master.iter().find_map(|r| match (r.first(), r.get(1), r.get(3)) {
        (Some(Val::Text(t)), Some(Val::Text(n)), Some(Val::Int(p))) if t == "table" && n == table => Some(*p as usize),
        _ => None,
    })?;
    let mut rows = vec![];
    walk(db, psize, root, 0, &mut rows)?;
    Some(rows.into_iter().filter_map(|r| match r.into_iter().nth(col) {
        Some(Val::Text(s)) => Some(s),
        _ => None,
    }).collect())
}

#[cfg(test)]
mod tests {
    /// Synthetic fixture from tests/fixtures/make_datacache.py: tables AJAZZ, AKP153E (3 scene rows),
    /// UnknownDevice (empty) and a 600-row table that forces interior b-tree pages.
    #[test]
    fn reads_scene_order() {
        let db = include_bytes!("../tests/fixtures/DataCache.db");
        assert_eq!(
            super::text_column(db, "AKP153E", 0).unwrap(),
            ["3I0U64IN-IT4R-RUB6-EZ4Z-VXS311JFXI05.sdProfile", "R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile",
             "H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q.sdProfile"]
        );
        assert_eq!(super::text_column(db, "UnknownDevice", 0).unwrap().len(), 0);
        let big = super::text_column(db, "Big", 0).unwrap();
        assert_eq!((big.len(), big[0].as_str(), big[599].as_str()), (600, "row0", "row599"));
        assert!(super::text_column(db, "Nope", 0).is_none());
        assert!(super::text_column(b"not a db", "AKP153E", 0).is_none());
    }
}

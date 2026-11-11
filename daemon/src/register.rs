//! Per-user file association for plugin packages (HKCU, no admin).
//!
//! `register` writes a `OrcaDeck.Plugin` ProgID whose open command is `"<exe>" install "%1"` and
//! points `.streamDeckPlugin`/`.sdPlugin` at it. `unregister` removes those entries and restores the
//! default each extension had before the first register; that value is kept under the ProgID key as
//! `PreviousDefault.<ext>` so a later register never overwrites it with our own ProgID.
//!
//! The plan is built by pure functions (unit-tested); `apply` only runs `reg.exe` on it.
use std::process::Command;

pub const PROGID: &str = "OrcaDeck.Plugin";
pub const EXTENSIONS: [&str; 2] = ["streamDeckPlugin", "sdPlugin"];
const CLASSES: &str = r"Software\Classes";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Create the key if needed and set its default value.
    SetDefault(String, String),
    /// Create the key if needed and set a named value (empty data allowed).
    SetValue(String, String, String),
    DeleteDefault(String),
    DeleteValue(String, String),
    DeleteKey(String),
}

pub fn command(exe: &str) -> String {
    format!("\"{exe}\" install \"%1\"")
}

/// What `register`/`unregister` need to know about one extension before writing anything.
pub struct ExtState {
    pub ext: &'static str,
    /// Current default value of `HKCU\Software\Classes\.<ext>`, if any.
    pub current: Option<String>,
    /// `PreviousDefault.<ext>` already stored by an earlier register.
    pub stored: Option<String>,
}

fn previous_name(ext: &str) -> String {
    format!("PreviousDefault.{ext}")
}

pub fn register_ops(exe: &str, state: &[ExtState]) -> Vec<Op> {
    let mut ops = vec![
        Op::SetDefault(PROGID.into(), "Plugin de OrcaDeck".into()),
        Op::SetDefault(format!("{PROGID}\\DefaultIcon"), format!("\"{exe}\",0")),
        Op::SetDefault(format!("{PROGID}\\shell\\open\\command"), command(exe)),
    ];
    for s in state {
        let key = format!(".{}", s.ext);
        if s.stored.is_none() {
            // Only the first register remembers the previous default; re-registering must not
            // replace it with our own ProgID.
            if let Some(prev) = s.current.as_deref().filter(|p| *p != PROGID) {
                ops.push(Op::SetValue(PROGID.into(), previous_name(s.ext), prev.to_string()));
            }
        }
        ops.push(Op::SetDefault(key.clone(), PROGID.into()));
        ops.push(Op::SetValue(format!("{key}\\OpenWithProgids"), PROGID.into(), String::new()));
    }
    ops
}

pub fn unregister_ops(state: &[ExtState]) -> Vec<Op> {
    let mut ops = vec![];
    for s in state {
        let key = format!(".{}", s.ext);
        match &s.stored {
            Some(prev) => {
                ops.push(Op::SetDefault(key, prev.clone()));
                ops.push(Op::DeleteValue(PROGID.into(), previous_name(s.ext)));
            }
            // Never had a default: remove ours without deleting anything the key may hold.
            None => ops.push(Op::DeleteDefault(key)),
        }
        ops.push(Op::DeleteValue(format!(".{}\\OpenWithProgids", s.ext), PROGID.into()));
    }
    ops.push(Op::DeleteKey(PROGID.into()));
    ops
}

pub fn register() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("no se pudo resolver la ruta de orcadeck.exe: {e}"))?
        .to_string_lossy()
        .into_owned();
    let state: Vec<ExtState> = EXTENSIONS
        .iter()
        .map(|ext| ExtState { ext, current: win::get(&format!(".{ext}"), None), stored: win::get(PROGID, Some(&previous_name(ext))) })
        .collect();
    apply(&register_ops(&exe, &state))?;
    println!("OrcaDeck: .streamDeckPlugin y .sdPlugin abren el editor para instalar el plugin.");
    println!("  comando: {}", command(&exe));
    println!("  deshacer: orcadeck unregister");
    Ok(())
}

pub fn unregister() -> Result<(), String> {
    let state: Vec<ExtState> = EXTENSIONS
        .iter()
        .map(|ext| ExtState { ext, current: None, stored: win::get(PROGID, Some(&previous_name(ext))) })
        .collect();
    apply(&unregister_ops(&state))?;
    for s in &state {
        match &s.stored {
            Some(prev) => println!("  .{} vuelve a «{prev}»", s.ext),
            None => println!("  .{} queda sin la asociación de OrcaDeck", s.ext),
        }
    }
    Ok(())
}

fn apply(ops: &[Op]) -> Result<(), String> {
    for op in ops {
        let key = |k: &str| format!(r"HKCU\{CLASSES}\{k}");
        match op {
            Op::SetDefault(k, data) => reg(&["add", &key(k), "/ve", "/d", data, "/f"])?,
            Op::SetValue(k, name, data) => reg(&["add", &key(k), "/v", name, "/d", data, "/f"])?,
            Op::DeleteDefault(k) => {
                let _ = reg(&["delete", &key(k), "/ve", "/f"]);
            }
            Op::DeleteValue(k, name) => {
                let _ = reg(&["delete", &key(k), "/v", name, "/f"]);
            }
            Op::DeleteKey(k) => {
                let _ = reg(&["delete", &key(k), "/f"]);
            }
        }
    }
    Ok(())
}

fn reg(args: &[&str]) -> Result<(), String> {
    let out = Command::new("reg.exe").args(args).output().map_err(|e| format!("no se pudo ejecutar reg.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("reg.exe falló ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    const HKCU: isize = 0x8000_0001u32 as isize;
    /// Accept REG_SZ and REG_EXPAND_SZ.
    const RRF_RT_STRING: u32 = 0x2 | 0x4;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            hkey: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            ty: *mut u32,
            data: *mut c_void,
            len: *mut u32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Read-only default or named value. Any failure (missing key/value, wrong type) is `None`.
    pub fn get(key: &str, name: Option<&str>) -> Option<String> {
        let (sub, value) = (wide(&format!(r"Software\Classes\{key}")), name.map(wide));
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * std::mem::size_of::<u16>()) as u32;
        let ok = unsafe {
            RegGetValueW(
                HKCU,
                sub.as_ptr(),
                value.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                RRF_RT_STRING,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut c_void,
                &mut len,
            )
        };
        if ok != 0 {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        String::from_utf16(&buf[..n]).ok().map(|s| s.trim_end_matches('\0').to_string()).filter(|s| !s.is_empty())
    }
}

#[cfg(not(windows))]
mod win {
    pub fn get(_key: &str, _name: Option<&str>) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(ext: &'static str, current: Option<&str>, stored: Option<&str>) -> ExtState {
        ExtState { ext, current: current.map(String::from), stored: stored.map(String::from) }
    }

    #[test]
    fn the_open_command_quotes_the_exe_and_the_file() {
        assert_eq!(command(r"C:\Tools\Orca Deck\orcadeck.exe"), r#""C:\Tools\Orca Deck\orcadeck.exe" install "%1""#);
    }

    #[test]
    fn register_remembers_the_previous_default_once_per_extension() {
        let st = [state("streamDeckPlugin", Some("StreamDock.Plugin"), None), state("sdPlugin", None, None)];
        let ops = register_ops(r"C:\orca\orcadeck.exe", &st);

        assert!(ops.contains(&Op::SetValue(PROGID.into(), "PreviousDefault.streamDeckPlugin".into(), "StreamDock.Plugin".into())));
        assert!(!ops.iter().any(|o| matches!(o, Op::SetValue(_, n, _) if n == "PreviousDefault.sdPlugin")));
        for ext in EXTENSIONS {
            assert!(ops.contains(&Op::SetDefault(format!(".{ext}"), PROGID.into())));
            assert!(ops.contains(&Op::SetValue(format!(".{ext}\\OpenWithProgids"), PROGID.into(), String::new())));
        }
        assert!(ops.contains(&Op::SetDefault(
            format!(r"{PROGID}\shell\open\command"),
            r#""C:\orca\orcadeck.exe" install "%1""#.into()
        )));
        assert!(ops.contains(&Op::SetDefault(format!(r"{PROGID}\DefaultIcon"), r#""C:\orca\orcadeck.exe",0"#.into())));
    }

    #[test]
    fn a_second_register_never_overwrites_the_stored_default() {
        let st = [state("streamDeckPlugin", Some(PROGID), Some("StreamDock.Plugin"))];
        let ops = register_ops(r"C:\orca\orcadeck.exe", &st);
        assert!(!ops.iter().any(|o| matches!(o, Op::SetValue(k, n, _) if k == PROGID && n.starts_with("PreviousDefault"))));
        assert!(ops.contains(&Op::SetDefault(".streamDeckPlugin".into(), PROGID.into())));
    }

    #[test]
    fn unregister_restores_what_was_there_before_and_removes_the_progid() {
        let st = [state("streamDeckPlugin", None, Some("StreamDock.Plugin")), state("sdPlugin", None, None)];
        let ops = unregister_ops(&st);

        assert!(ops.contains(&Op::SetDefault(".streamDeckPlugin".into(), "StreamDock.Plugin".into())));
        assert!(ops.contains(&Op::DeleteValue(PROGID.into(), "PreviousDefault.streamDeckPlugin".into())));
        assert!(ops.contains(&Op::DeleteValue(r".streamDeckPlugin\OpenWithProgids".into(), PROGID.into())));
        // No previous default: only our default goes away, the key is left alone.
        assert!(ops.contains(&Op::DeleteDefault(".sdPlugin".into())));
        assert!(!ops.iter().any(|o| matches!(o, Op::DeleteKey(k) if k.starts_with('.'))));
        assert_eq!(ops.last(), Some(&Op::DeleteKey(PROGID.into())));
    }
}

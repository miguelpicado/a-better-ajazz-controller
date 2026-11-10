//! Plugin process launch: argv, the `-info` JSON and the per-launch plugin UUID.

use crate::manifest::Plugin;
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Stdio};

/// A device as reported to plugins in `info.devices` and `deviceDidConnect`.
#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub columns: u32,
    pub rows: u32,
    /// Elgato device type. The AKP153E reports 0 (Stream Deck classic).
    pub kind: u32,
}

impl Device {
    pub(crate) fn info(&self) -> Value {
        json!({"name": self.name, "type": self.kind, "size": {"columns": self.columns, "rows": self.rows}})
    }
}

/// Elgato SDK feature gates are satisfied from 6.9; plugins asking for more get their own value.
const MIN_APP_VERSION: &str = "6.9";

/// Numeric dotted-version max ("3.10" > "3.9"). Non-numeric parts count as 0.
pub fn version_max<'a>(a: &'a str, b: &'a str) -> &'a str {
    let parse = |s: &str| s.split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    if parse(a) >= parse(b) {
        a
    } else {
        b
    }
}

/// The `-info` argument (plugin-host-protocol.md §1.3).
pub fn info(plugin: &Plugin, devices: &[Device], language: &str) -> Value {
    let version = version_max(plugin.min_version().unwrap_or(MIN_APP_VERSION), MIN_APP_VERSION);
    json!({
        "application": {"font": "Segoe UI", "language": language, "platform": "windows",
                        "platformVersion": "10.0", "version": version},
        "colors": {"buttonMouseOverBackgroundColor": "#464646FF", "buttonPressedBackgroundColor": "#303030FF",
                   "buttonPressedBorderColor": "#646464FF", "buttonPressedTextColor": "#969696FF",
                   "highlightColor": "#0078FFFF"},
        "devicePixelRatio": 1,
        "devices": devices.iter().map(|d| {
            let mut v = d.info();
            v["id"] = json!(d.id);
            v
        }).collect::<Vec<_>>(),
        "plugin": {"uuid": plugin.id, "version": plugin.version()},
    })
}

/// A fresh 32-char `[A-Z0-9]` id from the OS CSPRNG (about 165 bits), like the plugin UUID
/// Stream Dock generates on every launch. Used for plugin UUIDs, contexts and inspector tokens,
/// which are the only credentials a socket presents.
pub fn random_uuid() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut out = String::with_capacity(32);
    while out.len() < 32 {
        let mut buf = [0u8; 48];
        getrandom::fill(&mut buf).expect("OS random generator unavailable");
        // Rejection sampling: 252 = 7 * 36, so every symbol is equally likely.
        for b in buf.into_iter().filter(|&b| b < 252).take(32 - out.len()) {
            out.push(ALPHABET[(b % 36) as usize] as char);
        }
    }
    out
}

/// Builds the command for a plugin, or `None` for code it cannot run (`.html` needs a browser).
/// `.exe` runs directly, `.js/.mjs/.cjs` through `node`. The working directory is the plugin folder.
pub fn command(plugin: &Plugin, node: &Path, port: u16, uuid: &str, info: &Value) -> Option<Command> {
    let code = plugin.dir.join(&plugin.code_path);
    let ext = code.extension()?.to_string_lossy().to_ascii_lowercase();
    let mut cmd = match ext.as_str() {
        "exe" => Command::new(&code),
        "js" | "mjs" | "cjs" => {
            let mut c = Command::new(node);
            c.arg(&code);
            c
        }
        _ => return None,
    };
    cmd.args(["-port", &port.to_string(), "-pluginUUID", uuid, "-registerEvent", "registerPlugin"])
        .arg("-info")
        .arg(info.to_string())
        .current_dir(&plugin.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    Some(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn plugin(code: &str, min: Option<&str>) -> Plugin {
        let mut m = json!({"UUID": "com.t", "CodePath": code, "Version": "1.2"});
        if let Some(v) = min {
            m["Software"] = json!({"MinimumVersion": v});
        }
        Plugin::from_manifest(&PathBuf::from("C:/p/com.t.sdPlugin"), m).unwrap()
    }

    #[test]
    fn versions() {
        assert_eq!(version_max("3.10", "6.9"), "6.9");
        assert_eq!(version_max("6.10", "6.9"), "6.10");
        assert_eq!(version_max("7", "6.9"), "7");
    }

    #[test]
    fn info_shape() {
        let dev = Device { id: "D1".into(), name: "AKP153E".into(), columns: 6, rows: 3, kind: 0 };
        let i = info(&plugin("a.js", Some("3.10")), &[dev], "es");
        assert_eq!(i["application"]["version"], "6.9");
        assert_eq!(i["application"]["language"], "es");
        assert_eq!(i["devices"][0]["id"], "D1");
        assert_eq!(i["devices"][0]["size"]["columns"], 6);
        assert_eq!(i["plugin"], json!({"uuid": "com.t", "version": "1.2"}));
    }

    #[test]
    fn uuid_format() {
        let (a, b) = (random_uuid(), random_uuid());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        assert_ne!(a, b);
    }

    #[test]
    fn commands() {
        let node = Path::new("node");
        let info = json!({"k": "v"});
        let c = command(&plugin("bin/plugin.js", None), node, 18618, "U", &info).unwrap();
        assert_eq!(c.get_program(), "node");
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args[0].ends_with("plugin.js"));
        assert_eq!(&args[1..], ["-port", "18618", "-pluginUUID", "U", "-registerEvent", "registerPlugin", "-info", r#"{"k":"v"}"#]);
        assert_eq!(c.get_current_dir(), Some(Path::new("C:/p/com.t.sdPlugin")));

        let c = command(&plugin("x.EXE", None), node, 1, "U", &info).unwrap();
        assert!(c.get_program().to_string_lossy().ends_with("x.EXE"));
        assert!(command(&plugin("index.html", None), node, 1, "U", &info).is_none());
    }
}

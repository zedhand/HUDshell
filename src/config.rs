//! HUDshell configuration: `%APPDATA%\HUDshell\config.json`.
//!
//! * Missing file -> written with defaults. Unreadable/invalid file -> kept as `config.json.bad-<n>`
//!   and defaults are used, so a typo never loses settings silently or blocks startup.
//! * Saved atomically (temp file + rename). Unknown keys are ignored; missing keys take defaults.
//! * Values are validated/clamped on load. Never stores secrets.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::log;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Hotkeys {
    pub click_through: String,
    pub grab_focus: String,
    pub show_hide: String,
    pub cycle_colours: String,
    pub opacity_up: String,
    pub opacity_down: String,
    pub settings: String,
    pub quit: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            click_through: "Ctrl+Alt+T".into(),
            grab_focus: "Ctrl+Alt+G".into(),
            show_hide: "Ctrl+Alt+V".into(),
            cycle_colours: "Ctrl+Alt+C".into(),
            opacity_up: "Ctrl+Alt+]".into(),
            opacity_down: "Ctrl+Alt+[".into(),
            settings: "Ctrl+Alt+S".into(),
            quit: "Ctrl+Alt+Q".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Colours {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// Start in click-through mode (default: on).
    pub click_through: bool,
    /// Window opacity in percent (20-100). 100 = fully opaque.
    pub opacity: u32,
    pub hotkeys: Hotkeys,
    pub font_family: String,
    pub font_size: u32,
    /// Full path of the shell to run instead of the automatic PowerShell 7 search.
    pub shell: Option<String>,
    /// Last window position and size in physical pixels.
    pub window: Option<Geometry>,
    /// Name of a preset (see `presets`) or "custom".
    pub theme: String,
    pub custom_colours: Option<Colours>,
    pub scrollback: u32,
    pub unicode: String,
    pub shift_enter: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            click_through: true,
            opacity: 100,
            hotkeys: Hotkeys::default(),
            font_family: "Cascadia Mono, Consolas, monospace".into(),
            font_size: 14,
            shell: None,
            window: None,
            theme: "midnight".into(),
            custom_colours: None,
            scrollback: 2000,
            unicode: "11".into(),
            shift_enter: "kitty".into(),
        }
    }
}

fn c(bg: &str, fg: &str, cur: &str, sel: &str) -> Colours {
    Colours { background: bg.into(), foreground: fg.into(), cursor: cur.into(), selection: sel.into() }
}

/// Built-in colour presets (background / foreground / cursor / selection).
pub fn presets() -> Vec<(&'static str, Colours)> {
    vec![
        ("midnight", c("#0c1620", "#d7e2ec", "#d7e2ec", "#2b4a66")),
        ("graphite", c("#1c1c1e", "#e5e5e7", "#e5e5e7", "#48484a")),
        ("matrix", c("#000a00", "#33ff66", "#33ff66", "#0a4d1c")),
        ("amber", c("#140c00", "#ffb000", "#ffb000", "#5a3d00")),
        ("dracula", c("#282a36", "#f8f8f2", "#f8f8f2", "#44475a")),
        ("solarized-dark", c("#002b36", "#93a1a1", "#93a1a1", "#073642")),
        ("paper", c("#f6f4ee", "#2b2b2b", "#2b2b2b", "#c9d6e3")),
    ]
}

pub fn is_hex_colour(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Resolved colours for the current theme setting.
pub fn colours_for(cfg: &Config) -> Colours {
    if cfg.theme == "custom" {
        if let Some(cc) = &cfg.custom_colours {
            return cc.clone();
        }
    }
    let all = presets();
    all.iter().find(|(n, _)| *n == cfg.theme).unwrap_or(&all[0]).1.clone()
}

/// The theme name after `cfg.theme` in the cycle (presets, then "custom" if one is defined).
pub fn next_theme(cfg: &Config) -> String {
    let mut names: Vec<String> = presets().iter().map(|(n, _)| n.to_string()).collect();
    if cfg.custom_colours.is_some() {
        names.push("custom".into());
    }
    let i = names.iter().position(|n| *n == cfg.theme).map(|i| i + 1).unwrap_or(0);
    names[i % names.len()].clone()
}

pub fn path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("HUDshell").join("config.json")
}

/// Lowest opacity allowed, so the window can never be made invisible by accident.
pub const MIN_OPACITY: u32 = 20;

fn validate(cfg: &mut Config) {
    cfg.font_size = cfg.font_size.clamp(8, 48);
    cfg.opacity = cfg.opacity.clamp(MIN_OPACITY, 100);
    cfg.scrollback = cfg.scrollback.clamp(100, 100_000);
    if !matches!(cfg.unicode.as_str(), "default" | "11" | "graphemes") {
        cfg.unicode = "11".into();
    }
    if !matches!(cfg.shift_enter.as_str(), "none" | "esc-cr" | "kitty") {
        cfg.shift_enter = "kitty".into();
    }
    if let Some(cc) = &cfg.custom_colours {
        let ok = [&cc.background, &cc.foreground, &cc.cursor, &cc.selection]
            .iter()
            .all(|s| is_hex_colour(s));
        if !ok {
            crate::notice("custom_colours must be #rrggbb values; ignoring them");
            cfg.custom_colours = None;
        }
    }
    if cfg.theme == "custom" && cfg.custom_colours.is_none() {
        cfg.theme = "midnight".into();
    }
    if !presets().iter().any(|(n, _)| *n == cfg.theme) && cfg.theme != "custom" {
        cfg.theme = "midnight".into();
    }
    if let Some(g) = &mut cfg.window {
        g.w = g.w.clamp(240, 8000);
        g.h = g.h.clamp(120, 8000);
    }
    let def = Hotkeys::default();
    for (name, hk, fallback) in [
        ("click_through", &mut cfg.hotkeys.click_through, def.click_through),
        ("grab_focus", &mut cfg.hotkeys.grab_focus, def.grab_focus),
        ("show_hide", &mut cfg.hotkeys.show_hide, def.show_hide),
        ("cycle_colours", &mut cfg.hotkeys.cycle_colours, def.cycle_colours),
        ("opacity_up", &mut cfg.hotkeys.opacity_up, def.opacity_up),
        ("opacity_down", &mut cfg.hotkeys.opacity_down, def.opacity_down),
        ("settings", &mut cfg.hotkeys.settings, def.settings),
        ("quit", &mut cfg.hotkeys.quit, def.quit),
    ] {
        if parse_hotkey(hk).is_none() {
            crate::notice(&format!("Hotkey '{hk}' for {name} is not valid; using {fallback}"));
            *hk = fallback;
        }
    }
}

pub fn load() -> Config {
    let p = path();
    match std::fs::read_to_string(&p) {
        // Editors and PowerShell 5.1 can save UTF-8 with a byte-order mark; JSON parsing must not choke on it.
        Ok(text) => match serde_json::from_str::<Config>(text.trim_start_matches('\u{feff}')) {
            Ok(mut cfg) => {
                validate(&mut cfg);
                log(&format!("config: loaded {}", p.display()));
                cfg
            }
            Err(e) => {
                let mut n = 1;
                let mut bad = p.with_extension(format!("json.bad-{n}"));
                while bad.exists() {
                    n += 1;
                    bad = p.with_extension(format!("json.bad-{n}"));
                }
                let _ = std::fs::rename(&p, &bad);
                crate::notice(&format!(
                    "Your settings file was invalid ({e}); it was kept as {} and defaults are in use",
                    bad.display()
                ));
                let cfg = Config::default();
                save(&cfg);
                cfg
            }
        },
        Err(_) => {
            log(&format!("config: no file yet, writing defaults to {}", p.display()));
            let cfg = Config::default();
            save(&cfg);
            cfg
        }
    }
}

pub fn save(cfg: &Config) {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = p.with_extension("json.tmp");
    match serde_json::to_string_pretty(cfg) {
        Ok(text) => {
            if std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &p)).is_err() {
                log("config: could not save (continuing with in-memory settings)");
            }
        }
        Err(e) => log(&format!("config: could not serialise settings: {e}")),
    }
}

/// "Ctrl+Alt+T" -> (RegisterHotKey modifiers incl. MOD_NOREPEAT, virtual-key code).
pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let mut mods = 0x4000u32; // MOD_NOREPEAT
    let mut key: Option<u32> = None;
    for part in s.split('+').map(|p| p.trim()) {
        match part.to_ascii_lowercase().as_str() {
            "alt" => mods |= 0x1,
            "ctrl" | "control" => mods |= 0x2,
            "shift" => mods |= 0x4,
            "win" | "super" => mods |= 0x8,
            "" => return None,
            k => {
                if key.is_some() {
                    return None;
                }
                key = Some(match k {
                    "space" => 0x20,
                    "enter" | "return" => 0x0D,
                    "tab" => 0x09,
                    "esc" | "escape" => 0x1B,
                    "`" | "backtick" | "grave" => 0xC0,
                    "\\" | "backslash" => 0xDC,
                    ";" | "semicolon" => 0xBA,
                    "'" | "quote" => 0xDE,
                    "," | "comma" => 0xBC,
                    "." | "period" => 0xBE,
                    "/" | "slash" => 0xBF,
                    "-" | "minus" => 0xBD,
                    "=" | "equals" => 0xBB,
                    "[" => 0xDB,
                    "]" => 0xDD,
                    _ if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() => {
                        k.as_bytes()[0].to_ascii_uppercase() as u32
                    }
                    _ if k.starts_with('f') && k[1..].parse::<u32>().map(|n| (1..=24).contains(&n)).unwrap_or(false) => {
                        0x70 + k[1..].parse::<u32>().unwrap() - 1
                    }
                    _ => return None,
                });
            }
        }
    }
    if mods & 0xF == 0 {
        return None; // require at least one modifier so a bare key is never swallowed globally
    }
    key.map(|k| (mods, k))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_parse() {
        assert_eq!(parse_hotkey("Ctrl+Alt+T"), Some((0x4000 | 0x2 | 0x1, b'T' as u32)));
        assert_eq!(parse_hotkey("ctrl+shift+f5"), Some((0x4000 | 0x2 | 0x4, 0x74)));
        assert_eq!(parse_hotkey("Ctrl+Alt+["), Some((0x4000 | 0x2 | 0x1, 0xDB)));
        assert_eq!(parse_hotkey("Ctrl+Alt+]"), Some((0x4000 | 0x2 | 0x1, 0xDD)));
        assert_eq!(parse_hotkey("Win+Space"), Some((0x4000 | 0x8, 0x20)));
    }

    #[test]
    fn hotkeys_reject_unsafe_or_bad() {
        assert_eq!(parse_hotkey("T"), None, "a bare key must never be grabbed globally");
        assert_eq!(parse_hotkey("Ctrl+Alt"), None, "no key");
        assert_eq!(parse_hotkey("Ctrl+A+B"), None, "two keys");
        assert_eq!(parse_hotkey("Ctrl++"), None);
        assert_eq!(parse_hotkey("Ctrl+F25"), None);
        assert_eq!(parse_hotkey(""), None);
        assert_eq!(parse_hotkey("Ctrl+Banana"), None);
    }

    #[test]
    fn default_hotkeys_are_all_valid_and_distinct() {
        let h = Hotkeys::default();
        let all = [&h.click_through, &h.grab_focus, &h.show_hide, &h.cycle_colours, &h.opacity_up, &h.opacity_down, &h.settings, &h.quit];
        let mut seen = std::collections::HashSet::new();
        for s in all {
            let parsed = parse_hotkey(s).unwrap_or_else(|| panic!("default hotkey {s} must parse"));
            assert!(seen.insert(parsed), "duplicate default hotkey {s}");
        }
    }

    #[test]
    fn opacity_is_clamped() {
        let mut c = Config { opacity: 0, ..Config::default() };
        validate(&mut c);
        assert_eq!(c.opacity, MIN_OPACITY);
        c.opacity = 5000;
        validate(&mut c);
        assert_eq!(c.opacity, 100);
        c.opacity = 55;
        validate(&mut c);
        assert_eq!(c.opacity, 55);
    }

    #[test]
    fn other_values_are_clamped_and_defaulted() {
        let mut c = Config { font_size: 1, scrollback: 1, unicode: "x".into(), shift_enter: "y".into(), theme: "nope".into(), ..Config::default() };
        c.window = Some(Geometry { x: 0, y: 0, w: 1, h: 1 });
        validate(&mut c);
        assert_eq!(c.font_size, 8);
        assert_eq!(c.scrollback, 100);
        assert_eq!(c.unicode, "11");
        assert_eq!(c.shift_enter, "kitty");
        assert_eq!(c.theme, "midnight");
        let g = c.window.unwrap();
        assert_eq!((g.w, g.h), (240, 120));
    }

    #[test]
    fn bad_hotkey_falls_back_to_default() {
        let mut c = Config::default();
        c.hotkeys.quit = "Q".into();
        validate(&mut c);
        assert_eq!(c.hotkeys.quit, Hotkeys::default().quit);
    }

    #[test]
    fn missing_keys_take_defaults_and_unknown_keys_are_ignored() {
        let c: Config = serde_json::from_str(r#"{"opacity": 70, "somethingNew": true}"#).unwrap();
        assert_eq!(c.opacity, 70);
        assert!(c.click_through, "click-through defaults on");
        assert_eq!(c.theme, "midnight");
    }

    #[test]
    fn config_with_a_byte_order_mark_parses() {
        let text = "\u{feff}{\"opacity\": 40}";
        let c: Config = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap();
        assert_eq!(c.opacity, 40);
    }

    #[test]
    fn colours_and_theme_cycle() {
        assert!(is_hex_colour("#0a1B2c"));
        assert!(!is_hex_colour("0a1b2c"));
        assert!(!is_hex_colour("#12345"));
        assert!(!is_hex_colour("#12345g"));
        let mut c = Config::default();
        let first = c.theme.clone();
        let n = presets().len();
        for _ in 0..n {
            c.theme = next_theme(&c);
        }
        assert_eq!(c.theme, first, "cycling through every preset returns to the start");
        c.custom_colours = Some(presets()[0].1.clone());
        c.theme = presets()[n - 1].0.to_string();
        assert_eq!(next_theme(&c), "custom");
        c.theme = "custom".into();
        assert_eq!(colours_for(&c).background, presets()[0].1.background);
    }

    #[test]
    fn presets_have_valid_colours() {
        for (name, p) in presets() {
            for v in [&p.background, &p.foreground, &p.cursor, &p.selection] {
                assert!(is_hex_colour(v), "preset {name} has an invalid colour {v}");
            }
        }
    }

    #[test]
    fn assets_are_embedded() {
        for p in ["/index.html", "/app.js", "/xterm.js", "/xterm.css"] {
            let a = crate::assets::find(p).unwrap_or_else(|| panic!("missing embedded asset {p}"));
            assert!(!a.data.is_empty());
        }
        assert!(crate::assets::find("/../secret").is_none());
        assert!(crate::assets::find("/index.html?x=1").is_none(), "the query string is stripped by the caller");
    }
}

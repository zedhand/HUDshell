//! HUDshell configuration: `%APPDATA%\HUDshell\config.json`.
//!
//! * Missing file -> written with defaults. A file that is not JSON -> renamed to `config.json.bad-<n>` and defaults are
//!   used; a value of the wrong type -> only that setting takes its default (the file is copied aside first); a file that
//!   cannot be read -> left alone and never written that run. A typo never loses settings silently or blocks startup.
//! * Saved durably (temp file flushed, then renamed), and only over the version HUDshell last read or wrote, so a hand
//!   edit that was not reloaded yet is never overwritten. Unknown keys are ignored; missing keys take defaults.
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
    /// Ghost preset: click-through plus a dimmed window in one key; press again to restore.
    pub ghost: String,
    /// Move to the next / previous screen corner (clockwise), keeping the size.
    pub corner_next: String,
    pub corner_prev: String,
    /// Cycle through the `sizes` presets, keeping the nearest corner anchored.
    pub size_cycle: String,
    pub font_bigger: String,
    pub font_smaller: String,
    /// Put the focused terminal's text size back to `font_size` (with `text_scale_hotkeys = "all"`: every terminal).
    pub font_reset: String,
    /// Next / previous shell (works while HUDshell is passive: they are global hotkeys).
    pub shell_next: String,
    pub shell_prev: String,
    /// Open a new shell with the default profile / close the active shell (press twice to confirm).
    pub shell_new: String,
    pub shell_close: String,
    /// Show the shell list overlay for a moment.
    pub shell_list: String,
    /// Open the profile picker (grabs the keyboard; press a number to start that profile).
    pub shell_picker: String,
    /// Jump to the next shell that has unseen output or rang the bell.
    pub shell_attention: String,
    /// Modifiers for jumping straight to shell 1-9: "Ctrl+Alt" means Ctrl+Alt+1 ... Ctrl+Alt+9.
    pub shell_jump: String,
    /// Step through the split layouts: single, side by side, stacked, 2 x 2.
    pub layout_cycle: String,
    /// Zoom the focused pane to the whole window, or back.
    pub pane_zoom: String,
    /// Move the keyboard focus to the next pane.
    pub pane_next: String,
    /// Move the active shell into a window of its own.
    pub window_detach: String,
    /// Make the next HUDshell window the one the hotkeys act on.
    pub window_next: String,
    /// Move the active shell into the next window (a window's only shell takes its window with it).
    pub shell_move: String,
    /// Ghost mode for every window at once (the same hotkey again leaves it).
    pub ghost_all: String,
    /// Hide every window, or show them all again.
    pub show_hide_all: String,
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
            ghost: "Ctrl+Alt+X".into(),
            corner_next: "Ctrl+Alt+.".into(),
            corner_prev: "Ctrl+Alt+Shift+.".into(),
            size_cycle: "Ctrl+Alt+/".into(),
            font_bigger: "Ctrl+Alt+=".into(),
            font_smaller: "Ctrl+Alt+-".into(),
            font_reset: "Ctrl+Alt+0".into(),
            shell_next: "Ctrl+Alt+PageDown".into(),
            shell_prev: "Ctrl+Alt+PageUp".into(),
            shell_new: "Ctrl+Alt+N".into(),
            shell_close: "Ctrl+Alt+W".into(),
            shell_list: "Ctrl+Alt+`".into(),
            shell_picker: "Ctrl+Alt+Shift+N".into(),
            shell_attention: "Ctrl+Alt+;".into(),
            shell_jump: "Ctrl+Alt".into(),
            layout_cycle: "Ctrl+Alt+'".into(),
            pane_zoom: "Ctrl+Alt+Z".into(),
            pane_next: "Ctrl+Alt+O".into(),
            window_detach: "Ctrl+Alt+D".into(),
            window_next: "Ctrl+Alt+Shift+W".into(),
            shell_move: "Ctrl+Alt+Shift+D".into(),
            ghost_all: "Ctrl+Alt+Shift+X".into(),
            show_hide_all: "Ctrl+Alt+Shift+V".into(),
        }
    }
}

/// A kind of shell to start: where, what to run in it, and an optional accent colour for its tab.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Start directory; `%VAR%` environment variables are expanded. Default: the user profile folder.
    pub dir: Option<String>,
    /// Typed into the new shell (followed by Enter), for example `claude`.
    pub command: Option<String>,
    /// Full path of a different shell to run for this profile.
    pub shell: Option<String>,
    /// Tab accent colour, `#rrggbb`.
    pub color: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self { name: "pwsh".into(), dir: None, command: None, shell: None, color: None }
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
    /// Starting text size in points (8-48) of every terminal.
    pub font_size: u32,
    /// Percent (50-300) applied on top of every terminal's size: 100 = as set, 150 = half as big again.
    pub text_scale: u32,
    /// Percent (50-300) for HUDshell's own text: tab names, pane names, overlays, the picker and the settings panel.
    pub ui_scale: u32,
    /// Line height of the terminals in percent of the font's own (100-200).
    pub line_height: u32,
    /// Extra space between characters in pixels (0-10).
    pub letter_spacing: u32,
    /// What the bigger / smaller / reset text hotkeys change: "focused" = only the terminal that has the keyboard (each
    /// terminal keeps its own size for the run), "all" = every terminal together (and `font_size` is saved).
    pub text_scale_hotkeys: String,
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
    /// Opacity (percent, 20-100) used by the ghost preset. Never lowers it below the 20% floor.
    pub ghost_opacity: u32,
    /// Copy the selection to the clipboard when the mouse button is released (off by default).
    pub copy_on_select: bool,
    /// "off" or "paste": right-click copies the selection, or pastes when nothing is selected.
    pub right_click: String,
    /// "off" or "write": whether programs may SET the clipboard with OSC 52. Reads are never answered.
    pub osc52: String,
    /// Pulse the window border when a program rings the bell or sends an OSC 9 / OSC 777 notification
    /// while HUDshell does not hold the keyboard.
    pub attention_cue: bool,
    /// Also show a tray notification for such requests while the window is hidden.
    pub attention_toast: bool,
    /// Dragging within this many pixels (96-dpi) of a screen edge snaps to it. 0 turns snapping off.
    pub snap_distance: u32,
    /// Gap (96-dpi pixels) kept between a snapped or docked window and the screen edge.
    pub snap_margin: u32,
    /// What maximizing (drag to the top edge, double-click the top strip) fills: `monitor` = the whole monitor, over the taskbar;
    /// `work` = the usable area, leaving the taskbar visible.
    pub maximize_area: String,
    /// Remember the first window's place for each program that was in front when it was moved, and put it back when that program is in front
    /// again. Opt in: it reads the name of the foreground program's executable (never window titles); the names stay in this file.
    pub game_positions: bool,
    /// The places `game_positions` keeps (written by HUDshell).
    pub game_places: Vec<crate::gamepos::Place>,
    /// A game in exclusive fullscreen cannot show HUDshell, and taking the keyboard from it minimizes it. With this on (the default) HUDshell does not
    /// take the keyboard from a program listed in `exclusive_programs`, and learns a program when it is minimized by a grab (it then hands the keyboard back).
    pub exclusive_guard: bool,
    /// Executable file names (lower case, for example `game.exe`) of programs that run in exclusive fullscreen. Edit by hand; HUDshell adds the ones it learns.
    pub exclusive_programs: Vec<String>,
    /// Dropping the window with the cursor on the left or right edge of a screen makes it fill that half of the screen (like a normal window).
    pub edge_tiling: bool,
    /// Size presets [width, height] in 96-dpi pixels for the size hotkey.
    pub sizes: Vec<[u32; 2]>,
    /// Opacity used while HUDshell holds the keyboard. `null` = same as `opacity`.
    pub active_opacity: Option<u32>,
    /// Dim the window after this many seconds without shell output (0 = off). Output brings it back.
    pub idle_dim_seconds: u32,
    /// Opacity (20-100) while dimmed for idleness; never brighter than the normal opacity.
    pub idle_opacity: u32,
    /// Start with the window hidden (show it with the show/hide hotkey or the tray menu).
    pub start_hidden: bool,
    /// Start in ghost mode.
    pub start_ghost: bool,
    /// How the shell list shows: "auto" = a tab strip once there is more than one shell, "always" = a tab
    /// strip even for one shell, "overlay" = no strip, only a short overlay on switch and on the list hotkey.
    pub tabs: String,
    /// Label a tab with the program running in it: "known" = Claude Code, Codex and a few similar tools, "all" =
    /// also any other program, "off" = never. Found by looking at the processes below the shell; read-only.
    pub tab_program: String,
    /// Kinds of shell the new-shell hotkey and the picker can start.
    pub profiles: Vec<Profile>,
    /// Name of the profile the new-shell hotkey uses (default: the first one).
    pub default_profile: String,
    /// Split layout at start: "single", "side-by-side", "stacked" or "grid" (2 x 2).
    pub layout: String,
    /// When a layout has more panes than there are shells: "new" starts shells (default profile) for them,
    /// "empty" leaves the panes empty until a shell is started there.
    pub layout_fill: String,
    /// How panes get their shells: "shared" = every pane shows a shell from the one tab list; "own" = each pane
    /// has its own list of shells (the tab strip, switch and jump hotkeys act on the focused pane's list).
    pub pane_shells: String,
    /// "off" (default): start with one shell. "shells": the open shells and the layout are saved while HUDshell runs and started again
    /// at the next start (from their profiles; folders and running programs are not restored).
    pub restore_session: String,
    /// Save the chosen layout (and divider positions) in `layout` / `split` whenever it changes.
    pub remember_layout: bool,
    /// Share in percent (20-80) of the first column and of the first row.
    pub split: [u32; 2],
    /// Draggable dividers between panes (they need the mouse, so the window must not be click-through).
    pub pane_dividers: bool,
    /// Smallest pane worth drawing, [columns, rows]. Smaller windows show fewer panes.
    pub pane_min: [u32; 2],
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
            text_scale: 100,
            ui_scale: 100,
            line_height: 100,
            letter_spacing: 0,
            text_scale_hotkeys: "focused".into(),
            shell: None,
            window: None,
            theme: "midnight".into(),
            custom_colours: None,
            scrollback: 2000,
            unicode: "11".into(),
            shift_enter: "kitty".into(),
            ghost_opacity: 40,
            copy_on_select: false,
            right_click: "off".into(),
            osc52: "off".into(),
            attention_cue: true,
            attention_toast: true,
            snap_distance: 16,
            snap_margin: 8,
            maximize_area: "monitor".into(),
            edge_tiling: true,
            game_positions: false,
            game_places: Vec::new(),
            exclusive_guard: true,
            exclusive_programs: Vec::new(),
            sizes: vec![[640, 360], [900, 520], [1280, 720]],
            active_opacity: None,
            idle_dim_seconds: 0,
            idle_opacity: 50,
            start_hidden: false,
            start_ghost: false,
            tabs: "auto".into(),
            tab_program: "known".into(),
            profiles: vec![Profile::default()],
            default_profile: String::new(),
            layout: "single".into(),
            layout_fill: "new".into(),
            pane_shells: "shared".into(),
            restore_session: "off".into(),
            remember_layout: false,
            split: [50, 50],
            pane_dividers: false,
            pane_min: [30, 6],
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
    if cfg.theme == "custom"
        && let Some(cc) = &cfg.custom_colours {
            return cc.clone();
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
    roaming_app_data().join("HUDshell").join("config.json")
}

/// A folder Windows knows (roaming or local app data). `None` only if the shell cannot tell.
pub fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from).filter(|p| p.is_absolute())
    }
}

/// `%APPDATA%` when it is an absolute path, else the roaming app-data folder Windows reports, else the temp folder.
/// Never the current folder: a relative path would let a settings file in whatever folder HUDshell was started from take effect.
pub fn roaming_app_data() -> PathBuf {
    env_dir("APPDATA")
        .or_else(|| known_folder(&windows::Win32::UI::Shell::FOLDERID_RoamingAppData))
        .unwrap_or_else(std::env::temp_dir)
}

/// `%LOCALAPPDATA%` when absolute, else the local app-data folder Windows reports, else the temp folder.
pub fn local_app_data() -> PathBuf {
    env_dir("LOCALAPPDATA")
        .or_else(|| known_folder(&windows::Win32::UI::Shell::FOLDERID_LocalAppData))
        .unwrap_or_else(std::env::temp_dir)
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// Saved window coordinates beyond this (physical pixels) cannot be on any screen; clamping keeps the arithmetic far from overflow.
pub const COORD_LIMIT: i32 = 100_000;

/// Lowest opacity allowed, so the window can never be made invisible by accident.
pub const MIN_OPACITY: u32 = 20;

fn validate(cfg: &mut Config) {
    crate::gamepos::sanitize(&mut cfg.game_places);
    crate::gamepos::sanitize_exclusive(&mut cfg.exclusive_programs);
    cfg.font_size = cfg.font_size.clamp(8, 48);
    cfg.text_scale = cfg.text_scale.clamp(50, 300);
    cfg.ui_scale = cfg.ui_scale.clamp(50, 300);
    cfg.line_height = cfg.line_height.clamp(100, 200);
    cfg.letter_spacing = cfg.letter_spacing.min(10);
    if !matches!(cfg.text_scale_hotkeys.as_str(), "focused" | "all") {
        cfg.text_scale_hotkeys = "focused".into();
    }
    cfg.opacity = cfg.opacity.clamp(MIN_OPACITY, 100);
    cfg.scrollback = cfg.scrollback.clamp(100, 100_000);
    if !matches!(cfg.unicode.as_str(), "default" | "11" | "graphemes") {
        cfg.unicode = "11".into();
    }
    if !matches!(cfg.shift_enter.as_str(), "none" | "esc-cr" | "kitty") {
        cfg.shift_enter = "kitty".into();
    }
    cfg.ghost_opacity = cfg.ghost_opacity.clamp(MIN_OPACITY, 100);
    if !matches!(cfg.tabs.as_str(), "auto" | "always" | "overlay") {
        cfg.tabs = "auto".into();
    }
    validate_profiles(cfg);
    if !matches!(cfg.tab_program.as_str(), "known" | "all" | "off") {
        cfg.tab_program = "known".into();
    }
    cfg.layout = crate::layout::Preset::parse(&cfg.layout).unwrap_or(crate::layout::Preset::Single).name().to_string();
    if !matches!(cfg.layout_fill.as_str(), "new" | "empty") {
        cfg.layout_fill = "new".into();
    }
    if !matches!(cfg.restore_session.as_str(), "off" | "shells") {
        cfg.restore_session = "off".into();
    }
    cfg.maximize_area = cfg.maximize_area.trim().to_lowercase();
    if !matches!(cfg.maximize_area.as_str(), "monitor" | "work") {
        cfg.maximize_area = "monitor".into();
    }
    if !matches!(cfg.pane_shells.as_str(), "shared" | "own") {
        cfg.pane_shells = "shared".into();
    }
    for v in cfg.split.iter_mut() {
        *v = (*v).clamp(crate::layout::SPLIT_MIN, crate::layout::SPLIT_MAX);
    }
    cfg.pane_min = [cfg.pane_min[0].clamp(10, 200), cfg.pane_min[1].clamp(3, 100)];
    cfg.snap_distance = cfg.snap_distance.min(200);
    cfg.active_opacity = cfg.active_opacity.map(|v| v.clamp(MIN_OPACITY, 100));
    cfg.idle_dim_seconds = cfg.idle_dim_seconds.min(3600);
    cfg.idle_opacity = cfg.idle_opacity.clamp(MIN_OPACITY, 100);
    cfg.snap_margin = cfg.snap_margin.min(200);
    cfg.sizes.retain(|s| s[0] >= 240 && s[1] >= 120);
    for s in cfg.sizes.iter_mut() {
        s[0] = s[0].min(8000);
        s[1] = s[1].min(8000);
    }
    cfg.sizes.truncate(8);
    if cfg.sizes.is_empty() {
        cfg.sizes = Config::default().sizes;
    }
    if !matches!(cfg.right_click.as_str(), "off" | "paste") {
        cfg.right_click = "off".into();
    }
    if !matches!(cfg.osc52.as_str(), "off" | "write") {
        cfg.osc52 = "off".into();
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
        g.x = g.x.clamp(-COORD_LIMIT, COORD_LIMIT);
        g.y = g.y.clamp(-COORD_LIMIT, COORD_LIMIT);
    }
    if parse_hotkey(&format!("{}+1", cfg.hotkeys.shell_jump)).is_none() {
        let fallback = Hotkeys::default().shell_jump;
        crate::notice(&format!("shell_jump '{}' is not valid; using {fallback}", cfg.hotkeys.shell_jump));
        cfg.hotkeys.shell_jump = fallback;
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
        ("ghost", &mut cfg.hotkeys.ghost, def.ghost),
        ("corner_next", &mut cfg.hotkeys.corner_next, def.corner_next),
        ("corner_prev", &mut cfg.hotkeys.corner_prev, def.corner_prev),
        ("size_cycle", &mut cfg.hotkeys.size_cycle, def.size_cycle),
        ("font_bigger", &mut cfg.hotkeys.font_bigger, def.font_bigger),
        ("font_smaller", &mut cfg.hotkeys.font_smaller, def.font_smaller),
        ("font_reset", &mut cfg.hotkeys.font_reset, def.font_reset),
        ("shell_next", &mut cfg.hotkeys.shell_next, def.shell_next),
        ("shell_prev", &mut cfg.hotkeys.shell_prev, def.shell_prev),
        ("shell_new", &mut cfg.hotkeys.shell_new, def.shell_new),
        ("shell_close", &mut cfg.hotkeys.shell_close, def.shell_close),
        ("shell_list", &mut cfg.hotkeys.shell_list, def.shell_list),
        ("shell_picker", &mut cfg.hotkeys.shell_picker, def.shell_picker),
        ("shell_attention", &mut cfg.hotkeys.shell_attention, def.shell_attention),
        ("layout_cycle", &mut cfg.hotkeys.layout_cycle, def.layout_cycle),
        ("pane_zoom", &mut cfg.hotkeys.pane_zoom, def.pane_zoom),
        ("pane_next", &mut cfg.hotkeys.pane_next, def.pane_next),
        ("window_detach", &mut cfg.hotkeys.window_detach, def.window_detach),
        ("window_next", &mut cfg.hotkeys.window_next, def.window_next),
        ("shell_move", &mut cfg.hotkeys.shell_move, def.shell_move),
        ("ghost_all", &mut cfg.hotkeys.ghost_all, def.ghost_all),
        ("show_hide_all", &mut cfg.hotkeys.show_hide_all, def.show_hide_all),
    ] {
        if parse_hotkey(hk).is_none() {
            crate::notice(&format!("Hotkey '{hk}' for {name} is not valid; using {fallback}"));
            *hk = fallback;
        }
    }
}

/// Keep profiles sane: at most 12, unique non-empty names, no control characters in commands, valid colours.
fn validate_profiles(cfg: &mut Config) {
    let clean = |s: &str| s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Profile> = Vec::new();
    for p in cfg.profiles.drain(..) {
        let name = clean(&p.name).chars().take(24).collect::<String>();
        if name.is_empty() || !seen.insert(name.to_lowercase()) || out.len() >= 12 {
            continue;
        }
        let opt = |v: Option<String>, max: usize| {
            v.map(|x| clean(&x).chars().take(max).collect::<String>()).filter(|x| !x.is_empty())
        };
        out.push(Profile {
            name,
            dir: opt(p.dir, 260),
            command: opt(p.command, 500),
            shell: opt(p.shell, 260),
            color: p.color.filter(|c| is_hex_colour(c)),
        });
    }
    if out.is_empty() {
        out.push(Profile::default());
    }
    cfg.profiles = out;
    cfg.default_profile = clean(&cfg.default_profile);
    if !cfg.default_profile.is_empty() && !cfg.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(&cfg.default_profile)) {
        cfg.default_profile = String::new();
    }
}

/// The profile the new-shell hotkey starts: the configured default, else the first.
pub fn default_profile(cfg: &Config) -> &Profile {
    cfg.profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&cfg.default_profile))
        .unwrap_or(&cfg.profiles[0])
}

/// What identifies one version of the file on disk: modified time and length.
pub type Stamp = (std::time::SystemTime, u64);

/// The version of config.json HUDshell last read successfully or wrote itself. `save` only replaces the file while it is still
/// that version, so a hand edit that has not been reloaded yet (or a broken file the user is fixing) is never overwritten.
static KNOWN: std::sync::Mutex<Option<Stamp>> = std::sync::Mutex::new(None);
/// Set when the file exists but could not be read or kept aside at start: HUDshell then never writes it.
static SAVE_BLOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// At most this many `config.json.bad-<n>` copies; the last one is reused after that.
const MAX_BAD_COPIES: u32 = 10;

pub fn stamp_of(p: &std::path::Path) -> Option<Stamp> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

fn set_known(s: Option<Stamp>) {
    *KNOWN.lock().unwrap_or_else(|p| p.into_inner()) = s;
}

/// Settings text from the file's bytes: UTF-8 (with or without a byte-order mark) or UTF-16 with a byte-order mark
/// (what Windows PowerShell 5.1's `>` and `Out-File` write).
pub fn decode_text(bytes: &[u8]) -> Result<String, String> {
    let utf16 = |b: &[u8], le: bool| {
        let units: Vec<u16> =
            b.as_chunks::<2>().0.iter().map(|c| if le { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) }).collect();
        String::from_utf16(&units).map_err(|_| "it is not valid UTF-16 text".to_string())
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()).map_err(|_| "it is not valid UTF-8 text".to_string()),
        _ => String::from_utf8(bytes.to_vec()).map_err(|_| "it is not valid UTF-8 text".to_string()),
    }
}

/// Parse settings, keeping every top-level value that is valid on its own: one wrong value (say `"opacity": 85.5`)
/// costs that one setting, not all of them. Returns the settings and the names of the values that were dropped.
/// A file that is not a JSON object at all is an error.
pub fn parse_lenient(text: &str) -> Result<(Config, Vec<String>), String> {
    let text = text.trim_start_matches('\u{feff}');
    if let Ok(cfg) = serde_json::from_str::<Config>(text) {
        return Ok((cfg, Vec::new()));
    }
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let serde_json::Value::Object(given) = value else {
        return Err("it is not a JSON object".into());
    };
    let Ok(serde_json::Value::Object(mut merged)) = serde_json::to_value(Config::default()) else {
        return Err("internal: defaults could not be serialised".into());
    };
    let mut dropped = Vec::new();
    for (k, v) in given {
        let mut trial = merged.clone();
        trial.insert(k.clone(), v.clone());
        if serde_json::from_value::<Config>(serde_json::Value::Object(trial)).is_ok() {
            merged.insert(k, v);
        } else {
            dropped.push(k);
        }
    }
    let cfg = serde_json::from_value::<Config>(serde_json::Value::Object(merged)).map_err(|e| e.to_string())?;
    dropped.sort();
    Ok((cfg, dropped))
}

/// A free `config.json.bad-<n>` name (the last one is reused once there are `MAX_BAD_COPIES`).
fn bad_copy_path(p: &std::path::Path) -> PathBuf {
    let mut n = 1;
    let mut bad = p.with_extension(format!("json.bad-{n}"));
    while bad.exists() && n < MAX_BAD_COPIES {
        n += 1;
        bad = p.with_extension(format!("json.bad-{n}"));
    }
    bad
}

pub fn load() -> Config {
    let p = path();
    let stamp = stamp_of(&p);
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log(&format!("config: no file yet, writing defaults to {}", p.display()));
            let cfg = Config::default();
            save(&cfg);
            return cfg;
        }
        Err(e) => {
            // Locked by an editor or a scanner, or no access: never replace a file HUDshell could not read.
            SAVE_BLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
            crate::notice(&format!(
                "Your settings file could not be read ({e}); defaults are in use for this run and the file was left as it is. Restart HUDshell once it can be read."
            ));
            return Config::default();
        }
    };
    let parsed = decode_text(&bytes).and_then(|t| parse_lenient(&t));
    match parsed {
        Ok((mut cfg, dropped)) => {
            validate(&mut cfg);
            if dropped.is_empty() {
                set_known(stamp);
                log(&format!("config: loaded {}", p.display()));
            } else {
                // Keep a copy of the file as written before HUDshell ever saves over the dropped values.
                let bad = bad_copy_path(&p);
                if std::fs::copy(&p, &bad).is_ok() {
                    set_known(stamp);
                    crate::notice(&format!(
                        "These settings were not valid and their defaults are in use: {}. Your file was copied to {}",
                        dropped.join(", "),
                        bad.display()
                    ));
                } else {
                    SAVE_BLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
                    crate::notice(&format!(
                        "These settings were not valid and their defaults are in use: {}. The file could not be copied, so HUDshell will not save settings this run.",
                        dropped.join(", ")
                    ));
                }
            }
            cfg
        }
        Err(e) => {
            let bad = bad_copy_path(&p);
            match std::fs::rename(&p, &bad) {
                Ok(()) => {
                    crate::notice(&format!(
                        "Your settings file was invalid ({e}); it was kept as {} and defaults are in use",
                        bad.display()
                    ));
                    let cfg = Config::default();
                    save(&cfg);
                    cfg
                }
                Err(re) => {
                    SAVE_BLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
                    crate::notice(&format!(
                        "Your settings file is invalid ({e}) and could not be set aside ({re}); defaults are in use for this run and the file was left as it is."
                    ));
                    Config::default()
                }
            }
        }
    }
}

/// Parse and validate settings text without touching any file (a reload while HUDshell runs).
/// Strict about values: a reload that would silently drop one of them is refused, so the settings in use stay.
pub fn try_parse(text: &str) -> Result<Config, String> {
    let (mut cfg, dropped) = parse_lenient(text)?;
    if !dropped.is_empty() {
        return Err(format!("these values are not valid: {}", dropped.join(", ")));
    }
    validate(&mut cfg);
    Ok(cfg)
}

/// Read the settings file for a reload: unlike `load`, a bad file is left alone (the settings in use stay in use, and
/// HUDshell does not write the file until it is valid again).
pub fn try_load() -> Result<Config, String> {
    let p = path();
    let stamp = stamp_of(&p);
    let bytes = std::fs::read(&p).map_err(|e| format!("cannot read it: {e}"))?;
    let cfg = try_parse(&decode_text(&bytes)?)?;
    // only a file that was read in full and is valid becomes the version `save` may replace
    if stamp == stamp_of(&p) {
        set_known(stamp);
    }
    Ok(cfg)
}

/// Top-level settings whose values differ between `a` and `b`, sorted. The window position is ignored: HUDshell writes it itself.
pub fn changed_keys(a: &Config, b: &Config) -> Vec<String> {
    let (Ok(serde_json::Value::Object(x)), Ok(serde_json::Value::Object(y))) = (serde_json::to_value(a), serde_json::to_value(b)) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = Vec::new();
    for (k, v) in &y {
        if k != "window" && x.get(k) != Some(v) {
            keys.push(k.clone());
        }
    }
    for k in x.keys() {
        if k != "window" && !y.contains_key(k) {
            keys.push(k.clone());
        }
    }
    keys.sort();
    keys
}

/// Write `text` to `p` so that a crash or power loss leaves either the old file or the new one, never an empty or half
/// one: a temp file is written and flushed to the disk, then renamed over `p`. The temp file is removed on failure.
pub fn write_durable(p: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut name = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{}.tmp", std::process::id()));
    let tmp = p.with_file_name(name);
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, p)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Whether `save` may replace the file now: it is missing, or it is still the version HUDshell last read or wrote.
fn may_replace(known: Option<Stamp>, on_disk: Option<Stamp>) -> bool {
    match on_disk {
        None => true,
        Some(d) => known == Some(d),
    }
}

pub fn save(cfg: &Config) {
    if SAVE_BLOCKED.load(std::sync::atomic::Ordering::SeqCst) {
        log("config: not saved (the file could not be read or set aside at start)");
        return;
    }
    let p = path();
    let text = match serde_json::to_string_pretty(cfg) {
        Ok(t) => t,
        Err(e) => {
            log(&format!("config: could not serialise settings: {e}"));
            return;
        }
    };
    let mut known = KNOWN.lock().unwrap_or_else(|p| p.into_inner());
    if !may_replace(*known, stamp_of(&p)) {
        // Changed by hand since HUDshell read it (the reload picks it up), or invalid and being fixed: leave it alone.
        log("config: not saved (the file changed on disk and has not been reloaded yet)");
        return;
    }
    match write_durable(&p, &text) {
        Ok(()) => *known = stamp_of(&p),
        Err(e) => log(&format!("config: could not save ({e}); continuing with in-memory settings")),
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
                    "pageup" | "pgup" => 0x21,
                    "pagedown" | "pgdn" => 0x22,
                    "home" => 0x24,
                    "end" => 0x23,
                    "insert" => 0x2D,
                    "left" => 0x25,
                    "up" => 0x26,
                    "right" => 0x27,
                    "down" => 0x28,
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
    if mods & 0xB == 0 {
        // require Ctrl, Alt or Win: a bare key, or Shift+key (every capital letter), would be swallowed system-wide
        return None;
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
        let all = [&h.click_through, &h.grab_focus, &h.show_hide, &h.cycle_colours, &h.opacity_up, &h.opacity_down, &h.settings, &h.quit, &h.ghost, &h.corner_next, &h.corner_prev, &h.size_cycle, &h.font_bigger, &h.font_smaller, &h.font_reset,
            &h.shell_next, &h.shell_prev, &h.shell_new, &h.shell_close, &h.shell_list, &h.shell_picker, &h.shell_attention,
            &h.layout_cycle, &h.pane_zoom, &h.pane_next, &h.window_detach, &h.window_next, &h.shell_move, &h.ghost_all, &h.show_hide_all];
        let mut seen = std::collections::HashSet::new();
        for s in all {
            let parsed = parse_hotkey(s).unwrap_or_else(|| panic!("default hotkey {s} must parse"));
            assert!(seen.insert(parsed), "duplicate default hotkey {s}");
        }
        for n in 1..=9 {
            let s = format!("{}+{n}", h.shell_jump);
            let parsed = parse_hotkey(&s).unwrap_or_else(|| panic!("jump hotkey {s} must parse"));
            assert!(seen.insert(parsed), "jump hotkey {s} collides with another default");
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
    fn new_policy_settings_are_validated() {
        let mut c = Config { ghost_opacity: 1, right_click: "x".into(), osc52: "read".into(), ..Config::default() };
        validate(&mut c);
        assert_eq!(c.ghost_opacity, MIN_OPACITY);
        assert_eq!((c.right_click.as_str(), c.osc52.as_str()), ("off", "off"));
        c.ghost_opacity = 900;
        c.right_click = "paste".into();
        c.osc52 = "write".into();
        validate(&mut c);
        assert_eq!(c.ghost_opacity, 100);
        assert_eq!((c.right_click.as_str(), c.osc52.as_str()), ("paste", "write"));
        let d = Config::default();
        assert!(!d.copy_on_select, "copy-on-select is opt-in");
        assert_eq!((d.right_click.as_str(), d.osc52.as_str()), ("off", "off"), "clipboard policies are opt-in");
        assert!(d.attention_cue && d.attention_toast);
        assert_eq!(d.ghost_opacity, 40);
    }

    #[test]
    fn placement_settings_are_validated() {
        let mut c = Config { snap_distance: 9999, snap_margin: 9999, sizes: vec![[10, 10], [99999, 99999]], ..Config::default() };
        validate(&mut c);
        assert_eq!((c.snap_distance, c.snap_margin), (200, 200));
        assert_eq!(c.sizes, vec![[8000, 8000]], "too-small presets are dropped, huge ones clamped");
        c.sizes = vec![[10, 10]];
        validate(&mut c);
        assert_eq!(c.sizes, Config::default().sizes, "an empty list falls back to the defaults");
    }

    #[test]
    fn opacity_extras_are_validated() {
        let mut c = Config { active_opacity: Some(3), idle_dim_seconds: 99999, idle_opacity: 0, ..Config::default() };
        validate(&mut c);
        assert_eq!((c.active_opacity, c.idle_dim_seconds, c.idle_opacity), (Some(MIN_OPACITY), 3600, MIN_OPACITY));
        let d = Config::default();
        assert_eq!((d.active_opacity, d.idle_dim_seconds, d.start_hidden, d.start_ghost), (None, 0, false, false));
    }

    #[test]
    fn navigation_keys_parse() {
        assert_eq!(parse_hotkey("Ctrl+Alt+PageDown"), Some((0x4000 | 0x2 | 0x1, 0x22)));
        assert_eq!(parse_hotkey("ctrl+alt+pgup"), Some((0x4000 | 0x2 | 0x1, 0x21)));
        assert_eq!(parse_hotkey("Ctrl+Alt+Shift+N"), Some((0x4000 | 0x2 | 0x1 | 0x4, b'N' as u32)));
        assert_eq!(parse_hotkey("Ctrl+Alt+`"), Some((0x4000 | 0x2 | 0x1, 0xC0)));
    }

    #[test]
    fn profiles_are_cleaned_and_defaulted() {
        let mut c = Config {
            profiles: vec![
                Profile { name: "  claude  ".into(), command: Some("claude\r\nrm -rf /".into()), color: Some("#12ab34".into()), ..Profile::default() },
                Profile { name: "CLAUDE".into(), ..Profile::default() },
                Profile { name: "".into(), ..Profile::default() },
                Profile { name: "x".into(), color: Some("red".into()), dir: Some("   ".into()), ..Profile::default() },
            ],
            default_profile: "nope".into(),
            tabs: "weird".into(),
            ..Config::default()
        };
        validate(&mut c);
        assert_eq!(c.profiles.len(), 2, "duplicate and empty names are dropped");
        assert_eq!(c.profiles[0].name, "claude");
        assert_eq!(c.profiles[0].command.as_deref(), Some("clauderm -rf /"), "control characters are removed, so a command can never smuggle a second line");
        assert_eq!(c.profiles[0].color.as_deref(), Some("#12ab34"));
        assert_eq!(c.profiles[1].color, None, "an invalid colour is dropped");
        assert_eq!(c.profiles[1].dir, None, "a blank directory is dropped");
        assert_eq!(c.default_profile, "", "an unknown default falls back");
        assert_eq!(c.tabs, "auto");
        let mut empty = Config { profiles: vec![], ..Config::default() };
        validate(&mut empty);
        assert_eq!(empty.profiles, vec![Profile::default()], "never an empty profile list");
        assert_eq!(default_profile(&c).name, "claude");
        c.default_profile = "X".into();
        assert_eq!(default_profile(&c).name, "x", "the default is matched without regard to case");
    }

    #[test]
    fn a_reload_sees_exactly_what_changed() {
        let a = Config::default();
        assert!(changed_keys(&a, &a.clone()).is_empty());
        let mut b = a.clone();
        b.window = Some(Geometry { x: 1, y: 2, w: 300, h: 200 });
        assert!(changed_keys(&a, &b).is_empty(), "the window position is written by HUDshell itself and ignored");
        b.theme = "matrix".into();
        b.text_scale = 150;
        b.hotkeys.quit = "Ctrl+Alt+F9".into();
        assert_eq!(changed_keys(&a, &b), vec!["hotkeys", "text_scale", "theme"]);
    }

    #[test]
    fn parsing_for_a_reload_validates_and_rejects_bad_json_without_side_effects() {
        let c = try_parse("\u{feff}{\"text_scale\": 5000, \"theme\": \"nope\"}").unwrap();
        assert_eq!((c.text_scale, c.theme.as_str()), (300, "midnight"), "values are clamped as at start");
        assert!(try_parse("{ not json").is_err());
        assert!(try_parse("").is_err());
    }

    #[test]
    fn text_settings_are_validated() {
        let d = Config::default();
        assert_eq!((d.text_scale, d.ui_scale, d.line_height, d.letter_spacing, d.text_scale_hotkeys.as_str()), (100, 100, 100, 0, "focused"),
            "defaults leave the text exactly as it was");
        let mut c = Config { text_scale: 5, ui_scale: 999, line_height: 10, letter_spacing: 99, text_scale_hotkeys: "x".into(), ..Config::default() };
        validate(&mut c);
        assert_eq!((c.text_scale, c.ui_scale, c.line_height, c.letter_spacing, c.text_scale_hotkeys.as_str()), (50, 300, 100, 10, "focused"));
        c.text_scale = 175;
        c.line_height = 130;
        c.text_scale_hotkeys = "all".into();
        validate(&mut c);
        assert_eq!((c.text_scale, c.line_height, c.text_scale_hotkeys.as_str()), (175, 130, "all"));
        assert_eq!(parse_hotkey(&Hotkeys::default().font_reset), Some((0x4000 | 0x2 | 0x1, b'0' as u32)));
    }

    #[test]
    fn layout_settings_are_validated() {
        let d = Config::default();
        assert_eq!((d.layout.as_str(), d.layout_fill.as_str(), d.remember_layout, d.split, d.pane_dividers, d.pane_min),
            ("single", "new", false, [50, 50], false, [30, 6]), "defaults are the recommended behaviour");
        assert_eq!(d.pane_shells, "shared");
        assert_eq!(d.maximize_area, "monitor");
        assert!(d.edge_tiling);
        assert_eq!(d.restore_session, "off", "restoring is opt-in");
        let mut r = Config { restore_session: "x".into(), ..Config::default() };
        validate(&mut r);
        assert_eq!(r.restore_session, "off");
        assert_eq!(d.tab_program, "known");
        let mut t = Config { tab_program: "x".into(), ..Config::default() };
        validate(&mut t);
        assert_eq!(t.tab_program, "known");
        t.tab_program = "all".into();
        validate(&mut t);
        assert_eq!(t.tab_program, "all");
        let mut c = Config { layout: " GRID ".into(), layout_fill: "x".into(), pane_shells: "x".into(), split: [1, 99], pane_min: [1, 999], ..Config::default() };
        validate(&mut c);
        assert_eq!((c.layout.as_str(), c.layout_fill.as_str(), c.split, c.pane_min), ("grid", "new", [20, 80], [10, 100]));
        assert_eq!(c.pane_shells, "shared", "an unknown value falls back");
        c.maximize_area = " WORK ".into();
        c.pane_shells = "own".into();
        validate(&mut c);
        assert_eq!(c.pane_shells, "own");
        assert_eq!(c.maximize_area, "work", "trimmed and lower-cased");
        c.maximize_area = "bogus".into();
        validate(&mut c);
        assert_eq!(c.maximize_area, "monitor", "an unknown value falls back");
        c.layout = "2x2".into();
        c.layout_fill = "empty".into();
        validate(&mut c);
        assert_eq!((c.layout.as_str(), c.layout_fill.as_str()), ("single", "empty"));
        assert_eq!(parse_hotkey("Ctrl+Alt+'"), Some((0x4000 | 0x2 | 0x1, 0xDE)));
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

    #[test]
    fn shift_alone_is_not_enough_for_a_global_hotkey() {
        assert_eq!(parse_hotkey("Shift+A"), None);
        assert_eq!(parse_hotkey("Shift+Space"), None);
        assert!(parse_hotkey("Win+A").is_some());
        assert!(parse_hotkey("Alt+Shift+A").is_some());
    }

    #[test]
    fn utf16_and_bom_files_are_read() {
        let json = r#"{"opacity": 70}"#;
        let mut le = vec![0xFF, 0xFE];
        le.extend(json.encode_utf16().flat_map(|u| u.to_le_bytes()));
        let mut be = vec![0xFE, 0xFF];
        be.extend(json.encode_utf16().flat_map(|u| u.to_be_bytes()));
        let mut u8bom = vec![0xEF, 0xBB, 0xBF];
        u8bom.extend(json.as_bytes());
        for b in [le, be, u8bom, json.as_bytes().to_vec()] {
            assert_eq!(decode_text(&b).unwrap(), json);
        }
        assert!(decode_text(&[0xC3, 0x28]).is_err(), "invalid UTF-8 is an error, not an empty file");
    }

    #[test]
    fn one_wrong_value_costs_only_that_value() {
        let (cfg, dropped) = parse_lenient(r#"{"opacity": 85.5, "font_size": 18, "window": {"x": 1}, "theme": "amber"}"#).unwrap();
        assert_eq!(dropped, vec!["opacity".to_string(), "window".to_string()]);
        assert_eq!(cfg.font_size, 18);
        assert_eq!(cfg.theme, "amber");
        assert_eq!(cfg.opacity, Config::default().opacity);
        assert!(cfg.window.is_none());
        assert!(parse_lenient("{not json").is_err());
        assert!(parse_lenient("[1, 2]").is_err());
        assert!(try_parse(r#"{"opacity": 85.5}"#).is_err(), "a reload refuses a file with a wrong value");
        assert!(try_parse(r#"{"opacity": 85}"#).is_ok());
    }

    #[test]
    fn save_replaces_only_the_version_it_knows() {
        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = t0 + std::time::Duration::from_secs(1);
        assert!(may_replace(None, None), "no file yet");
        assert!(may_replace(Some((t0, 10)), Some((t0, 10))), "unchanged since read");
        assert!(!may_replace(Some((t0, 10)), Some((t1, 10))), "edited by hand since");
        assert!(!may_replace(Some((t0, 10)), Some((t0, 11))), "same time, other length");
        assert!(!may_replace(None, Some((t0, 10))), "never read successfully (invalid or unreadable)");
    }

    #[test]
    fn durable_write_replaces_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("hudshell-cfgtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("config.json");
        write_durable(&p, "one").unwrap();
        write_durable(&p, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "no temp file left: {names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_coordinates_and_shell_jump_are_validated() {
        let mut cfg: Config = serde_json::from_str(r#"{"window": {"x": 2000000000, "y": -2000000000, "w": 0, "h": 99999}, "hotkeys": {"shell_jump": "Shift"}}"#).unwrap();
        validate(&mut cfg);
        let g = cfg.window.unwrap();
        assert_eq!((g.x, g.y, g.w, g.h), (COORD_LIMIT, -COORD_LIMIT, 240, 8000));
        assert_eq!(cfg.hotkeys.shell_jump, Hotkeys::default().shell_jump);
    }
}

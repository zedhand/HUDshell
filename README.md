# HUDshell

A compact, always-on-top terminal for Windows 11 that runs the real, unmodified **PowerShell 7**
(`pwsh.exe`), so command-line tools such as Claude Code and Codex CLI behave exactly as they do in
PowerShell. It is built to sit over other windows (for example a game) without getting in the way:
click-through by default, an explicit keyboard-focus toggle, and an adjustable opacity.

> Status: early (v0.1). Windows 11, US keyboard layout, PowerShell 7. See [docs/known-gaps.md](docs/known-gaps.md).

## Features

- Hosts PowerShell 7 through ConPTY; no emulation of PowerShell itself.
- Floats above other windows (topmost, no taskbar button) and does **not** take keyboard focus unless asked.
- **Click-through** by default (mouse input goes to the window underneath). Toggle with a hotkey.
- **Grab / release keyboard focus** with a hotkey, or click the terminal while it is interactive.
- **Opacity** from 20% to 100% (hotkeys, settings panel, or `config.json`).
- Colour themes (midnight, graphite, matrix, amber, dracula, solarized-dark, paper, or your own).
- Frameless, rounded, draggable and resizable; remembers its position and size (multi-monitor aware).
- Tray icon with a menu; notices (for example a hotkey conflict) appear as a tray notification.
- Small: the executable is about 1 MB. The terminal UI (xterm.js) is compiled into it.

## Requirements

- Windows 11 (built and tested on build 26300).
- PowerShell 7 (`pwsh`). Found via `PATH`, the Store alias, Program Files, or a path you set in `config.json`.
- Microsoft Edge WebView2 Runtime (included with Windows 11).
- To build: Rust (MSVC toolchain, see `rust-toolchain.toml`) and Node.js (only to copy the pinned xterm.js files).

## Build

```powershell
# one step: installs web assets, embeds them, builds a release exe with machine paths stripped
scripts\build-release.ps1
# result: target\release\hudshell.exe
```

Or by hand: `npm install`, `npm run build`, `cargo build --release`.

## Hotkeys (global, configurable)

| Keys | Action |
|---|---|
| Ctrl+Alt+T | toggle click-through |
| Ctrl+Alt+G | grab / release keyboard focus |
| Ctrl+Alt+V | show / hide |
| Ctrl+Alt+C | next colour theme |
| Ctrl+Alt+[ / Ctrl+Alt+] | opacity down / up (10% steps) |
| Ctrl+Alt+S | settings panel (arrows change opacity and colours, Esc closes) |
| Ctrl+Alt+Q | quit |

Inside the terminal: **Shift+Enter** inserts a newline in Claude Code and Codex; **Ctrl+Shift+C** or
**Ctrl+Insert** copies the selection; **Ctrl+V** pastes. Plain Ctrl+C is the interrupt key.

## Configuration

`%APPDATA%\HUDshell\config.json` is created on first run. Missing keys take defaults, unknown keys are ignored,
values are range-checked, and an unreadable file is kept as `config.json.bad-N` while defaults are used.

| Key | Default | Notes |
|---|---|---|
| `click_through` | `true` | start in click-through mode |
| `opacity` | `100` | percent, 20-100 |
| `theme` | `"midnight"` | preset name or `"custom"` |
| `custom_colours` | none | `{background, foreground, cursor, selection}` as `#rrggbb` |
| `font_family` / `font_size` | Cascadia Mono, Consolas / 14 | size 8-48 |
| `scrollback` | `2000` | lines, 100-100000 |
| `shell` | none | full path of a shell to use instead of PowerShell 7 |
| `hotkeys` | see table above | `click_through`, `grab_focus`, `show_hide`, `cycle_colours`, `opacity_up`, `opacity_down`, `settings`, `quit`; every chord needs at least one modifier |
| `window` | remembered | position and size in physical pixels |

Logs: `%LOCALAPPDATA%\HUDshell\hudshell.log` (rotated at 1 MB). It contains no terminal content and no window titles.

## Tests

`scripts\test-all.ps1` runs unit tests, release-binary checks and scripted end-to-end scenarios.
See [docs/testing.md](docs/testing.md). Design notes are in [docs/design.md](docs/design.md) and measured
results in [docs/test-results.md](docs/test-results.md).

## Third-party components

[xterm.js](https://github.com/xtermjs/xterm.js) 6.0.0 and its fit / Unicode addons (MIT), the Rust crates in
`Cargo.lock`, and the Microsoft Edge WebView2 Runtime.

## Licence

[MIT](LICENSE): use it however you like.

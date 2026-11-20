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
- **Several shells in one window** (up to 9): visible tabs, a compact overlay-only list, switch hotkeys that work while the game has the keyboard, tab names taken from each program's own window title (like a normal terminal tab), a label when a tab runs Claude Code, Codex or a similar tool, a mark on tabs with unseen output, and profiles (start folder, start command, accent colour).
- **Split panes**: show two shells side by side or stacked, or four in a 2 x 2 grid, in the same window; a hotkey moves the keyboard between panes and another zooms one pane to the whole window. A window too small for the layout shows fewer panes. Equal halves by default; uneven splits, draggable dividers and a separate list of shells per pane are options.
- **Corners and sizes**: a hotkey docks the window in the next screen corner, another cycles size presets, and dragging near a screen edge snaps to it.
- **Opacity that follows you**: a separate opacity while you are typing, and an optional dim after the shell has been quiet for a while.
- **Start options**: start hidden or ghosted; a second launch brings the running window forward; "Start with Windows" in the tray menu.
- **Ghost** preset: one key makes the window click-through and dims it (40% by default); press again to restore.
- **Attention cue**: a program that rings the bell or sends an OSC 9 / OSC 777 notification while you are in the game makes the window border pulse (a tray notification when hidden).
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
| Ctrl+Alt+X | ghost: click-through plus dim, press again to restore |
| Ctrl+Alt+. / Ctrl+Alt+, | dock in the next / previous screen corner |
| Ctrl+Alt+/ | cycle size presets |
| Ctrl+Alt+= / Ctrl+Alt+- | bigger / smaller font |
| Ctrl+Alt+N | new shell (default profile) |
| Ctrl+Alt+Shift+N | profile picker (takes the keyboard; press a number, Esc cancels) |
| Ctrl+Alt+PageDown / PageUp | next / previous shell |
| Ctrl+Alt+1 ... 9 | jump to shell 1-9 |
| Ctrl+Alt+` | show the shell list for a moment |
| Ctrl+Alt+; | jump to the next shell with unseen output |
| Ctrl+Alt+W | close the active shell (press twice within 3 s; closing the last shell starts a fresh one) |
| Ctrl+Alt+' | next layout: single, side by side, stacked, 2 x 2 |
| Ctrl+Alt+O | move the keyboard to the next pane |
| Ctrl+Alt+Z | zoom the focused pane to the whole window, press again to go back |
| Ctrl+Alt+Q | quit |

Inside the terminal: **Shift+Enter** inserts a newline in Claude Code and Codex; **Ctrl+Shift+C** or
**Ctrl+Insert** copies the selection; **Ctrl+V** pastes. Plain Ctrl+C is the interrupt key.
Scrollback from the keyboard (normal screen; useful while click-through blocks the mouse wheel): **Shift+PageUp / Shift+PageDown**
scroll a page, **Ctrl+Shift+Up / Down** a line, **Ctrl+Shift+Home / End** jump to the top or bottom.

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
| `hotkeys` | see table above | `click_through`, `grab_focus`, `show_hide`, `cycle_colours`, `opacity_up`, `opacity_down`, `settings`, `ghost`, `quit`, `corner_next`, `corner_prev`, `size_cycle`, `font_bigger`, `font_smaller`, `shell_next`, `shell_prev`, `shell_new`, `shell_close`, `shell_list`, `shell_picker`, `shell_attention`, `layout_cycle`, `pane_zoom`, `pane_next`, `shell_jump` (modifiers only; `Ctrl+Alt` means Ctrl+Alt+1..9); every chord needs at least one modifier |
| `ghost_opacity` | `40` | percent used by the ghost preset (20-100); never brightens a window that is already dimmer; never saved over `opacity` |
| `copy_on_select` | `false` | copy the selection when the mouse button is released |
| `right_click` | `"off"` | `"paste"`: right-click copies a selection, or pastes when nothing is selected |
| `osc52` | `"off"` | `"write"`: let programs set the clipboard (OSC 52). Reads are never answered |
| `snap_distance` / `snap_margin` | `16` / `8` | pixels (96-dpi): dragging this close to a screen edge snaps to it (0 = off); gap kept from the edge |
| `sizes` | `[[640,360],[900,520],[1280,720]]` | size presets (96-dpi pixels) for the size hotkey |
| `active_opacity` | `null` | opacity while HUDshell holds the keyboard (`null` = same as `opacity`) |
| `idle_dim_seconds` / `idle_opacity` | `0` / `50` | dim to `idle_opacity` after this many seconds without shell output (0 = off); output brightens it at once |
| `start_hidden` / `start_ghost` | `false` / `false` | start hidden / in ghost mode |
| `tab_program` | `"known"` | label a tab with the program running in it (`claude · <title>`): `"known"` = Claude Code, Codex, Gemini CLI, Aider, OpenCode; `"all"` = also any other program (`git`, `ssh`, ...); `"off"` = never. Found by reading the process list below each shell: nothing is injected and nothing is written to the log |
| `tabs` | `"auto"` | `"auto"`: tab strip once there are 2+ shells; `"always"`: strip even for one; `"overlay"`: no strip, only a short overlay on switch and on the list hotkey |
| `profiles` / `default_profile` | one `pwsh` profile / `""` | kinds of shell: `{name, dir, command, shell, color}`. `dir` expands `%VARS%`; `command` is typed into the new shell; `color` is a tab accent `#rrggbb` |
| `layout` | `"single"` | layout at start: `"single"`, `"side-by-side"`, `"stacked"` or `"grid"` (2 x 2) |
| `pane_shells` | `"shared"` | `"shared"`: every pane shows a shell from the one tab list; `"own"`: each pane has its own list of shells (the tab strip, PageUp/PageDown and the number keys then act on the focused pane's list; a new shell joins the focused pane; going to fewer panes moves the shells into the focused pane) |
| `layout_fill` | `"new"` | when a layout has more panes than there are shells: `"new"` starts shells (default profile) for them, `"empty"` leaves the panes empty until you start a shell there |
| `remember_layout` | `false` | save the chosen layout and divider positions in `layout` / `split` whenever they change (which shell is in which pane is not saved) |
| `split` | `[50, 50]` | share in percent (20-80) of the first column and of the first row |
| `pane_dividers` | `false` | draggable dividers between panes (needs the mouse, so the window must not be click-through) |
| `pane_min` | `[30, 6]` | smallest pane worth drawing, in columns and rows; a smaller window shows fewer panes |
| `attention_cue` / `attention_toast` | `true` / `true` | pulse the border, and show a tray notification while hidden, when a program rings the bell or sends OSC 9 / OSC 777 |
| `window` | remembered | position and size in physical pixels |

OSC 8 hyperlinks are drawn but clicking them does nothing (copy the text instead); HUDshell denies every browser permission request.

Logs: `%LOCALAPPDATA%\HUDshell\hudshell.log` (rotated at 1 MB). It contains no terminal content and no window titles.

## Tests

`scripts\test-all.ps1` runs unit tests, release-binary checks and scripted end-to-end scenarios.
See [docs/testing.md](docs/testing.md). Design notes are in [docs/design.md](docs/design.md) and measured
results in [docs/test-results.md](docs/test-results.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Third-party components

[xterm.js](https://github.com/xtermjs/xterm.js) 6.0.0 and its fit / Unicode addons (MIT), the Rust crates in
`Cargo.lock`, and the Microsoft Edge WebView2 Runtime.

## Licence

[MIT](LICENSE): use it however you like.

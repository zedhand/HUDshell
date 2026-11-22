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
- **Tabs by hand**: double-click a tab to give it a name (Enter keeps it, Esc cancels, an empty name goes back to the automatic one); drag a tab sideways to move it in the strip. Names and order are part of the saved session.
- **Several windows** (up to 4): move a shell into a window of its own (Shift+click on its tab, or a hotkey); the shell keeps running and takes its screen and scrollback along. Each window has its own shells (up to 9), layout, opacity and pointer mode. The hotkeys act on the window that was last active (a hotkey switches between windows), the tray menu has "all windows" items (ghost, show / hide), windows snap flush against each other and touching windows move together (hold Shift while dragging to move one alone), closing the last shell of a window closes it, and `restore_session` brings all windows back where they were.
- **Split panes**: show two shells side by side or stacked, or four in a 2 x 2 grid, in the same window; a hotkey moves the keyboard between panes and another zooms one pane to the whole window. A window too small for the layout shows fewer panes. Equal halves by default; uneven splits, draggable dividers and a separate list of shells per pane are options.
- **Text size**: each terminal keeps its own text size (bigger / smaller / reset hotkeys act on the focused one), plus a global percent scale, a scale for HUDshell's own text (tabs, pane names, overlays), line height and letter spacing; all optional settings, all off at their defaults.
- **Moving**: drag the slim strip along the top edge (resize from any edge or corner); the window snaps magnetically to screen edges, **dropping it near a corner pulls it flush into the corner (corners square, no gap)**, **drag it to the top edge of a screen to maximize it** (over the taskbar by default; `maximize_area`), drag the strip of a maximized window to restore it, or double-click the strip to maximize / restore.
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
| Ctrl+Alt+S | settings panel: left/right opacity, up/down colours, `-` / `=` text size, `,` / `.` size of HUDshell's own text, `[` / `]` ghost opacity, `t` tab style, `;` / `'` line height, `n` / `m` letter spacing (or click the `[-]` `[+]` and `<` `>` buttons; Esc closes). Every change is live, saved, and applied to every window |
| Ctrl+Alt+X | ghost: click-through plus dim, press again to restore |
| Ctrl+Alt+. / Ctrl+Alt+Shift+. | dock in the next / previous screen corner |
| Ctrl+Alt+/ | cycle size presets |
| Ctrl+Alt+= / Ctrl+Alt+- | bigger / smaller text in the focused terminal (`text_scale_hotkeys = "all"`: in every terminal) |
| Ctrl+Alt+0 | text size back to the starting size (`font_size`) |
| Ctrl+Alt+N | new shell (default profile) |
| Ctrl+Alt+Shift+N | profile picker (takes the keyboard; press a number, Esc cancels) |
| Ctrl+Alt+PageDown / PageUp | next / previous shell |
| Ctrl+Alt+1 ... 9 | jump to shell 1-9 |

A program in front that binds the same chord reacts too, because a registered hotkey does not stop a program from also acting on it. **Windows Terminal** binds Ctrl+Alt+1 to 9 (switch to tab 1-9) and Ctrl+Alt+, (open its defaults file); the previous-corner hotkey is Ctrl+Alt+Shift+. for that reason. If Windows Terminal is in front while you press Ctrl+Alt+1 to 9, HUDshell and the terminal both switch; change those hotkeys in `config.json` if that bothers you.

| Ctrl+Alt+` | show the shell list for a moment |
| Ctrl+Alt+; | jump to the next shell with unseen output |
| Ctrl+Alt+W | close the active shell (press twice within 3 s; closing the last shell starts a fresh one) |
| Ctrl+Alt+' | next layout: single, side by side, stacked, 2 x 2 |
| Ctrl+Alt+O | move the keyboard to the next pane |
| Ctrl+Alt+Z | zoom the focused pane to the whole window, press again to go back |
| Ctrl+Alt+D | move the active shell into a window of its own (Shift+click on a tab does the same for that tab) |
| Ctrl+Alt+Shift+W | make the next HUDshell window the one the hotkeys act on |
| Ctrl+Alt+Shift+D | move the active shell into the next window (a window's only shell takes its window with it) |
| Ctrl+Alt+Shift+X | ghost mode for every window at once; again to leave it |
| Ctrl+Alt+Shift+V | hide every window, or show them all |
| Ctrl+Alt+Q | quit |

On a keyboard layout with an **AltGr** key (right Alt, which Windows treats as Ctrl+Alt), a character typed with AltGr that sits on the same key as one of these hotkeys is taken by HUDshell instead (on a German layout AltGr+Q, `@`, is the quit hotkey). HUDshell warns you at start (a tray notification) which of its hotkeys clash on your current layout. Change those hotkeys in `config.json` if you need the characters.

Inside the terminal: **Shift+Enter** inserts a newline in Claude Code and Codex; **Ctrl+Shift+C** or
**Ctrl+Insert** copies the selection; **Ctrl+V** pastes. Plain Ctrl+C is the interrupt key.
Scrollback from the keyboard (normal screen; useful while click-through blocks the mouse wheel): **Shift+PageUp / Shift+PageDown**
scroll a page, **Ctrl+Shift+Up / Down** a line, **Ctrl+Shift+Home / End** jump to the top or bottom.

## Configuration

`%APPDATA%\HUDshell\config.json` is created on first run. Missing keys take defaults, unknown keys are ignored,
values are range-checked, and an unreadable file is kept as `config.json.bad-N` while defaults are used.

**Edits apply while HUDshell runs** (it checks the file every 1.5 seconds; the tray menu has "Reload settings" to do it at once): opacity, colours, hotkeys, tabs, profiles, layout options, text sizes, line and letter spacing, font, scrollback, clipboard options and the other pointer and notification settings. A few are read only at start and a notice names them if you change them: `shell`, `unicode`, `shift_enter`, `pane_shells`, `layout`, `start_hidden`, `start_ghost`. If the file is not valid JSON when you save it, the settings in use are kept and a notice says so.

| Key | Default | Notes |
|---|---|---|
| `click_through` | `true` | start in click-through mode |
| `opacity` | `100` | percent, 20-100 |
| `theme` | `"midnight"` | preset name or `"custom"` |
| `custom_colours` | none | `{background, foreground, cursor, selection}` as `#rrggbb` |
| `font_family` / `font_size` | Cascadia Mono, Consolas / 14 | size 8-48; the starting size of every terminal |
| `text_scale` | `100` | percent, 50-300, multiplies every terminal's size (150 = half as big again) |
| `text_scale_hotkeys` | `"focused"` | `"focused"`: the bigger / smaller / reset hotkeys change only the terminal that has the keyboard (each terminal keeps its own size until HUDshell quits); `"all"`: every terminal together, and `font_size` is saved |
| `line_height` / `letter_spacing` | `100` / `0` | line height in percent of the font's (100-200); extra pixels between characters (0-10); both apply to every terminal |
| `ui_scale` | `100` | percent, 50-300, for HUDshell's own text: tab names, pane names, overlays, the picker and the settings panel |
| `scrollback` | `2000` | lines, 100-100000 |
| `shell` | none | full path of a shell to use instead of PowerShell 7 |
| `hotkeys` | see table above | `click_through`, `grab_focus`, `show_hide`, `cycle_colours`, `opacity_up`, `opacity_down`, `settings`, `ghost`, `quit`, `corner_next`, `corner_prev`, `size_cycle`, `font_bigger`, `font_smaller`, `font_reset`, `shell_next`, `shell_prev`, `shell_new`, `shell_close`, `shell_list`, `shell_picker`, `shell_attention`, `layout_cycle`, `pane_zoom`, `pane_next`, `window_detach`, `window_next`, `shell_move`, `ghost_all`, `show_hide_all`, `shell_jump` (modifiers only; `Ctrl+Alt` means Ctrl+Alt+1..9); every chord needs at least one modifier |
| `ghost_opacity` | `40` | percent used by the ghost preset (20-100); never brightens a window that is already dimmer; never saved over `opacity` |
| `copy_on_select` | `false` | copy the selection when the mouse button is released |
| `right_click` | `"off"` | `"paste"`: right-click copies a selection, or pastes when nothing is selected |
| `osc52` | `"off"` | `"write"`: let programs set the clipboard (OSC 52). Reads are never answered |
| `snap_distance` / `snap_margin` | `16` / `8` | pixels (96-dpi): dragging this close to a screen edge snaps to it (0 = off); gap kept from the edge. Within 3 x `snap_distance` + `snap_margin` of a corner the window goes flush into the corner instead (no gap) |
| `game_positions` | `false` | opt in: HUDshell remembers where the first window was put while a given program was in front, and puts it back there when that program comes to the front again (looks only at the program's executable name, never window titles; the names are kept in `game_places` in this file and never logged) |
| `exclusive_guard` | `true` | a game in exclusive fullscreen cannot show HUDshell, and taking the keyboard from it (Ctrl+Alt+G) minimizes it. With this on, HUDshell does not take the keyboard from a program listed in `exclusive_programs`, and it learns a program when a grab minimizes a full-screen program (it gives the keyboard back, brings the program back and adds it to the list). A tray notice says why. Use the game's borderless or windowed mode to use HUDshell over it |
| `exclusive_programs` | `[]` | executable file names (lower case, for example `game.exe`) of programs that run in exclusive fullscreen; edit by hand, HUDshell adds the ones it learns; names are never logged |
| `game_places` | `[]` | the places `game_positions` keeps, written by HUDshell: `{ "app": "game.exe", "rect": [x, y, width, height] }` |
| `edge_tiling` | `true` | dropping the window with the cursor on the left or right edge of a screen makes it fill that half (drag it away to restore its size); the top edge maximizes |
| `maximize_area` | `monitor` | what maximizing fills: `monitor` = the whole screen, over the taskbar; `work` = the usable area, taskbar left visible |
| `sizes` | `[[640,360],[900,520],[1280,720]]` | size presets (96-dpi pixels) for the size hotkey |
| `active_opacity` | `null` | opacity while HUDshell holds the keyboard (`null` = same as `opacity`) |
| `idle_dim_seconds` / `idle_opacity` | `0` / `50` | dim to `idle_opacity` after this many seconds without shell output (0 = off); output brightens it at once |
| `start_hidden` / `start_ghost` | `false` / `false` | start hidden / in ghost mode |
| `tab_program` | `"known"` | label a tab with the program running in it (`claude · <title>`): `"known"` = Claude Code, Codex, Gemini CLI, Aider, OpenCode; `"all"` = also any other program (`git`, `ssh`, ...); `"off"` = never. Found by reading the process list below each shell: nothing is injected and nothing is written to the log |
| `tabs` | `"auto"` | `"auto"`: tab strip once there are 2+ shells; `"always"`: strip even for one; `"overlay"`: no strip, only a short overlay on switch and on the list hotkey |
| `profiles` / `default_profile` | one `pwsh` profile / `""` | kinds of shell: `{name, dir, command, shell, color}`. `dir` expands `%VARS%`; `command` is typed into the new shell; `color` is a tab accent `#rrggbb` |
| `restore_session` | `"off"` | `"shells"`: the open shells (by profile) and the layout are saved while HUDshell runs (`session.json` next to `config.json`) and started again at the next start; folders and running programs are not restored, and a saved layout wins over `layout` |
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

[xterm.js](https://github.com/xtermjs/xterm.js) 6.0.0 and its fit / serialize / Unicode addons (MIT), the Rust crates in
`Cargo.lock`, and the Microsoft Edge WebView2 Runtime.

## Licence

[MIT](LICENSE): use it however you like.

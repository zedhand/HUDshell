// HUDshell terminal front end: xterm.js <-> Rust host over a byte-safe JSON/base64 bridge.
(function () {
  'use strict';
  const wv = window.chrome.webview;
  const send = (o) => wv.postMessage(JSON.stringify(o));
  const enc = new TextEncoder();

  function b2a(u8) {
    let s = '';
    const CH = 0x8000;
    for (let i = 0; i < u8.length; i += CH) s += String.fromCharCode.apply(null, u8.subarray(i, i + CH));
    return btoa(s);
  }
  function a2b(b64) {
    const s = atob(b64);
    const u = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) u[i] = s.charCodeAt(i);
    return u;
  }

  const injected = window.__hudshell || {};
  const startTheme = injected.theme || { background: '#0c1620', foreground: '#d7e2ec' };
  function applyPageColours(t) {
    const root = document.documentElement.style;
    root.setProperty('--bg', t.background);
    root.setProperty('--fg', t.foreground);
  }
  applyPageColours(startTheme);
  // Terminal policies (config.json): all clipboard conveniences are opt-in.
  const copyOnSelect = injected.copyOnSelect === true;
  const rightClick = injected.rightClick === 'paste' ? 'paste' : 'off';
  const osc52Mode = injected.osc52 === 'write' ? 'write' : 'off';
  const term = new Terminal({
    // OSC 8 hyperlinks are drawn but never opened: xterm.js' default handler would show a confirm()
    // dialog and open a window, which has no place over a game. Copy the link text instead.
    linkHandler: { allowNonHttpProtocols: false, activate: () => noteOnce('link-click-ignored'), hover: () => {}, leave: () => {} },
    allowProposedApi: true,
    cursorBlink: false,
    scrollback: injected.scrollback || 2000,
    fontFamily: injected.font || '"Cascadia Mono", Consolas, monospace',
    fontSize: injected.fontSize || 14,
    windowsPty: { backend: 'conpty', buildNumber: injected.build || 26100 },
    theme: startTheme,
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);

  // Experiment switch: which Unicode width rules xterm.js uses ('default' | '11' | 'graphemes').
  const uni = injected.unicode || 'default';
  try {
    if (uni === '11') {
      term.loadAddon(new Unicode11Addon.Unicode11Addon());
      term.unicode.activeVersion = '11';
    } else if (uni === 'graphemes') {
      term.loadAddon(new UnicodeGraphemesAddon.UnicodeGraphemesAddon());
      term.unicode.activeVersion = '15-graphemes';
    }
  } catch (e) {
    send({ t: 'jserror', where: 'unicode-addon', msg: String(e) });
  }

  // Shift+Enter: xterm.js has no kitty keyboard protocol, so Shift+Enter would arrive as plain
  // Enter and submit the prompt. 'esc-cr' sends ESC CR (what Claude Code's own terminal setup
  // uses for Shift+Enter); 'none' leaves xterm.js behaviour untouched.
  const shiftEnter = injected.shiftEnter || 'none';

  // Minimal kitty keyboard protocol negotiation (query / push / pop / set). Programs that want
  // distinguishable modified keys ask with CSI ? u and enable with CSI > flags u. We answer the
  // query and remember the flags; only then is Shift+Enter reported as CSI 13;2 u. Plain
  // programs (PowerShell's prompt) never negotiate it and keep seeing a normal Enter.
  const kitty = { flags: 0, stack: [] };
  const reply = (s) => send({ t: 'in', d: b2a(enc.encode(s)) });
  const num = (v) => (Array.isArray(v) ? Number(v[0]) : Number(v)) || 0;
  // XTVERSION (CSI > q): identify honestly. Claude Code and Codex both ask for this.
  term.parser.registerCsiHandler({ prefix: '>', final: 'q' }, () => {
    reply('\x1bP>|HUDshell 0.1.0\x1b\\');
    return true;
  });
  term.parser.registerCsiHandler({ prefix: '?', final: 'u' }, () => {
    reply('\x1b[?' + kitty.flags + 'u');
    return true;
  });
  term.parser.registerCsiHandler({ prefix: '>', final: 'u' }, (p) => {
    kitty.stack.push(kitty.flags);
    if (kitty.stack.length > 32) kitty.stack.shift();
    kitty.flags = num(p[0]);
    return true;
  });
  term.parser.registerCsiHandler({ prefix: '<', final: 'u' }, (p) => {
    let n = p.length ? num(p[0]) || 1 : 1;
    while (n-- > 0 && kitty.stack.length) kitty.flags = kitty.stack.pop();
    if (!kitty.stack.length && n >= 0) kitty.flags = 0;
    return true;
  });
  term.parser.registerCsiHandler({ prefix: '=', final: 'u' }, (p) => {
    const f = num(p[0]);
    const mode = (p.length > 1 ? num(p[1]) : 1) || 1;
    if (mode === 1) kitty.flags = f;
    else if (mode === 2) kitty.flags |= f;
    else if (mode === 3) kitty.flags &= ~f;
    return true;
  });

  // Tell the host something happened, at most once a minute per kind, never with terminal content.
  const noted = {};
  function noteOnce(what) {
    const now = Date.now();
    if (noted[what] && now - noted[what] < 60000) return;
    noted[what] = now;
    send({ t: 'note', what });
  }
  function copySelection() {
    const text = term.getSelection();
    return navigator.clipboard.writeText(text).then(
      () => send({ t: 'copied', chars: text.length }),
      (e) => send({ t: 'copied', error: String(e) }));
  }

  term.attachCustomKeyEventHandler((ev) => {
    // Keyboard scrollback (normal screen only; full-screen programs keep these keys):
    // Shift+PageUp/PageDown = a page, Ctrl+Shift+Up/Down = a line, Ctrl+Shift+Home/End = top / bottom.
    // Useful while click-through is on, because the mouse wheel then never reaches the terminal.
    if (term.buffer.active.type === 'normal' && ev.shiftKey && !ev.altKey && !ev.metaKey) {
      let scroll = null;
      if (!ev.ctrlKey && ev.code === 'PageUp') scroll = () => term.scrollPages(-1);
      else if (!ev.ctrlKey && ev.code === 'PageDown') scroll = () => term.scrollPages(1);
      else if (ev.ctrlKey && ev.code === 'ArrowUp') scroll = () => term.scrollLines(-1);
      else if (ev.ctrlKey && ev.code === 'ArrowDown') scroll = () => term.scrollLines(1);
      else if (ev.ctrlKey && ev.code === 'Home') scroll = () => term.scrollToTop();
      else if (ev.ctrlKey && ev.code === 'End') scroll = () => term.scrollToBottom();
      if (scroll) {
        if (ev.type === 'keydown') scroll();
        return false;
      }
    }
    // Copy: Ctrl+Shift+C or Ctrl+Insert copies the selection (plain Ctrl+C stays the interrupt key).
    if (ev.type === 'keydown' && term.hasSelection() &&
        ((ev.ctrlKey && ev.shiftKey && ev.code === 'KeyC') || (ev.ctrlKey && ev.code === 'Insert'))) {
      copySelection();
      return false;
    }
    // 'kitty' mode: if the running program negotiated the kitty keyboard protocol, report
    // Shift+Enter as CSI 13;2 u. Otherwise send ESC CR (Alt+Enter). Tested live: Claude Code
    // and Codex both insert a newline for ESC CR (Codex ignores a bare LF / Ctrl+J), and
    // PowerShell's PSReadLine ignores it, so Shift+Enter can never submit a prompt by accident.
    if (shiftEnter === 'kitty' && ev.key === 'Enter' && ev.shiftKey &&
        !ev.ctrlKey && !ev.altKey && !ev.metaKey) {
      if (ev.type === 'keydown') reply(kitty.flags !== 0 ? '\x1b[13;2u' : '\x1b\r');
      return false;
    }
    // The browser fires keydown AND keypress for Enter; xterm.js routes both through this handler,
    // so send ESC CR once (on keydown) and swallow the keypress too, or a stray CR follows it.
    if (shiftEnter === 'esc-cr' && ev.key === 'Enter' &&
        ev.shiftKey && !ev.ctrlKey && !ev.altKey && !ev.metaKey) {
      if (ev.type === 'keydown') send({ t: 'in', d: b2a(enc.encode('\x1b\r')) });
      return false;
    }
    return true;
  });

  const termEl = document.getElementById('term');
  term.open(termEl);
  fit.fit();

  // Mouse policies. Copy on select copies when the button is released; right-click (opt-in) copies a
  // selection and clears it, or pastes when nothing is selected (the host reads the clipboard).
  termEl.addEventListener('contextmenu', (e) => e.preventDefault());
  termEl.addEventListener('mouseup', (e) => {
    if (e.button === 0 && copyOnSelect && term.hasSelection()) copySelection();
    else if (e.button === 2 && rightClick === 'paste') {
      if (term.hasSelection()) { copySelection(); term.clearSelection(); }
      else send({ t: 'rightpaste' });
    }
  });

  // Programs ask for attention with the bell, OSC 9 (iTerm2 style) or OSC 777;notify (rxvt style).
  // The host decides whether to pulse the window or show a tray notification; it never logs the text.
  const attention = (kind, text) => send({ t: 'attention', kind, text: String(text || '').slice(0, 200) });
  term.onBell(() => attention('bell', ''));
  term.parser.registerOscHandler(9, (data) => {
    // OSC 9;<n>;... with a number first is ConEmu's progress / working-directory protocol (PowerShell
    // and winget use it), not a notification.
    if (!/^\d+(;|$)/.test(data)) attention('osc9', data);
    return true;
  });
  term.parser.registerOscHandler(777, (data) => {
    const p = data.split(';');
    if (p[0] === 'notify') attention('osc777', [p[1], p.slice(2).join(';')].filter(Boolean).join(': '));
    return true;
  });
  // OSC 52 (a program sets the clipboard): refused unless config osc52 = "write". Reads ("?") are never
  // answered, so a program can never learn what is on the clipboard.
  term.parser.registerOscHandler(52, (data) => {
    const i = data.indexOf(';');
    const payload = i < 0 ? '' : data.slice(i + 1);
    if (osc52Mode !== 'write') { noteOnce('osc52-refused'); return true; }
    if (!payload || payload === '?' || payload.length > 150000) return true;
    // The host sets the clipboard: the page-side Clipboard API needs keyboard focus, which a passive
    // overlay does not have when a program asks.
    send({ t: 'osc52', b64: payload });
    return true;
  });
  let cueTimer;
  function pulseCue() {
    document.body.classList.remove('cue');
    void document.body.offsetWidth; // restart the animation
    document.body.classList.add('cue');
    clearTimeout(cueTimer);
    cueTimer = setTimeout(() => document.body.classList.remove('cue'), 3000);
  }

  // Input: onData is text (UTF-8 encode once); onBinary is a binary-coded string (keep byte values).
  term.onData((d) => send({ t: 'in', d: b2a(enc.encode(d)) }));
  term.onBinary((s) => {
    const u = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) u[i] = s.charCodeAt(i) & 0xff;
    send({ t: 'in', d: b2a(u) });
  });

  // Resize: debounce the fit, only tell the host when the cell size really changed.
  let lastSize = '';
  term.onResize(({ cols, rows }) => {
    const k = cols + 'x' + rows;
    if (k !== lastSize) { lastSize = k; send({ t: 'resize', cols, rows }); }
  });
  let rt;
  new ResizeObserver(() => { clearTimeout(rt); rt = setTimeout(() => fit.fit(), 40); }).observe(document.body);

  wv.addEventListener('message', (ev) => {
    let m;
    try { m = JSON.parse(ev.data); } catch (e) { return; }
    switch (m.t) {
      case 'out': {
        const u = a2b(m.d);
        term.write(u, () => send({ t: 'ack', n: u.length }));
        break;
      }
      case 'exit':
        term.write('\r\n[session ended - press Ctrl+Alt+Q to quit HUDshell]\r\n');
        break;
      case 'focus':
        term.focus();
        break;
      case 'cue':
        pulseCue();
        break;
      case 'paste':
        term.paste(m.text);
        break;
      case 'selectall':
        term.selectAll();
        break;
      case 'theme':
        // Live colour change from the host (hotkey / tray menu / settings).
        term.options.theme = m.theme;
        applyPageColours(m.theme);
        S.theme = m.name; render();
        break;
      case 'settings':
        showSettings(m);
        break;
      case 'dump': {
        const b = term.buffer.active;
        const lines = [];
        for (let i = 0; i < term.rows; i++) {
          const l = b.getLine(b.viewportY + i);
          lines.push(l ? l.translateToString(true) : '');
        }
        send({ t: 'dump', cols: term.cols, rows: term.rows, buf: b.type, modes: term.modes,
               cursor: [b.cursorX, b.cursorY], unicode: uni, view: [b.viewportY, b.baseY],
               cue: document.body.classList.contains('cue'), lines });
        break;
      }
    }
  });

  // ---- settings panel (text-mode box) ----------------------------------------------------------
  const panel = document.getElementById('settings');
  const sset = (o) => send(Object.assign({ t: 'settings' }, o));
  const S = { opacity: 100, min: 20, theme: '', themes: [] };
  const BAR = 20;
  const esc = (s) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
  function render() {
    const W = 36;
    const filled = Math.round(((S.opacity - S.min) / (100 - S.min)) * BAR);
    let bar = '';
    for (let i = 0; i < BAR; i++) bar += '<span class="c" data-i="' + i + '">' + (i < filled ? '#' : '-') + '</span>';
    let out = 'hudshell settings\n' + '─'.repeat(W) + '\n';
    out += 'opacity  [' + bar + '] ' + String(S.opacity).padStart(3) + '%\n';
    out += 'colours  <span class="c" data-d="-1">&lt;</span> <span class="on">' + esc(S.theme.padEnd(14)) + '</span> <span class="c" data-d="1">&gt;</span>\n';
    out += '\n<span class="k">left/right  opacity\nup/down     colours</span>\n';    out += '<span class="k">esc</span> or <span class="c" id="sx">[close]</span>';
    panel.innerHTML = out;
  }  function closePanel() {
    if (!panel.classList.contains('open')) return;
    panel.classList.remove('open');
    sset({ a: 'close' });
    term.focus();
  }
  function showSettings(m) {
    S.opacity = m.opacity; S.min = m.minOpacity; S.theme = m.theme; S.themes = m.themes;
    render();
    if (m.open) panel.classList.add('open');
  }
  function setOpacity(v, save) {
    S.opacity = Math.max(S.min, Math.min(100, Math.round(v)));
    render();
    sset({ a: 'opacity', v: S.opacity, save });
  }
  function stepTheme(d) { const i = S.themes.indexOf(S.theme); setTheme(S.themes[(i + d + S.themes.length) % S.themes.length]); }
  function setTheme(n) { S.theme = n; render(); sset({ a: 'theme', v: n }); }
  let dragging = false;
  const barValue = (el) => {
    const i = Number(el.dataset.i);
    return S.min + ((i + 1) / BAR) * (100 - S.min);
  };
  panel.addEventListener('mousedown', (e) => {
    const t = e.target;
    if (t.dataset && t.dataset.i !== undefined) { dragging = true; setOpacity(barValue(t), false); }
    else if (t.dataset && t.dataset.d) stepTheme(Number(t.dataset.d));
    else if (t.id === 'sx') closePanel();
  });
  panel.addEventListener('mouseover', (e) => {
    if (dragging && e.target.dataset && e.target.dataset.i !== undefined) setOpacity(barValue(e.target), false);
  });
  window.addEventListener('mouseup', () => {
    if (dragging) { dragging = false; sset({ a: 'opacity', v: S.opacity, save: true }); }
  });
  document.addEventListener('keydown', (e) => {
    if (!panel.classList.contains('open')) return;
    const i = S.themes.indexOf(S.theme);
    if (e.key === 'Escape') closePanel();
    else if (e.key === 'ArrowLeft') setOpacity(S.opacity - 5, true);
    else if (e.key === 'ArrowRight') setOpacity(S.opacity + 5, true);
    else if (e.key === 'ArrowUp') stepTheme(-1);
    else if (e.key === 'ArrowDown') stepTheme(1);
    else return;
    e.preventDefault(); e.stopPropagation();
  }, true);
  // A click on the terminal asks the host for keyboard focus ("click to type"); the host only
  // acts on it when the window is interactive and not already focused.
  window.addEventListener('mousedown', () => send({ t: 'click' }), true);

  window.addEventListener('focus', () => term.focus());
  lastSize = term.cols + 'x' + term.rows;
  // Report what was really applied (used by the tests to prove config values take effect).
  send({ t: 'applied', scrollback: term.options.scrollback, fontSize: term.options.fontSize,
         fontFamily: term.options.fontFamily, unicode: term.unicode.activeVersion,
         copyOnSelect, rightClick, osc52: osc52Mode, linkHandler: !!term.options.linkHandler,
         background: term.options.theme && term.options.theme.background,
         foreground: term.options.theme && term.options.theme.foreground });
  send({ t: 'ready', cols: term.cols, rows: term.rows });
})();

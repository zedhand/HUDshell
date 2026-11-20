// HUDshell terminal front end: xterm.js <-> Rust host over a byte-safe JSON/base64 bridge.
// One page hosts any number of terminal views. Every bridge message that belongs to a shell carries its
// session id `s`; the host decides which sessions exist (it sends open / show / close).
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
  // Experiment switch: which Unicode width rules xterm.js uses ('default' | '11' | 'graphemes').
  const uni = injected.unicode || 'default';
  // Shift+Enter: xterm.js has no kitty keyboard protocol, so Shift+Enter would arrive as plain
  // Enter and submit the prompt. 'esc-cr' sends ESC CR (what Claude Code's own terminal setup
  // uses for Shift+Enter); 'none' leaves xterm.js behaviour untouched.
  const shiftEnter = injected.shiftEnter || 'none';

  const termHost = document.getElementById('term');
  const views = new Map();   // session id -> view
  let activeId = 1;
  const active = () => views.get(activeId);
  // Every shell-bound message names its session.
  const sendS = (id, o) => send(Object.assign({ s: id }, o));
  // Split panes: the host decides which shells are on screen and where (layout.rs); the page draws it.
  let L = { shape: [1, 1], cells: [], focus: 0, zoom: false, split: [50, 50], dividers: false, hint: '', preset: 'single', shown: 'single',
            own: false, groups: [], strip: [] };
  let haveLayout = false;
  const shownIds = () => (haveLayout ? L.cells.map((c) => c.id).filter((id) => id) : [activeId]);
  const isShown = (id) => shownIds().includes(id);

  // Tell the host something happened, at most once a minute per kind, never with terminal content.
  const noted = {};
  function noteOnce(what) {
    const now = Date.now();
    if (noted[what] && now - noted[what] < 60000) return;
    noted[what] = now;
    send({ t: 'note', what });
  }

  let cueTimer;
  function pulseCue() {
    document.body.classList.remove('cue');
    void document.body.offsetWidth; // restart the animation
    document.body.classList.add('cue');
    clearTimeout(cueTimer);
    cueTimer = setTimeout(() => document.body.classList.remove('cue'), 3000);
  }

  // ---- one terminal view per shell session ----------------------------------------------------
  function createView(id) {
    const pane = document.createElement('div');
    pane.className = 'pane';
    pane.dataset.id = String(id);
    termHost.appendChild(pane);
    // Small name in the corner of each pane while the window is split.
    const label = document.createElement('div');
    label.className = 'plabel';
    pane.appendChild(label);
    // A click on a pane that is not the focused one moves the keyboard there (the host decides).
    pane.addEventListener('mousedown', () => { if (id !== activeId) send({ t: 'switch', s: id }); });
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
      theme: currentTheme,
    });
    const fit = new FitAddon.FitAddon();
    term.loadAddon(fit);
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
    const v = { id, term, fit, pane, label, exited: false, lastSize: '', kitty: { flags: 0, stack: [] },
               title: '', prog: '', unread: false, attn: false, profile: '', color: '' };
    // The program's own window title (OSC 0 / 2) names the tab; control characters are never shown.
    term.onTitleChange((t) => {
      const clean = String(t).replace(/[\u0000-\u001f\u007f-\u009f]/g, '').trim();
      v.title = isExePath(clean) ? '' : clean.slice(0, 60);
      renderTabs();
    });
    const reply = (s) => sendS(id, { t: 'in', d: b2a(enc.encode(s)) });
    const num = (x) => (Array.isArray(x) ? Number(x[0]) : Number(x)) || 0;

    // Minimal kitty keyboard protocol negotiation (query / push / pop / set). Programs that want
    // distinguishable modified keys ask with CSI ? u and enable with CSI > flags u. We answer the
    // query and remember the flags; only then is Shift+Enter reported as CSI 13;2 u. Plain
    // programs (PowerShell's prompt) never negotiate it and keep seeing a normal Enter.
    // XTVERSION (CSI > q): identify honestly. Claude Code and Codex both ask for this.
    term.parser.registerCsiHandler({ prefix: '>', final: 'q' }, () => {
      reply('\x1bP>|HUDshell 0.1.0\x1b\\');
      return true;
    });
    term.parser.registerCsiHandler({ prefix: '?', final: 'u' }, () => {
      reply('\x1b[?' + v.kitty.flags + 'u');
      return true;
    });
    term.parser.registerCsiHandler({ prefix: '>', final: 'u' }, (p) => {
      v.kitty.stack.push(v.kitty.flags);
      if (v.kitty.stack.length > 32) v.kitty.stack.shift();
      v.kitty.flags = num(p[0]);
      return true;
    });
    term.parser.registerCsiHandler({ prefix: '<', final: 'u' }, (p) => {
      let n = p.length ? num(p[0]) || 1 : 1;
      while (n-- > 0 && v.kitty.stack.length) v.kitty.flags = v.kitty.stack.pop();
      if (!v.kitty.stack.length && n >= 0) v.kitty.flags = 0;
      return true;
    });
    term.parser.registerCsiHandler({ prefix: '=', final: 'u' }, (p) => {
      const f = num(p[0]);
      const mode = (p.length > 1 ? num(p[1]) : 1) || 1;
      if (mode === 1) v.kitty.flags = f;
      else if (mode === 2) v.kitty.flags |= f;
      else if (mode === 3) v.kitty.flags &= ~f;
      return true;
    });

    function copySelection() {
      const text = term.getSelection();
      return navigator.clipboard.writeText(text).then(
        () => send({ t: 'copied', chars: text.length }),
        (e) => send({ t: 'copied', error: String(e) }));
    }

    term.attachCustomKeyEventHandler((ev) => {
      // A finished shell: Enter starts a new one in this tab (the host spawns it at the current size).
      if (v.exited && ev.key === 'Enter' && !ev.shiftKey && !ev.ctrlKey && !ev.altKey && !ev.metaKey) {
        if (ev.type === 'keydown') {
          v.exited = false;
          v.kitty = { flags: 0, stack: [] };
          term.write('\r\n[starting a new shell]\r\n');
          sendS(id, { t: 'restart', cols: term.cols, rows: term.rows });
        }
        return false;
      }
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
        if (ev.type === 'keydown') reply(v.kitty.flags !== 0 ? '\x1b[13;2u' : '\x1b\r');
        return false;
      }
      // The browser fires keydown AND keypress for Enter; xterm.js routes both through this handler,
      // so send ESC CR once (on keydown) and swallow the keypress too, or a stray CR follows it.
      if (shiftEnter === 'esc-cr' && ev.key === 'Enter' &&
          ev.shiftKey && !ev.ctrlKey && !ev.altKey && !ev.metaKey) {
        if (ev.type === 'keydown') reply('\x1b\r');
        return false;
      }
      return true;
    });

    term.open(pane);

    // Mouse policies. Copy on select copies when the button is released; right-click (opt-in) copies a
    // selection and clears it, or pastes when nothing is selected (the host reads the clipboard).
    pane.addEventListener('contextmenu', (e) => e.preventDefault());
    pane.addEventListener('mouseup', (e) => {
      if (e.button === 0 && copyOnSelect && term.hasSelection()) copySelection();
      else if (e.button === 2 && rightClick === 'paste') {
        if (term.hasSelection()) { copySelection(); term.clearSelection(); }
        else send({ t: 'rightpaste' });
      }
    });

    // Programs ask for attention with the bell, OSC 9 (iTerm2 style) or OSC 777;notify (rxvt style).
    // The host decides whether to pulse the window or show a tray notification; it never logs the text.
    const attention = (kind, text) => {
      if (!isShown(id) && !v.attn) { v.attn = true; renderTabs(); }
      sendS(id, { t: 'attention', kind, text: String(text || '').slice(0, 200) });
    };
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

    // Input: onData is text (UTF-8 encode once); onBinary is a binary-coded string (keep byte values).
    term.onData((d) => sendS(id, { t: 'in', d: b2a(enc.encode(d)) }));
    term.onBinary((s) => {
      const u = new Uint8Array(s.length);
      for (let i = 0; i < s.length; i++) u[i] = s.charCodeAt(i) & 0xff;
      sendS(id, { t: 'in', d: b2a(u) });
    });

    // Resize: only tell the host when the cell size really changed.
    term.onResize(({ cols, rows }) => {
      const k = cols + 'x' + rows;
      if (k !== v.lastSize) { v.lastSize = k; sendS(id, { t: 'resize', cols, rows }); }
    });

    views.set(id, v);
    return v;
  }

  let currentTheme = startTheme;

  // Make a view the active one (it gets the keyboard); the layout decides what else is on screen.
  function showView(id) {
    const v = views.get(id);
    if (!v) return;
    activeId = id;
    v.unread = false;
    v.attn = false;
    drawLayout();
    v.term.focus();
    renderTabs();
  }

  // ---- split panes ----------------------------------------------------------------------------------
  const GAP = 3; // pixels between panes
  function cellRects(shape) {
    const [c, r] = shape;
    const cw = c === 2 ? [L.split[0], 100 - L.split[0]] : [100];
    const rh = r === 2 ? [L.split[1], 100 - L.split[1]] : [100];
    const out = [];
    for (let y = 0; y < r; y++) for (let x = 0; x < c; x++) out.push({ l: x === 0 ? 0 : cw[0], t: y === 0 ? 0 : rh[0], w: cw[x], h: rh[y], x, y });
    return out;
  }
  function place(el, q, shape) {
    const gl = q.x > 0 ? GAP / 2 : 0, gr = q.x < shape[0] - 1 ? GAP / 2 : 0;
    const gt = q.y > 0 ? GAP / 2 : 0, gb = q.y < shape[1] - 1 ? GAP / 2 : 0;
    el.style.left = 'calc(' + q.l + '% + ' + gl + 'px)';
    el.style.top = 'calc(' + q.t + '% + ' + gt + 'px)';
    el.style.width = 'calc(' + q.w + '% - ' + (gl + gr) + 'px)';
    el.style.height = 'calc(' + q.h + '% - ' + (gt + gb) + 'px)';
  }
  const fitShown = () => { for (const id of shownIds()) { const w = views.get(id); if (w) w.fit.fit(); } };
  function updateLabels() {
    for (const v of views.values()) v.label.textContent = '';
    if (!(haveLayout && L.cells.length > 1)) return;
    for (const c of L.cells) {
      const v = views.get(c.id);
      const i = v ? tabs.list.findIndex((e) => e.id === v.id) : -1;
      if (i < 0) continue;
      if (L.own) {
        // Own lists: "2/3 name" = the second of this pane's three shells, a dot when another one has news.
        const g = L.groups[c.slot] || [];
        const other = g.some((x) => { const w = views.get(x); return x !== v.id && w && (w.unread || w.attn); });
        v.label.textContent = (g.indexOf(v.id) + 1) + '/' + g.length + ' ' + labelOf(tabs.list[i]) + (other ? ' \u2022' : '');
      } else {
        v.label.textContent = (i + 1) + ' ' + labelOf(tabs.list[i]);
      }
    }
  }
  function drawLayout() {
    termHost.querySelectorAll('.empty, .divider').forEach((e) => e.remove());
    const shape = haveLayout ? L.shape : [1, 1];
    const cells = haveLayout ? L.cells : [{ slot: 0, id: activeId }];
    const split = cells.length > 1;
    const rects = cellRects(shape);
    const on = new Set();
    cells.forEach((c, i) => {
      const q = rects[i] || rects[0];
      const focused = !haveLayout || c.slot === L.focus;
      const w = c.id ? views.get(c.id) : null;
      if (w) {
        on.add(c.id);
        place(w.pane, q, shape);
        w.pane.classList.toggle('split', split);
        w.pane.classList.toggle('focus', split && focused);
      } else if (!c.id) {
        const e = document.createElement('div');
        e.className = 'pane empty split' + (focused ? ' focus' : '');
        e.dataset.slot = String(c.slot);
        e.textContent = 'empty pane' + (L.hint ? '\n' + L.hint + ' starts a shell here' : '');
        place(e, q, shape);
        e.addEventListener('mousedown', () => send({ t: 'panefocus', i: c.slot }));
        termHost.appendChild(e);
      }
    });
    for (const v of views.values()) v.pane.classList.toggle('hidden', !on.has(v.id));
    if (haveLayout && L.dividers && !L.zoom) addDividers(shape);
    fitShown();
    updateLabels();
    // The keyboard goes nowhere while the focused pane is empty.
    const fc = cells.find((c) => !haveLayout || c.slot === L.focus);
    if (fc && !fc.id) { const a = active(); if (a) a.term.blur(); }
    requestAnimationFrame(reportArea);
  }
  // Draggable dividers (config pane_dividers). They need the mouse, so they only work while the window
  // is not click-through.
  let drag = null, dragFrame = 0;
  function addDividers(shape) {
    const mk = (axis, pos) => {
      const d = document.createElement('div');
      d.className = 'divider ' + axis;
      d.style[axis === 'v' ? 'left' : 'top'] = 'calc(' + pos + '% - 3px)';
      d.addEventListener('mousedown', (e) => { drag = axis; e.preventDefault(); e.stopPropagation(); });
      termHost.appendChild(d);
    };
    if (shape[0] === 2) mk('v', L.split[0]);
    if (shape[1] === 2) mk('h', L.split[1]);
  }
  window.addEventListener('mousemove', (e) => {
    if (!drag) return;
    const r = termHost.getBoundingClientRect();
    const pct = drag === 'v' ? ((e.clientX - r.left) / r.width) * 100 : ((e.clientY - r.top) / r.height) * 100;
    L.split[drag === 'v' ? 0 : 1] = Math.max(20, Math.min(80, Math.round(pct)));
    if (!dragFrame) dragFrame = requestAnimationFrame(() => { dragFrame = 0; drawLayout(); });
  });
  window.addEventListener('mouseup', () => {
    if (drag) { drag = null; send({ t: 'split', c: L.split[0], r: L.split[1] }); }
  });
  // Tell the host how many cells the whole terminal area holds, so a window too small for the layout can
  // show fewer panes. The cell size comes from a pane on screen.
  let lastArea = '';
  function reportArea() {
    const v = isShown(activeId) ? views.get(activeId) : views.get(shownIds()[0]);
    if (!v || !v.term.cols || !v.term.rows) return;
    const core = v.term._core;
    const cell = core && core._renderService && core._renderService.dimensions && core._renderService.dimensions.css.cell;
    const pr = v.pane.getBoundingClientRect();
    const cw = (cell && cell.width) || pr.width / v.term.cols;
    const ch = (cell && cell.height) || pr.height / v.term.rows;
    const tr = termHost.getBoundingClientRect();
    if (!cw || !ch || !tr.width || !tr.height) return;
    const cols = Math.floor(tr.width / cw), rows = Math.floor(tr.height / ch);
    const k = cols + 'x' + rows;
    if (k !== lastArea) { lastArea = k; send({ t: 'area', cols, rows }); }
  }

  function closeView(id) {
    const v = views.get(id);
    if (!v) return;
    views.delete(id);
    v.term.dispose();
    v.pane.remove();
    if (activeId === id) {
      const next = views.keys().next().value;
      if (next !== undefined) showView(next);
    }
  }

  // ---- shell list: tab strip, quick overlay, profile picker ---------------------------------------
  const tabs = { list: [], active: 1, mode: 'auto', profiles: [] };
  const tabsEl = document.getElementById('tabs');
  const shellsEl = document.getElementById('shells');
  const pickerEl = document.getElementById('picker');
  // A bare executable path (PowerShell's default title) says nothing; show the profile name instead.
  const isExePath = (t) => /\.exe$/i.test(t);
  // What the tab says: the program's own title (like a normal terminal tab), else the profile name; with the
  // program running in it in front when the host recognised one (config tab_program).
  const labelOf = (e) => {
    const v = views.get(e.id);
    const t = v && v.title && !isExePath(v.title) ? v.title : '';
    const base = (t || e.profile || ('shell ' + e.id)).slice(0, 40);
    const p = v && v.prog;
    return p && !base.toLowerCase().includes(p) ? p + ' \u00b7 ' + base : base;
  };
  const markOf = (e) => { const v = views.get(e.id); return v ? (v.attn ? '!' : (v.unread ? '\u2022' : '')) : ''; };
  // The shells the tab strip, the list overlay and the number keys refer to: all of them, or (own lists per
  // pane) the ones of the focused pane.
  function stripList() {
    if (haveLayout && L.own) return L.strip.map((id) => tabs.list.find((e) => e.id === id)).filter(Boolean);
    return tabs.list;
  }
  const stripWanted = () => tabs.mode === 'always' || (tabs.mode === 'auto' && stripList().length > 1);
  function renderTabs() {
    tabsEl.textContent = '';
    stripList().forEach((e, i) => {
      const d = document.createElement('div');
      d.className = 'tab' + (e.id === activeId ? ' on' : (isShown(e.id) ? ' vis' : ''));
      d.dataset.id = String(e.id);
      if (e.color) d.style.setProperty('--accent', e.color);
      d.appendChild(document.createTextNode((i + 1) + ' ' + labelOf(e)));
      const m = markOf(e);
      if (m) { const sp = document.createElement('span'); sp.className = 'mark' + (m === '!' ? ' bang' : ''); sp.textContent = m; d.appendChild(sp); }
      tabsEl.appendChild(d);
    });
    const want = stripWanted();
    if (want !== document.body.classList.contains('tabs-on')) {
      document.body.classList.toggle('tabs-on', want);
      requestAnimationFrame(() => { fitShown(); reportArea(); });
    }
    updateLabels();
  }
  tabsEl.addEventListener('mousedown', (e) => {
    const t = e.target.closest('.tab');
    if (t) send({ t: 'switch', s: Number(t.dataset.id) });
  });
  let overlayTimer;
  function listText() {
    return stripList().map((e, i) => (e.id === activeId ? '\u25b6 ' : '  ') + (i + 1) + ' ' + labelOf(e) + (markOf(e) ? '  ' + markOf(e) : '')).join('\n');
  }
  function flashOverlay(extra) {
    shellsEl.textContent = listText() + (extra ? '\n\n' + extra : '');
    shellsEl.classList.add('open');
    clearTimeout(overlayTimer);
    overlayTimer = setTimeout(() => shellsEl.classList.remove('open'), 2200);
  }
  const pickerOpen = () => pickerEl.classList.contains('open');
  function openPicker() {
    const names = tabs.profiles.slice(0, 9);
    pickerEl.textContent = 'new shell\n' + '\u2500'.repeat(22) + '\n' + names.map((n, i) => (i + 1) + '  ' + n).join('\n') + '\n\nesc  cancel';
    pickerEl.classList.add('open');
  }
  function closePicker() {
    if (!pickerOpen()) return;
    pickerEl.classList.remove('open');
    send({ t: 'pickerclose' });
    const a = active(); if (a) a.term.focus();
  }
  document.addEventListener('keydown', (e) => {
    if (!pickerOpen()) return;
    if (e.key === 'Escape') closePicker();
    else if (/^[1-9]$/.test(e.key) && Number(e.key) <= tabs.profiles.length) {
      pickerEl.classList.remove('open');
      send({ t: 'newshell', profile: tabs.profiles[Number(e.key) - 1] });
    } else return;
    e.preventDefault(); e.stopPropagation();
  }, true);

  let rt;
  new ResizeObserver(() => { clearTimeout(rt); rt = setTimeout(() => { fitShown(); reportArea(); }, 40); }).observe(document.body);

  wv.addEventListener('message', (ev) => {
    let m;
    try { m = JSON.parse(ev.data); } catch (e) { return; }
    const sid = m.s === undefined ? activeId : m.s;
    const v = views.get(sid);
    switch (m.t) {
      case 'open': {
        // The host created a session: build its view, tell the host its size so the shell starts at it.
        const nv = createView(sid);
        showView(sid);
        nv.lastSize = nv.term.cols + 'x' + nv.term.rows;
        sendS(sid, { t: 'ready', cols: nv.term.cols, rows: nv.term.rows });
        break;
      }
      case 'show':
        showView(sid);
        break;
      case 'close': closeView(sid); renderTabs(); break;
      case 'layout':
        L = { shape: m.shape || [1, 1], cells: m.cells || [], focus: m.focus || 0, zoom: !!m.zoom, split: m.split || [50, 50],
              dividers: !!m.dividers, hint: m.hint || '', preset: m.preset || 'single', shown: m.shown || 'single',
              own: !!m.own, groups: m.groups || [], strip: m.strip || [] };
        haveLayout = true;
        drawLayout();
        renderTabs();
        break;
      case 'tabs':
        { const before = tabs.active; tabs.list = m.list || []; tabs.active = m.active; tabs.mode = m.mode || 'auto'; tabs.profiles = m.profiles || [];
          if (tabs.mode === 'overlay' && tabs.list.length > 1 && m.active !== before) setTimeout(() => flashOverlay(''), 0); }
        for (const e of tabs.list) { const w = views.get(e.id); if (w) { w.profile = e.profile; w.color = e.color || ''; } }
        renderTabs();
        break;
      case 'prog':
        // The host's full list of recognised programs (shells not listed run none).
        for (const w of views.values()) w.prog = '';
        for (const e of (m.list || [])) { const w = views.get(e.s); if (w) w.prog = String(e.p || '').slice(0, 20); }
        renderTabs();
        break;
      case 'overlay': flashOverlay(''); break;
      case 'confirm': flashOverlay('press the close key again to close shell ' + m.n); break;
      case 'full': flashOverlay('a window holds at most 9 shells'); break;
      case 'picker': openPicker(); break;
      case 'reset':
        // The last shell was closed: start a fresh one in the same tab.
        if (v) {
          v.term.reset();
          v.exited = false;
          v.kitty = { flags: 0, stack: [] };
          sendS(sid, { t: 'restart', cols: v.term.cols, rows: v.term.rows });
        }
        break;
      case 'attentionnext': {
        const order = tabs.list.map((e) => e.id);
        const i = Math.max(0, order.indexOf(activeId));
        const pick = order.map((_, k) => order[(i + 1 + k) % order.length]).find((id2) => { const w = views.get(id2); return id2 !== activeId && w && (w.attn || w.unread); });
        if (pick !== undefined) send({ t: 'switch', s: pick }); else flashOverlay('nothing new in the other shells');
        break;
      }
      case 'out': {
        if (!v) { send({ t: 'ack', n: a2b(m.d).length }); break; }
        const u = a2b(m.d);
        v.term.write(u, () => send({ t: 'ack', n: u.length }));
        if (!isShown(sid) && !v.unread) { v.unread = true; renderTabs(); }
        break;
      }
      case 'exit':
        if (v) {
          v.exited = true;
          v.term.write('\r\n[session ended - press Enter to start a new shell, or Ctrl+Alt+Q to quit HUDshell]\r\n');
        }
        break;
      case 'focus': { const a = active(); if (a) a.term.focus(); break; }
      case 'cue':
        pulseCue();
        break;
      case 'font':
        // Live font size change from the host hotkeys; refit so each shell gets the new cell count.
        for (const o of views.values()) o.term.options.fontSize = m.size;
        fitShown();
        requestAnimationFrame(reportArea);
        break;
      case 'paste': { const a = active(); if (a) a.term.paste(m.text); break; }
      case 'selectall': { const a = active(); if (a) a.term.selectAll(); break; }
      case 'theme':
        // Live colour change from the host (hotkey / tray menu / settings).
        currentTheme = m.theme;
        for (const o of views.values()) o.term.options.theme = m.theme;
        applyPageColours(m.theme);
        S.theme = m.name; render();
        break;
      case 'settings':
        showSettings(m);
        break;
      case 'dump': {
        const d = views.get(m.s === undefined ? activeId : m.s);
        if (!d) break;
        const term = d.term;
        const b = term.buffer.active;
        const lines = [];
        for (let i = 0; i < term.rows; i++) {
          const l = b.getLine(b.viewportY + i);
          lines.push(l ? l.translateToString(true) : '');
        }
        send({ t: 'dump', s: d.id, cols: term.cols, rows: term.rows, buf: b.type, modes: term.modes,
               cursor: [b.cursorX, b.cursorY], unicode: uni, view: [b.viewportY, b.baseY],
               cue: document.body.classList.contains('cue'), fs: term.options.fontSize,
               ui: { strip: document.body.classList.contains('tabs-on'), active: activeId,
                     labels: tabs.list.map((e, i) => (i + 1) + ' ' + labelOf(e) + markOf(e)),
                     progs: tabs.list.map((e) => (views.get(e.id) || {}).prog || '').join(','),
                     rects: (() => {
                       const rc = (el) => { const b = el.getBoundingClientRect(); return [Math.round(b.left), Math.round(b.top), Math.round(b.width), Math.round(b.height)]; };
                       return { dpr: window.devicePixelRatio, term: rc(termHost),
                                tabs: [...tabsEl.querySelectorAll('.tab')].map((t) => ({ id: Number(t.dataset.id), r: rc(t) })),
                                dividers: [...termHost.querySelectorAll('.divider')].map((d) => ({ axis: d.classList.contains('v') ? 'v' : 'h', r: rc(d) })) };
                     })(),
                     overlay: shellsEl.classList.contains('open') ? shellsEl.textContent : '', picker: pickerOpen(),
                     layout: { own: L.own, gt: L.groups.map((g) => g.join(',')).join('|'), st: L.strip.join(','),
                               te: tabsEl.querySelectorAll('.tab').length, preset: L.preset, shown: L.shown, shape: L.shape, cells: L.cells.map((c) => c.id), focus: L.focus,
                               zoom: L.zoom, split: L.split, area: lastArea,
                               sizes: L.cells.map((c) => { const w = views.get(c.id); return w ? w.term.cols + 'x' + w.term.rows : '-'; }),
                               visible: [...views.values()].filter((w) => !w.pane.classList.contains('hidden')).map((w) => w.id),
                               labels: L.cells.map((c) => { const w = views.get(c.id); return w ? w.label.textContent : ''; }),
                               dividers: termHost.querySelectorAll('.divider').length, empties: termHost.querySelectorAll('.empty').length } },
               lines });
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
    out += '\n<span class="k">left/right  opacity\nup/down     colours</span>\n';
    out += '<span class="k">esc</span> or <span class="c" id="sx">[close]</span>';
    panel.innerHTML = out;
  }
  function closePanel() {
    if (!panel.classList.contains('open')) return;
    panel.classList.remove('open');
    sset({ a: 'close' });
    const a = active(); if (a) a.term.focus();
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
  // The web view covers the whole window, so a press on the top strip or on an edge never reaches the host's own
  // hit-test (which says "caption" / "border" there). The page says so instead, and the host starts Windows' own
  // move / resize loop (so edge snapping and the size limits apply). Neither takes the keyboard.
  window.addEventListener('mousedown', (e) => {
    const t = e.target && e.target.closest ? e.target.closest('#drag, .edge') : null;
    if (t && e.button === 0) {
      send({ t: 'nc', hit: t.id === 'drag' ? 'caption' : t.dataset.hit });
      e.preventDefault();
      return;
    }
    send({ t: 'click' });
  }, true);

  window.addEventListener('focus', () => { const a = active(); if (a) a.term.focus(); });

  // ---- start: session 1 exists from the beginning ------------------------------------------------
  const first = createView(1);
  showView(1);
  first.lastSize = first.term.cols + 'x' + first.term.rows;
  // Report what was really applied (used by the tests to prove config values take effect).
  const t1 = first.term;
  send({ t: 'applied', scrollback: t1.options.scrollback, fontSize: t1.options.fontSize,
         fontFamily: t1.options.fontFamily, unicode: t1.unicode.activeVersion,
         copyOnSelect, rightClick, osc52: osc52Mode, linkHandler: !!t1.options.linkHandler,
         background: t1.options.theme && t1.options.theme.background,
         foreground: t1.options.theme && t1.options.theme.foreground });
  sendS(1, { t: 'ready', cols: t1.cols, rows: t1.rows });
  send({ t: 'tabsneeded' });
})();

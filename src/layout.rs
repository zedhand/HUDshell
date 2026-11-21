//! Split panes inside the one window: which layout preset is chosen, which shell each pane shows, which
//! pane has the keyboard, zoom, and the fallback to fewer panes when the window is too small. Pure logic
//! on shell ids (0 = an empty pane), so it can be unit-tested without a window.
//!
//! Two ways to share shells between panes (config `pane_shells`):
//! * shared (default): every pane shows a shell from the one tab list;
//! * own: each pane keeps its own list of shells; switching, jumping and new shells act on the focused
//!   pane's list, and going to fewer panes moves the shells of the dropped panes into the focused one.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Single,
    /// Two panes next to each other.
    Columns,
    /// Two panes above each other.
    Rows,
    /// Four panes, 2 x 2.
    Grid,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Single, Preset::Columns, Preset::Rows, Preset::Grid];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Single => "single",
            Preset::Columns => "side-by-side",
            Preset::Rows => "stacked",
            Preset::Grid => "grid",
        }
    }

    pub fn parse(s: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s.trim()))
    }

    /// (columns, rows) of panes.
    pub fn shape(self) -> (usize, usize) {
        match self {
            Preset::Single => (1, 1),
            Preset::Columns => (2, 1),
            Preset::Rows => (1, 2),
            Preset::Grid => (2, 2),
        }
    }

    pub fn panes(self) -> usize {
        let (c, r) = self.shape();
        c * r
    }

    /// The order the layout hotkey steps through.
    pub fn next(self) -> Preset {
        match self {
            Preset::Single => Preset::Columns,
            Preset::Columns => Preset::Rows,
            Preset::Rows => Preset::Grid,
            Preset::Grid => Preset::Single,
        }
    }

    /// What to try, in order, when this preset does not fit the window.
    fn fallbacks(self) -> &'static [Preset] {
        match self {
            Preset::Single => &[Preset::Single],
            Preset::Columns => &[Preset::Columns, Preset::Single],
            Preset::Rows => &[Preset::Rows, Preset::Single],
            Preset::Grid => &[Preset::Grid, Preset::Columns, Preset::Rows, Preset::Single],
        }
    }
}

/// Lowest and highest split percentage (the share of the first column / row).
pub const SPLIT_MIN: u32 = 20;
pub const SPLIT_MAX: u32 = 80;

pub struct Layout {
    preset: Preset,
    /// Shell id per pane, in reading order (left to right, top to bottom). 0 = empty pane.
    slots: Vec<u32>,
    focus: usize,
    zoom: bool,
    /// Size of the whole terminal area in cells; (0, 0) = not known yet (no fallback).
    area: (u32, u32),
    /// Smallest pane worth drawing, in cells.
    min: (u32, u32),
    /// Share (percent) of the first column and of the first row.
    split: (u32, u32),
    /// Each pane has its own list of shells (`pane_shells = "own"`).
    own: bool,
    /// Own mode only: the shells of each pane, parallel to `slots`.
    groups: Vec<Vec<u32>>,
}

impl Layout {
    pub const fn new() -> Self {
        Self {
            preset: Preset::Single,
            slots: Vec::new(),
            focus: 0,
            zoom: false,
            area: (0, 0),
            min: (30, 6),
            split: (50, 50),
            own: false,
            groups: Vec::new(),
        }
    }

    /// Start with one shell in a single pane.
    pub fn start(&mut self, first: u32) {
        self.preset = Preset::Single;
        self.slots = vec![first];
        self.groups = vec![vec![first]];
        self.focus = 0;
        self.zoom = false;
    }

    /// Choose shared (false) or per-pane (true) shell lists. Set once, before any change.
    pub fn set_own(&mut self, own: bool) {
        self.own = own;
    }

    pub fn own(&self) -> bool {
        self.own
    }

    /// Own mode: which pane's list holds `id`.
    fn group_of(&self, id: u32) -> Option<usize> {
        self.groups.iter().position(|g| g.contains(&id))
    }

    /// Own mode: the focused pane's shells, in order (what the tab strip and jump hotkeys use).
    pub fn tab_order(&self) -> Option<Vec<u32>> {
        if self.own { self.groups.get(self.focus).cloned() } else { None }
    }

    /// Own mode: the shells of pane `i`.
    pub fn group(&self, i: usize) -> &[u32] {
        self.groups.get(i).map(|g| g.as_slice()).unwrap_or(&[])
    }

    /// Own mode: next / previous shell in the focused pane (wrapping). Returns the shell now shown.
    pub fn group_step(&mut self, delta: i32) -> Option<u32> {
        let g = self.groups.get(self.focus)?;
        if g.is_empty() {
            return None;
        }
        let cur = g.iter().position(|&x| x == self.slots[self.focus]).unwrap_or(0) as i32;
        let id = g[(cur + delta).rem_euclid(g.len() as i32) as usize];
        self.slots[self.focus] = id;
        Some(id)
    }

    /// Own mode: the `n`-th shell (1-based) of the focused pane.
    pub fn group_jump(&mut self, n: usize) -> Option<u32> {
        let id = *self.groups.get(self.focus)?.get(n.checked_sub(1)?)?;
        self.slots[self.focus] = id;
        Some(id)
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    pub fn slots(&self) -> &[u32] {
        &self.slots
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn zoomed(&self) -> bool {
        self.zoom
    }

    pub fn split(&self) -> (u32, u32) {
        self.split
    }

    /// The shell in the focused pane (0 when that pane is empty).
    /// Put back a saved layout. `slots` is the shell id per pane (0 = empty), `groups` the own-lists (only used in own mode; when
    /// they are missing each pane's list is its own shell, and shells in no pane join the focused pane's list), `all` every shell id.
    pub fn restore(&mut self, preset: Preset, split: (u32, u32), slots: Vec<u32>, groups: Vec<Vec<u32>>, focus: usize, all: &[u32]) {
        let n = preset.panes();
        let mut slots = slots;
        slots.resize(n, 0);
        slots.truncate(n);
        let from_slots: Vec<Vec<u32>> = slots.iter().map(|&s| if s != 0 { vec![s] } else { Vec::new() }).collect();
        let mut g = if self.own && groups.len() == n { groups } else { from_slots };
        if self.own {
            let focus = focus.min(n - 1);
            for &id in all {
                if !g.iter().any(|l| l.contains(&id)) {
                    g[focus].push(id);
                }
            }
        }
        self.preset = preset;
        self.set_split(split.0, split.1);
        self.groups = g;
        self.slots = slots;
        self.focus = focus.min(n - 1);
        self.zoom = false;
    }

    /// Move the focus to the next pane (from the focused one, wrapping) that shows a shell, without changing
    /// what any pane shows. Returns that shell, 0 when every pane is empty.
    pub fn focus_nonempty(&mut self) -> u32 {
        let n = self.slots.len();
        for k in 0..n {
            let i = (self.focus + k) % n;
            if self.slots[i] != 0 {
                self.focus = i;
                return self.slots[i];
            }
        }
        0
    }

    /// Indexes of the panes that show nothing.
    pub fn empty_panes(&self) -> Vec<usize> {
        self.slots.iter().enumerate().filter(|(_, s)| **s == 0).map(|(i, _)| i).collect()
    }

    pub fn focused_id(&self) -> u32 {
        self.slots.get(self.focus).copied().unwrap_or(0)
    }

    pub fn set_min(&mut self, cols: u32, rows: u32) {
        self.min = (cols.max(1), rows.max(1));
    }

    pub fn set_split(&mut self, cols_pct: u32, rows_pct: u32) {
        self.split = (cols_pct.clamp(SPLIT_MIN, SPLIT_MAX), rows_pct.clamp(SPLIT_MIN, SPLIT_MAX));
    }

    /// Record the terminal area in cells. Returns true when that changes what is drawn.
    pub fn set_area(&mut self, cols: u32, rows: u32) -> bool {
        let before = self.shown();
        self.area = (cols, rows);
        self.shown() != before
    }

    /// Does `p` give every pane at least the minimum size in the known area?
    fn fits(&self, p: Preset) -> bool {
        let (ac, ar) = self.area;
        if ac == 0 || ar == 0 {
            return true;
        }
        let (c, r) = p.shape();
        // The smaller share, less one cell for the gap between panes.
        let w = if c == 2 { ac * self.split.0.min(100 - self.split.0) / 100 - 1 } else { ac };
        let h = if r == 2 { ar * self.split.1.min(100 - self.split.1) / 100 - 1 } else { ar };
        w >= self.min.0 && h >= self.min.1
    }

    /// The preset actually drawn: the chosen one, or the largest fallback that fits the window.
    pub fn shown(&self) -> Preset {
        if self.zoom {
            return Preset::Single;
        }
        self.preset.fallbacks().iter().copied().find(|p| self.fits(*p)).unwrap_or(Preset::Single)
    }

    /// Panes on screen as (pane index, shell id), in drawing order. When the window is too small for the
    /// chosen preset, the focused pane and the ones after it are drawn; when zoomed, only the focused one.
    pub fn cells(&self) -> Vec<(usize, u32)> {
        let n = self.slots.len();
        if n == 0 {
            return Vec::new();
        }
        let k = self.shown().panes().min(n);
        if k == n {
            return self.slots.iter().copied().enumerate().collect();
        }
        (0..k).map(|i| (self.focus + i) % n).map(|i| (i, self.slots[i])).collect()
    }

    #[allow(dead_code)]
    pub fn is_shown(&self, id: u32) -> bool {
        id != 0 && self.cells().iter().any(|(_, s)| *s == id)
    }

    /// Choose a preset. The focused shell goes first, then the shells already on screen, then the rest of
    /// `order` (the tab order) starting after the focused one. Returns how many panes are left empty.
    /// Own mode: the focused pane comes first with its shells, then the other panes in order; new panes start
    /// empty, and the shells of panes that no longer fit move into the focused pane.
    pub fn set_preset(&mut self, p: Preset, order: &[u32], active: u32) -> usize {
        if self.own {
            return self.set_preset_own(p);
        }
        let n = p.panes();
        let mut fill: Vec<u32> = Vec::new();
        let push = |id: u32, fill: &mut Vec<u32>| {
            if id != 0 && order.contains(&id) && !fill.contains(&id) {
                fill.push(id);
            }
        };
        push(active, &mut fill);
        for &id in &self.slots {
            push(id, &mut fill);
        }
        let start = order.iter().position(|&x| x == active).map(|i| i + 1).unwrap_or(0);
        for k in 0..order.len() {
            push(order[(start + k) % order.len()], &mut fill);
        }
        fill.truncate(n);
        let empty = n - fill.len();
        fill.resize(n, 0);
        self.preset = p;
        self.slots = fill;
        self.focus = 0;
        self.zoom = false;
        empty
    }

    fn set_preset_own(&mut self, p: Preset) -> usize {
        let n = p.panes();
        let f = self.focus.min(self.groups.len().saturating_sub(1));
        let mut panes: Vec<(Vec<u32>, u32)> = Vec::new();
        if let Some(g) = self.groups.get(f) {
            panes.push((g.clone(), self.slots.get(f).copied().unwrap_or(0)));
        }
        for (i, g) in self.groups.iter().enumerate() {
            if i != f {
                panes.push((g.clone(), self.slots.get(i).copied().unwrap_or(0)));
            }
        }
        if panes.len() > n {
            let dropped: Vec<(Vec<u32>, u32)> = panes.drain(n..).collect();
            for (g, _) in dropped {
                panes[0].0.extend(g);
            }
            if panes[0].1 == 0 {
                panes[0].1 = panes[0].0.first().copied().unwrap_or(0);
            }
        }
        while panes.len() < n {
            panes.push((Vec::new(), 0));
        }
        let empty = panes.iter().filter(|(g, _)| g.is_empty()).count();
        self.groups = panes.iter().map(|(g, _)| g.clone()).collect();
        self.slots = panes.iter().map(|(_, s)| *s).collect();
        self.preset = p;
        self.focus = 0;
        self.zoom = false;
        empty
    }

    /// Step to the next preset (see `Preset::next`). Returns how many panes are left empty.
    pub fn cycle(&mut self, order: &[u32], active: u32) -> usize {
        self.set_preset(self.preset.next(), order, active)
    }

    /// Bring shell `id` on screen: focus its pane if it is in one, otherwise show it in the focused pane.
    /// Own mode: focus the pane whose list holds `id` and show it there; a shell in no list joins the
    /// focused pane.
    pub fn show(&mut self, id: u32) {
        if self.slots.is_empty() {
            self.slots = vec![id];
            self.groups = vec![vec![id]];
            self.focus = 0;
            return;
        }
        if self.own {
            match self.group_of(id) {
                Some(g) => self.focus = g,
                None => self.groups[self.focus].push(id),
            }
            self.slots[self.focus] = id;
            return;
        }
        match self.slots.iter().position(|&s| s == id) {
            Some(i) => self.focus = i,
            None => self.slots[self.focus] = id,
        }
    }

    /// A new shell: into the focused pane if it is empty, else into the first empty pane, else it replaces
    /// the shell in the focused pane (that shell stays open as a tab). The new shell's pane gets the focus.
    /// Own mode: it always joins the focused pane's list.
    pub fn place_new(&mut self, id: u32) {
        if self.slots.is_empty() || self.own {
            self.show(id);
        } else if self.slots[self.focus] == 0 {
            self.slots[self.focus] = id;
        } else if let Some(i) = self.slots.iter().position(|&s| s == 0) {
            self.slots[i] = id;
            self.focus = i;
        } else {
            self.slots[self.focus] = id;
        }
    }

    /// Focus the next (`delta` = 1) or previous (-1) pane, wrapping. Returns the shell in it (0 = empty).
    pub fn focus_step(&mut self, delta: i32) -> u32 {
        let n = self.slots.len() as i32;
        if n > 0 {
            self.focus = (self.focus as i32 + delta).rem_euclid(n) as usize;
        }
        self.focused_id()
    }

    /// Focus pane `i` (a click on it). Returns false when there is no such pane.
    pub fn set_focus(&mut self, i: usize) -> bool {
        if i < self.slots.len() {
            self.focus = i;
            true
        } else {
            false
        }
    }

    /// Zoom the focused pane to the whole window, or back. Returns the new state; a single pane never zooms.
    pub fn toggle_zoom(&mut self) -> bool {
        self.zoom = !self.zoom && self.slots.len() > 1;
        self.zoom
    }

    /// A shell was closed. Its pane shows `prefer` (the neighbour the tab list picked, 0 = none) when that
    /// shell is not already on screen, else the first shell from `order` that is not, or becomes empty when
    /// every remaining shell is already showing.
    /// Own mode: the pane shows the next shell of its own list (or becomes empty when the list is empty).
    pub fn remove(&mut self, id: u32, order: &[u32], prefer: u32) {
        if self.own {
            if let Some(g) = self.group_of(id) {
                let pos = self.groups[g].iter().position(|&x| x == id).unwrap_or(0);
                self.groups[g].remove(pos);
                if self.slots[g] == id {
                    let list = &self.groups[g];
                    self.slots[g] = if list.is_empty() { 0 } else { list[pos.min(list.len() - 1)] };
                }
            }
            return;
        }
        if let Some(i) = self.slots.iter().position(|&s| s == id) {
            let free = |x: &u32| *x != id && !self.slots.contains(x);
            let hidden = Some(prefer).filter(|p| *p != 0 && order.contains(p) && free(p)).or_else(|| order.iter().copied().find(free));
            self.slots[i] = hidden.unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(order: &[u32]) -> Layout {
        let mut l = Layout::new();
        l.start(order[0]);
        l
    }

    #[test]
    fn presets_parse_name_and_cycle() {
        for p in Preset::ALL {
            assert_eq!(Preset::parse(p.name()), Some(p));
        }
        assert_eq!(Preset::parse(" Grid "), Some(Preset::Grid));
        assert_eq!(Preset::parse("2x2"), None);
        let mut p = Preset::Single;
        let mut seen = vec![];
        for _ in 0..4 {
            p = p.next();
            seen.push(p.panes());
        }
        assert_eq!(seen, vec![2, 2, 4, 1]);
        assert_eq!(p, Preset::Single, "four steps return to the start");
    }

    #[test]
    fn a_preset_fills_with_the_focused_shell_then_tab_order() {
        let mut l = with(&[3]);
        let order = [1, 2, 3, 4, 5];
        assert_eq!(l.set_preset(Preset::Columns, &order, 3), 0);
        assert_eq!(l.slots(), &[3, 4], "focused shell first, then the next one in tab order");
        assert_eq!((l.focus(), l.focused_id()), (0, 3));
        assert_eq!(l.set_preset(Preset::Grid, &order, 3), 0);
        assert_eq!(l.slots(), &[3, 4, 5, 1], "shells already on screen stay, the rest wrap around the tab order");
    }

    #[test]
    fn missing_shells_leave_empty_panes_and_are_counted() {
        let mut l = with(&[1]);
        assert_eq!(l.set_preset(Preset::Grid, &[1, 2], 1), 2);
        assert_eq!(l.slots(), &[1, 2, 0, 0]);
        assert_eq!(l.cycle(&[1, 2], 1), 0, "grid -> single");
        assert_eq!(l.slots(), &[1]);
    }

    #[test]
    fn showing_a_shell_focuses_its_pane_or_replaces_the_focused_one() {
        let mut l = with(&[1]);
        l.set_preset(Preset::Columns, &[1, 2, 3], 1);
        l.show(2);
        assert_eq!((l.focus(), l.slots()), (1, &[1u32, 2][..]), "already on screen: focus moves there");
        l.show(3);
        assert_eq!(l.slots(), &[1, 3], "not on screen: it replaces the shell in the focused pane");
        assert_eq!(l.focus(), 1);
    }

    #[test]
    fn a_new_shell_prefers_the_focused_empty_pane_then_any_empty_one() {
        let mut l = with(&[1]);
        l.set_preset(Preset::Grid, &[1], 1);
        l.set_focus(2);
        l.place_new(7);
        assert_eq!((l.slots(), l.focus()), (&[1u32, 0, 7, 0][..], 2));
        l.set_focus(0);
        l.place_new(8);
        assert_eq!((l.slots(), l.focus()), (&[1u32, 8, 7, 0][..], 1), "first empty pane, which gets the focus");
        l.place_new(9);
        l.place_new(10);
        assert_eq!(l.slots(), &[1, 8, 7, 10], "no empty pane left: it replaces the focused shell");
    }

    #[test]
    fn pane_focus_wraps_and_reports_empty_panes() {
        let mut l = with(&[1]);
        l.set_preset(Preset::Grid, &[1, 2], 1);
        assert_eq!(l.focus_step(1), 2);
        assert_eq!(l.focus_step(1), 0, "an empty pane");
        assert_eq!((l.focus_step(-3), l.focus()), (0, 3), "wraps backwards");
        assert_eq!(l.focus_step(1), 1, "and forwards");
        assert!(!l.set_focus(4));
    }

    #[test]
    fn zoom_shows_only_the_focused_pane_and_never_for_one_pane() {
        let mut l = with(&[1]);
        assert!(!l.toggle_zoom(), "a single pane does not zoom");
        l.set_preset(Preset::Columns, &[1, 2], 1);
        l.focus_step(1);
        assert!(l.toggle_zoom());
        assert_eq!(l.cells(), vec![(1, 2)]);
        assert_eq!(l.shown(), Preset::Single);
        l.focus_step(1);
        assert_eq!(l.cells(), vec![(0, 1)], "moving the focus while zoomed shows the other pane zoomed");
        assert!(!l.toggle_zoom());
        assert_eq!(l.cells().len(), 2);
        l.toggle_zoom();
        l.set_preset(Preset::Grid, &[1, 2], 1);
        assert!(!l.zoomed(), "a new preset ends zoom");
    }

    #[test]
    fn a_small_window_falls_back_to_fewer_panes() {
        let mut l = with(&[1]);
        l.set_preset(Preset::Grid, &[1, 2, 3, 4], 1);
        assert_eq!(l.shown(), Preset::Grid, "unknown area: drawn as chosen");
        assert!(!l.set_area(100, 30), "fits: nothing changes");
        assert!(l.set_area(56, 16), "too narrow for two columns of 30");
        assert_eq!(l.shown(), Preset::Rows);
        assert!(l.set_area(80, 10));
        assert_eq!(l.shown(), Preset::Columns, "too short for two rows of 6");
        assert!(l.set_area(40, 10));
        assert_eq!(l.shown(), Preset::Single);
        l.set_focus(2);
        assert_eq!(l.cells(), vec![(2, 3)], "the focused pane is the one drawn");
        l.set_area(80, 10);
        assert_eq!(l.cells(), vec![(2, 3), (3, 4)], "the focused pane and the one after it");
        assert_eq!(l.preset(), Preset::Grid, "the chosen preset is kept for when the window grows");
    }

    #[test]
    fn minimum_and_split_change_what_fits() {
        let mut l = with(&[1]);
        l.set_preset(Preset::Columns, &[1, 2], 1);
        l.set_area(70, 20);
        assert_eq!(l.shown(), Preset::Columns);
        l.set_split(30, 50);
        assert_eq!(l.shown(), Preset::Single, "a 30% column is 20 cells, under the minimum of 30");
        l.set_min(20, 6);
        assert_eq!(l.shown(), Preset::Columns);
        l.set_split(5, 95);
        assert_eq!(l.split(), (SPLIT_MIN, SPLIT_MAX), "the split is clamped");
    }

    fn own() -> Layout {
        let mut l = Layout::new();
        l.set_own(true);
        l.start(1);
        l
    }

    #[test]
    fn own_mode_new_panes_start_empty_and_new_shells_join_the_focused_pane() {
        let mut l = own();
        assert_eq!(l.set_preset(Preset::Columns, &[1, 2, 3], 1), 1, "other shells do not move into the new pane");
        assert_eq!(l.slots(), &[1, 0]);
        l.set_focus(1);
        l.place_new(4);
        l.place_new(5);
        assert_eq!((l.slots(), l.group(1)), (&[1u32, 5][..], &[4u32, 5][..]), "both join pane 2; the newest is shown");
        assert_eq!(l.tab_order(), Some(vec![4, 5]), "the tab strip lists the focused pane's shells");
        l.set_focus(0);
        l.place_new(6);
        assert_eq!((l.group(0), l.slots()), (&[1u32, 6][..], &[6u32, 5][..]), "a pane with a shell gets another tab");
    }

    #[test]
    fn own_mode_switching_and_jumping_stay_in_the_focused_pane() {
        let mut l = own();
        l.set_preset(Preset::Columns, &[1], 1);
        l.set_focus(1);
        l.place_new(2);
        l.place_new(3);
        assert_eq!(l.group_step(-1), Some(2));
        assert_eq!(l.group_step(-1), Some(3), "wraps within the pane");
        assert_eq!(l.group_jump(1), Some(2));
        assert_eq!(l.group_jump(3), None, "pane 2 has two shells");
        l.set_focus(0);
        assert_eq!(l.group_step(1), Some(1), "pane 1 has only shell 1");
        assert_eq!(l.slots(), &[1, 2]);
        l.show(3);
        assert_eq!((l.focus(), l.slots()), (1, &[1u32, 3][..]), "switching to a shell of another pane focuses that pane");
    }

    #[test]
    fn own_mode_closing_shows_the_next_shell_of_the_pane_or_leaves_it_empty() {
        let mut l = own();
        l.set_preset(Preset::Columns, &[1], 1);
        l.set_focus(1);
        l.place_new(2);
        l.place_new(3);
        l.remove(3, &[], 0);
        assert_eq!((l.slots(), l.group(1)), (&[1u32, 2][..], &[2u32][..]));
        l.remove(2, &[], 0);
        assert_eq!(l.slots(), &[1, 0], "nothing borrowed from pane 1");
        assert_eq!(l.focused_id(), 0);
    }

    #[test]
    fn own_mode_fewer_panes_merge_into_the_focused_pane() {
        let mut l = own();
        l.set_preset(Preset::Grid, &[1], 1);
        for (pane, id) in [(1, 2), (2, 3), (2, 4), (3, 5)] {
            l.set_focus(pane);
            l.place_new(id);
        }
        l.set_focus(2);
        assert_eq!(l.set_preset(Preset::Columns, &[], 0), 0);
        assert_eq!(l.slots(), &[4, 1], "the focused pane comes first, still showing its shell");
        assert_eq!(l.group(0), &[3, 4, 2, 5], "the dropped panes' shells joined it");
        assert_eq!(l.group(1), &[1]);
        assert_eq!(l.set_preset(Preset::Single, &[], 0), 0);
        assert_eq!((l.group(0), l.slots()), (&[3u32, 4, 2, 5, 1][..], &[4u32][..]), "no shell is lost");
        assert_eq!(l.set_preset(Preset::Rows, &[], 0), 1, "growing again adds an empty pane");
    }

    #[test]
    fn closing_prefers_the_neighbour_the_tab_list_picked() {
        let mut l = with(&[2]);
        assert_eq!(l.slots(), &[2]);
        l.remove(2, &[1, 3, 4], 3);
        assert_eq!(l.slots(), &[3], "a single pane takes the right-hand neighbour, not the first shell");
        l.remove(3, &[1, 4], 9);
        assert_eq!(l.slots(), &[1], "an unknown preference falls back to the first hidden shell");
        let mut c = with(&[1]);
        c.set_preset(Preset::Columns, &[1, 2, 3], 1);
        c.remove(1, &[2, 3], 2);
        assert_eq!(c.slots(), &[3, 2], "a preferred shell that is already on screen is not used twice");
    }

    #[test]
    fn the_focus_can_leave_an_empty_pane_without_changing_any_pane() {
        let mut l = own();
        l.set_preset(Preset::Columns, &[1], 1);
        l.set_focus(1);
        assert_eq!(l.focus_nonempty(), 1);
        assert_eq!((l.focus(), l.slots()), (0, &[1u32, 0][..]));
        assert_eq!(l.focus_nonempty(), 1, "already on a pane with a shell: stays");
        assert_eq!(l.empty_panes(), vec![1]);
        let mut e = Layout::new();
        e.set_preset(Preset::Columns, &[], 0);
        assert_eq!(e.focus_nonempty(), 0, "every pane empty");
    }

    #[test]
    fn a_saved_layout_can_be_put_back() {
        let mut l = with(&[1]);
        l.restore(Preset::Grid, (60, 40), vec![3, 1, 0, 2], vec![], 1, &[1, 2, 3]);
        assert_eq!((l.preset(), l.slots(), l.focus(), l.split()), (Preset::Grid, &[3u32, 1, 0, 2][..], 1, (60, 40)));
        assert!(!l.zoomed());
        l.restore(Preset::Columns, (50, 50), vec![2], vec![], 9, &[1, 2]);
        assert_eq!((l.slots(), l.focus()), (&[2u32, 0][..], 1), "too few panes are padded, an impossible focus is brought back");
    }

    #[test]
    fn own_lists_come_back_and_orphans_join_the_focused_pane() {
        let mut l = own();
        l.restore(Preset::Columns, (50, 50), vec![1, 3], vec![vec![1, 2], vec![3]], 1, &[1, 2, 3]);
        assert_eq!((l.group(0), l.group(1)), (&[1u32, 2][..], &[3u32][..]));
        // saved without lists (it was saved in shared mode): each pane has its own shell, the rest join the focused pane
        let mut m = own();
        m.restore(Preset::Columns, (50, 50), vec![1, 3], vec![], 1, &[1, 2, 3, 4]);
        assert_eq!((m.group(0), m.group(1)), (&[1u32][..], &[3u32, 2, 4][..]));
    }

    #[test]
    fn shared_mode_has_no_per_pane_tab_order() {
        let mut l = with(&[1]);
        l.set_preset(Preset::Columns, &[1, 2], 1);
        assert_eq!(l.tab_order(), None);
        assert!(!l.own());
    }

    #[test]
    fn closing_refills_from_hidden_shells_or_leaves_the_pane_empty() {
        let mut l = with(&[1]);
        l.set_preset(Preset::Columns, &[1, 2, 3], 1);
        l.remove(1, &[2, 3], 0);
        assert_eq!(l.slots(), &[3, 2], "a shell that was not on screen takes the pane");
        l.remove(3, &[2], 0);
        assert_eq!(l.slots(), &[0, 2], "nothing hidden left: the pane is empty");
        l.remove(42, &[2], 0);
        assert_eq!(l.slots(), &[0, 2], "unknown ids are ignored");
        assert!(l.is_shown(2) && !l.is_shown(0));
    }
}

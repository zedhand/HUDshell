//! The list of shells in the window: tab order, which one is on screen, switching, limits and the
//! "press again to close" confirmation. Pure logic on ids, so it can be unit-tested without a window.

/// Most shells one window holds. The jump hotkeys cover 1..=9.
pub const MAX_SHELLS: usize = 9;
/// A second close press within this many milliseconds confirms the first.
pub const CLOSE_CONFIRM_MS: u64 = 3000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u32,
    pub profile: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CloseStep {
    /// First press: ask the user to press again.
    Confirm,
    /// Second press in time: close it.
    Close,
}

#[derive(Default)]
pub struct Shells {
    order: Vec<Entry>,
    active: u32,
    next_id: u32,
    pending_close: Option<(u32, u64)>,
}

impl Shells {
    pub const fn new() -> Self {
        Self { order: Vec::new(), active: 0, next_id: 1, pending_close: None }
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn active(&self) -> u32 {
        self.active
    }

    pub fn entries(&self) -> &[Entry] {
        &self.order
    }

    #[allow(dead_code)]
    pub fn ids(&self) -> Vec<u32> {
        self.order.iter().map(|e| e.id).collect()
    }

    /// 0-based position of an id in the tab order.
    pub fn position(&self, id: u32) -> Option<usize> {
        self.order.iter().position(|e| e.id == id)
    }

    #[allow(dead_code)]
    pub fn profile_of(&self, id: u32) -> Option<&str> {
        self.order.iter().find(|e| e.id == id).map(|e| e.profile.as_str())
    }

    /// Add a shell at the end with the next id of this list (the unit tests use it; the program gives ids that are unique across windows).
    /// The first one becomes the active shell. `None` when the window is full.
    #[allow(dead_code)]
    pub fn add(&mut self, profile: &str) -> Option<u32> {
        if self.order.len() >= MAX_SHELLS {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.order.push(Entry { id, profile: profile.to_string() });
        if self.order.len() == 1 {
            self.active = id;
        }
        Some(id)
    }

    /// Adopt an id that already exists elsewhere (session 1 is created by the page at start).
    pub fn adopt(&mut self, id: u32, profile: &str) {
        if self.position(id).is_none() && self.order.len() < MAX_SHELLS {
            self.order.push(Entry { id, profile: profile.to_string() });
            self.next_id = self.next_id.max(id + 1);
            if self.order.len() == 1 {
                self.active = id;
            }
        }
    }

    pub fn activate(&mut self, id: u32) -> bool {
        if self.position(id).is_some() {
            self.active = id;
            true
        } else {
            false
        }
    }

    /// Move the active shell by `delta` places, wrapping around. Returns the new active id.
    pub fn step(&mut self, delta: i32) -> Option<u32> {
        let n = self.order.len() as i32;
        if n == 0 {
            return None;
        }
        let cur = self.position(self.active).unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(n) as usize;
        self.active = self.order[next].id;
        Some(self.active)
    }

    /// Make the `n`-th shell (1-based, as the jump hotkeys number them) active.
    pub fn jump(&mut self, n: usize) -> Option<u32> {
        if n == 0 || n > self.order.len() {
            return None;
        }
        self.active = self.order[n - 1].id;
        Some(self.active)
    }

    /// Remove a shell. If it was the active one, the right-hand neighbour (or the left one at the end)
    /// takes over. Returns the id that is active afterwards, `None` when the list is now empty.
    pub fn remove(&mut self, id: u32) -> Option<u32> {
        let i = self.position(id)?;
        self.order.remove(i);
        if self.pending_close.map(|(p, _)| p) == Some(id) {
            self.pending_close = None;
        }
        if self.order.is_empty() {
            self.active = 0;
            return None;
        }
        if self.active == id {
            let j = i.min(self.order.len() - 1);
            self.active = self.order[j].id;
        }
        Some(self.active)
    }

    /// The close hotkey: the first press asks for confirmation, a second press on the same shell within
    /// `CLOSE_CONFIRM_MS` closes it. A press for a different shell starts over.
    pub fn request_close(&mut self, id: u32, now_ms: u64) -> CloseStep {
        if let Some((p, t)) = self.pending_close
            && p == id && now_ms.saturating_sub(t) <= CLOSE_CONFIRM_MS {
                self.pending_close = None;
                return CloseStep::Close;
            }
        self.pending_close = Some((id, now_ms));
        CloseStep::Confirm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Shells {
        let mut s = Shells::new();
        for _ in 0..3 {
            s.add("pwsh");
        }
        s
    }

    #[test]
    fn the_first_shell_is_active_and_ids_never_repeat() {
        let mut s = Shells::new();
        assert_eq!(s.add("pwsh"), Some(1));
        assert_eq!(s.active(), 1);
        assert_eq!(s.add("claude"), Some(2));
        assert_eq!(s.active(), 1, "adding does not switch");
        s.remove(2);
        assert_eq!(s.add("pwsh"), Some(3), "a closed shell's id is not reused");
    }

    #[test]
    fn the_window_holds_at_most_nine() {
        let mut s = Shells::new();
        for _ in 0..MAX_SHELLS {
            assert!(s.add("pwsh").is_some());
        }
        assert_eq!(s.add("pwsh"), None);
        assert_eq!(s.len(), 9);
    }

    #[test]
    fn stepping_wraps_in_both_directions() {
        let mut s = three();
        assert_eq!(s.step(1), Some(2));
        assert_eq!(s.step(1), Some(3));
        assert_eq!(s.step(1), Some(1));
        assert_eq!(s.step(-1), Some(3));
        assert_eq!(Shells::new().step(1), None);
    }

    #[test]
    fn jumping_is_one_based_and_range_checked() {
        let mut s = three();
        assert_eq!(s.jump(3), Some(3));
        assert_eq!(s.active(), 3);
        assert_eq!(s.jump(1), Some(1));
        assert_eq!(s.jump(0), None);
        assert_eq!(s.jump(4), None);
        assert_eq!(s.active(), 1, "a bad jump changes nothing");
    }

    #[test]
    fn removing_the_active_shell_picks_a_neighbour() {
        let mut s = three();
        s.activate(2);
        assert_eq!(s.remove(2), Some(3), "the right-hand neighbour takes over");
        s.activate(3);
        assert_eq!(s.remove(3), Some(1), "at the end, the left one does");
        assert_eq!(s.remove(1), None, "the last one leaves the list empty");
        assert!(s.is_empty());
    }

    #[test]
    fn removing_another_shell_keeps_the_active_one() {
        let mut s = three();
        s.activate(3);
        assert_eq!(s.remove(1), Some(3));
        assert_eq!(s.active(), 3);
        assert_eq!(s.ids(), vec![2, 3]);
        assert_eq!(s.remove(99), None, "unknown ids are ignored");
    }

    #[test]
    fn closing_needs_a_second_press_on_the_same_shell_in_time() {
        let mut s = three();
        assert_eq!(s.request_close(2, 1000), CloseStep::Confirm);
        assert_eq!(s.request_close(2, 2500), CloseStep::Close);
        assert_eq!(s.request_close(2, 2600), CloseStep::Confirm, "the confirmation is used up");
        assert_eq!(s.request_close(3, 2700), CloseStep::Confirm, "another shell starts over");
        assert_eq!(s.request_close(3, 2700 + CLOSE_CONFIRM_MS + 1), CloseStep::Confirm, "too late");
        assert_eq!(s.request_close(3, 2700 + CLOSE_CONFIRM_MS + 100), CloseStep::Close);
    }

    #[test]
    fn adopting_an_existing_session_id_works_once() {
        let mut s = Shells::new();
        s.adopt(1, "pwsh");
        s.adopt(1, "pwsh");
        assert_eq!(s.len(), 1);
        assert_eq!(s.add("pwsh"), Some(2));
        assert_eq!(s.profile_of(1), Some("pwsh"));
    }
}

//! Saving and restoring the open shells and the layout between runs (config `restore_session = "shells"`). Pure logic on
//! shell ids and indexes, so it can be unit-tested without a window. What is saved is the profile of every shell (in tab order),
//! the active shell and the layout; the shells themselves start again from their profiles (their folders and what was running
//! in them are not restored). With several windows the first window's session carries the others in `windows` (each with the position and
//! size its window had); a file from before that has none, and restores exactly as it did.

use serde::{Deserialize, Serialize};

use crate::layout::{Layout, Preset};
use crate::shells::MAX_SHELLS;

/// Windows one saved session may describe: the first one and up to this many others.
pub const MAX_EXTRA_WINDOWS: usize = 3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Session {
    pub version: u32,
    /// Profile name of every shell, in tab order.
    pub shells: Vec<String>,
    /// Index (into `shells`) of the active shell.
    pub active: usize,
    /// Layout preset name.
    pub layout: String,
    pub split: [u32; 2],
    /// Per pane: index into `shells`, or -1 for an empty pane.
    pub slots: Vec<i32>,
    pub focus: usize,
    /// Own-lists mode only: the shells (indexes) of each pane.
    pub groups: Vec<Vec<usize>>,
    /// Where the window was (x, y, width, height in physical pixels); `None` in files from before several windows.
    pub rect: Option<[i32; 4]>,
    /// The names given to tabs by hand, parallel to `shells` (empty strings for none); empty in files from before tab names.
    pub names: Vec<String>,
    /// The other windows (first window's session only): each one is a session of its own without further windows.
    pub windows: Vec<Session>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: 1,
            shells: Vec::new(),
            active: 0,
            layout: "single".into(),
            split: [50, 50],
            slots: vec![0],
            focus: 0,
            groups: Vec::new(),
            rect: None,
            names: Vec::new(),
            windows: Vec::new(),
        }
    }
}

/// A session with its shell indexes turned into the ids the shells have now.
#[derive(Debug, PartialEq)]
pub struct Resolved {
    pub preset: Preset,
    pub split: (u32, u32),
    pub slots: Vec<u32>,
    pub groups: Vec<Vec<u32>>,
    pub focus: usize,
    pub active: u32,
}

impl Session {
    /// The state to save: `entries` are (shell id, profile name) in tab order.
    pub fn capture(entries: &[(u32, String)], active: u32, l: &Layout) -> Session {
        let idx = |id: u32| entries.iter().position(|(i, _)| *i == id);
        let slots = l.slots().iter().map(|&id| idx(id).map(|i| i as i32).unwrap_or(-1)).collect();
        let groups = if l.own() {
            (0..l.slots().len()).map(|p| l.group(p).iter().filter_map(|&id| idx(id)).collect()).collect()
        } else {
            Vec::new()
        };
        Session {
            version: 1,
            shells: entries.iter().map(|(_, p)| p.clone()).collect(),
            active: idx(active).unwrap_or(0),
            layout: l.preset().name().to_string(),
            split: [l.split().0, l.split().1],
            slots,
            focus: l.focus(),
            groups,
            rect: None,
            names: Vec::new(),
            windows: Vec::new(),
        }
    }

    /// Read saved text; `None` unless it is a complete, consistent session (a damaged file is simply not restored).
    pub fn parse(text: &str) -> Option<Session> {
        let s: Session = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        if !s.consistent() || s.windows.len() > MAX_EXTRA_WINDOWS {
            return None;
        }
        // every other window must be a complete session too, and none of them has windows of its own
        if s.windows.iter().any(|w| !w.consistent() || !w.windows.is_empty()) {
            return None;
        }
        Some(s)
    }

    /// This one session (not the windows it lists) is complete and consistent.
    fn consistent(&self) -> bool {
        let s = self;
        if s.version != 1 || s.shells.is_empty() || s.shells.len() > MAX_SHELLS {
            return false;
        }
        let Some(preset) = Preset::parse(&s.layout) else { return false };
        if s.slots.len() != preset.panes() || s.focus >= s.slots.len() || s.active >= s.shells.len() {
            return false;
        }
        let n = s.shells.len() as i32;
        if s.slots.iter().any(|&i| i < -1 || i >= n) {
            return false;
        }
        if !s.groups.is_empty() && (s.groups.len() != s.slots.len() || s.groups.iter().flatten().any(|&i| i >= s.shells.len())) {
            return false;
        }
        if s.rect.is_some_and(|r| r[2] < 50 || r[3] < 50) {
            return false;
        }
        if !s.names.is_empty() && s.names.len() != s.shells.len() {
            return false;
        }
        true
    }

    /// `ids[i]` is the id of the shell now running for saved shell `i`.
    pub fn resolve(&self, ids: &[u32]) -> Option<Resolved> {
        if ids.len() != self.shells.len() {
            return None;
        }
        let preset = Preset::parse(&self.layout)?;
        Some(Resolved {
            preset,
            split: (self.split[0], self.split[1]),
            slots: self.slots.iter().map(|&i| if i < 0 { 0 } else { ids[i as usize] }).collect(),
            groups: self.groups.iter().map(|g| g.iter().map(|&i| ids[i]).collect()).collect(),
            focus: self.focus,
            active: *ids.get(self.active)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<(u32, String)> {
        vec![(1, "pwsh".into()), (2, "work".into()), (5, "cmd".into())]
    }

    #[test]
    fn a_session_round_trips_through_its_text() {
        let mut l = Layout::new();
        l.start(1);
        l.set_preset(Preset::Columns, &[1, 2, 5], 2);
        let s = Session::capture(&entries(), 2, &l);
        assert_eq!(s.shells, vec!["pwsh", "work", "cmd"]);
        // the focused shell (tab 2) is placed first, then the shell that was showing (tab 1): ids become tab-order indexes
        assert_eq!((s.layout.as_str(), s.active, s.slots.clone()), ("side-by-side", 1, vec![1, 0]));
        let text = serde_json::to_string_pretty(&s).unwrap();
        assert_eq!(Session::parse(&text), Some(s));
    }

    #[test]
    fn ids_come_back_in_the_new_numbering() {
        let mut l = Layout::new();
        l.start(1);
        l.set_preset(Preset::Grid, &[1, 2, 5], 5);
        let s = Session::capture(&entries(), 5, &l);
        let r = s.resolve(&[1, 2, 3]).unwrap();
        assert_eq!(r.preset, Preset::Grid);
        assert_eq!(r.active, 3, "the third saved shell is shell 3 now");
        assert_eq!(r.slots, vec![3, 1, 2, 0], "empty panes stay empty");
        assert!(s.resolve(&[1, 2]).is_none(), "a different number of shells does not fit");
    }

    #[test]
    fn own_lists_are_kept_per_pane() {
        let mut l = Layout::new();
        l.set_own(true);
        l.start(1);
        l.set_preset(Preset::Columns, &[1], 1);
        l.set_focus(1);
        l.place_new(2);
        l.place_new(5);
        let s = Session::capture(&entries(), 5, &l);
        assert_eq!(s.groups, vec![vec![0usize], vec![1, 2]]);
        let r = s.resolve(&[7, 8, 9]).unwrap();
        assert_eq!(r.groups, vec![vec![7u32], vec![8, 9]]);
    }

    #[test]
    fn tab_names_round_trip_and_must_fit_the_shells() {
        let mut s = Session { shells: vec!["a".into(), "b".into()], ..Session::default() };
        s.names = vec!["".into(), "build".into()];
        let back = Session::parse(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.names, vec!["", "build"]);
        s.names = vec!["only one".into()];
        assert!(Session::parse(&serde_json::to_string(&s).unwrap()).is_none(), "one name for two shells does not fit");
    }

    #[test]
    fn several_windows_round_trip_and_an_old_file_still_reads() {
        let mut first = Session { shells: vec!["pwsh".into(), "work".into()], ..Session::default() };
        first.rect = Some([100, 100, 700, 420]);
        let second = Session { shells: vec!["cmd".into()], rect: Some([300, 200, 640, 360]), ..Session::default() };
        first.windows = vec![second.clone()];
        let text = serde_json::to_string_pretty(&first).unwrap();
        let back = Session::parse(&text).expect("a session with two windows reads back");
        assert_eq!(back.windows.len(), 1);
        assert_eq!(back.windows[0], second);
        // a file written before several windows existed has neither field
        let old = r#"{"version":1,"shells":["a","b"],"active":1,"layout":"side-by-side","split":[50,50],"slots":[0,1],"focus":1}"#;
        let s = Session::parse(old).unwrap();
        assert!(s.windows.is_empty() && s.rect.is_none());
        // a damaged second window spoils the file; windows do not nest; there is a limit
        let mut bad = first.clone();
        bad.windows[0].shells.clear();
        assert!(Session::parse(&serde_json::to_string(&bad).unwrap()).is_none());
        let mut nested = first.clone();
        nested.windows[0].windows = vec![second.clone()];
        assert!(Session::parse(&serde_json::to_string(&nested).unwrap()).is_none());
        let mut many = first.clone();
        many.windows = vec![second.clone(); MAX_EXTRA_WINDOWS + 1];
        assert!(Session::parse(&serde_json::to_string(&many).unwrap()).is_none());
        let mut tiny = first.clone();
        tiny.rect = Some([0, 0, 10, 10]);
        assert!(Session::parse(&serde_json::to_string(&tiny).unwrap()).is_none());
    }

    #[test]
    fn damaged_or_inconsistent_text_is_not_restored() {
        assert!(Session::parse("").is_none());
        assert!(Session::parse("{ not json").is_none());
        assert!(Session::parse(r#"{"version":1,"shells":[]}"#).is_none(), "no shells");
        let ok = r#"{"version":1,"shells":["a","b"],"active":1,"layout":"side-by-side","split":[50,50],"slots":[0,1],"focus":1}"#;
        assert!(Session::parse(ok).is_some());
        assert!(Session::parse(&ok.replace("\"active\":1", "\"active\":2")).is_none(), "active shell out of range");
        assert!(Session::parse(&ok.replace("[0,1]", "[0,3]")).is_none(), "pane points at a shell that is not there");
        assert!(Session::parse(&ok.replace("[0,1]", "[0]")).is_none(), "wrong number of panes for the layout");
        assert!(Session::parse(&ok.replace("side-by-side", "diagonal")).is_none(), "unknown layout");
        assert!(Session::parse(&ok.replace("\"version\":1", "\"version\":2")).is_none(), "a version this build does not know");
        let ten: Vec<String> = (0..10).map(|i| format!("\"p{i}\"")).collect();
        assert!(Session::parse(&format!(r#"{{"version":1,"shells":[{}],"layout":"single","slots":[0]}}"#, ten.join(","))).is_none(), "more shells than a window holds");
    }
}

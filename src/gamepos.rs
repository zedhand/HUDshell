//! Window places per program: where the window sat the last time it was moved while a given program was in front (opt in, config
//! `game_positions`). Pure logic on names and rectangles, so it can be unit-tested without a window.

use serde::{Deserialize, Serialize};

/// Programs remembered at most; the oldest is forgotten first.
pub const MAX_PLACES: usize = 50;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Place {
    /// The program's executable name, lower case (for example `game.exe`). Window titles are never used.
    pub app: String,
    /// x, y, width, height in physical pixels.
    pub rect: [i32; 4],
}

/// The name a program is remembered under: the file name of its executable, lower case.
pub fn app_key(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).trim().to_lowercase()
}

/// Programs that are the desktop and its shell, not something the window should follow.
pub fn ignored(app: &str) -> bool {
    matches!(
        app,
        "" | "explorer.exe" | "searchhost.exe" | "shellexperiencehost.exe" | "startmenuexperiencehost.exe" | "textinputhost.exe" | "lockapp.exe"
    )
}

/// Remember `rect` for `app` (replacing an earlier place of the same program). Too small a rectangle is not a window anyone placed.
pub fn remember(places: &mut Vec<Place>, app: &str, rect: [i32; 4]) {
    if ignored(app) || rect[2] < 50 || rect[3] < 50 {
        return;
    }
    places.retain(|p| p.app != app);
    places.push(Place { app: app.to_string(), rect });
    while places.len() > MAX_PLACES {
        places.remove(0);
    }
}

pub fn lookup(places: &[Place], app: &str) -> Option<[i32; 4]> {
    places.iter().find(|p| p.app == app).map(|p| p.rect)
}

/// Drop entries that cannot be right (empty or ignored names, tiny rectangles, duplicates) and keep the newest `MAX_PLACES`.
pub fn sanitize(places: &mut Vec<Place>) {
    let mut out: Vec<Place> = Vec::new();
    for p in places.drain(..) {
        let app = app_key(&p.app);
        if ignored(&app) || p.rect[2] < 50 || p.rect[3] < 50 || p.rect[2] > 16000 || p.rect[3] > 16000 {
            continue;
        }
        let lim = crate::config::COORD_LIMIT;
        if !(-lim..=lim).contains(&p.rect[0]) || !(-lim..=lim).contains(&p.rect[1]) {
            continue;
        }
        out.retain(|q| q.app != app);
        out.push(Place { app, rect: p.rect });
    }
    while out.len() > MAX_PLACES {
        out.remove(0);
    }
    *places = out;
}

/// Programs kept in the exclusive-fullscreen list at most; the oldest is forgotten first.
pub const MAX_EXCLUSIVE: usize = 50;

/// Is `app` (an executable file name, lower case) on the list of programs that run in exclusive fullscreen?
pub fn is_exclusive(list: &[String], app: &str) -> bool {
    !app.is_empty() && list.iter().any(|a| a == app)
}

/// Programs that host the windows of many others (store apps, browsers, script and runtime hosts): learning one of them as
/// "exclusive fullscreen" would refuse the keyboard for everything it hosts, so they are never learned (a hand-edited entry stays).
pub fn shared_host(app: &str) -> bool {
    matches!(
        app,
        "applicationframehost.exe"
            | "wwahost.exe"
            | "dllhost.exe"
            | "rundll32.exe"
            | "msedge.exe"
            | "chrome.exe"
            | "firefox.exe"
            | "brave.exe"
            | "opera.exe"
            | "msedgewebview2.exe"
    )
}

/// Add `app` to the list (once). The desktop shell and shared hosts are never added.
pub fn remember_exclusive(list: &mut Vec<String>, app: &str) {
    if ignored(app) || shared_host(app) || is_exclusive(list, app) {
        return;
    }
    list.push(app.to_string());
    while list.len() > MAX_EXCLUSIVE {
        list.remove(0);
    }
}

/// A hand-edited list is cleaned: names are reduced to the file name in lower case; empty, ignored and duplicate names go.
pub fn sanitize_exclusive(list: &mut Vec<String>) {
    let mut out: Vec<String> = Vec::new();
    for n in list.drain(..) {
        let app = app_key(&n);
        if !ignored(&app) && !out.contains(&app) {
            out.push(app);
        }
    }
    while out.len() > MAX_EXCLUSIVE {
        out.remove(0);
    }
    *list = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exclusive_list_is_kept_clean() {
        let mut v = vec![r"C:\x\Game.EXE".to_string(), "game.exe".into(), "".into(), "explorer.exe".into(), "other.exe".into()];
        sanitize_exclusive(&mut v);
        assert_eq!(v, vec!["game.exe".to_string(), "other.exe".to_string()]);
        assert!(is_exclusive(&v, "game.exe") && !is_exclusive(&v, "x.exe") && !is_exclusive(&v, ""));
        remember_exclusive(&mut v, "game.exe");
        remember_exclusive(&mut v, "explorer.exe");
        remember_exclusive(&mut v, "new.exe");
        assert_eq!(v.len(), 3, "a known program is not added twice and the desktop shell is never added");
        for i in 0..(MAX_EXCLUSIVE + 5) {
            remember_exclusive(&mut v, &format!("g{i}.exe"));
        }
        assert_eq!(v.len(), MAX_EXCLUSIVE);
        assert!(!is_exclusive(&v, "game.exe"), "the oldest went first");
    }

    #[test]
    fn names_come_from_the_executable_file_name_only() {
        assert_eq!(app_key(r"C:\Games\Some Game\Game.EXE"), "game.exe");
        assert_eq!(app_key("game.exe"), "game.exe");
        assert!(ignored("explorer.exe") && ignored(""));
        assert!(!ignored("game.exe"));
    }

    #[test]
    fn places_are_remembered_replaced_and_capped() {
        let mut v = Vec::new();
        remember(&mut v, "a.exe", [10, 20, 600, 300]);
        remember(&mut v, "b.exe", [30, 40, 700, 400]);
        assert_eq!(lookup(&v, "a.exe"), Some([10, 20, 600, 300]));
        remember(&mut v, "a.exe", [50, 60, 640, 360]);
        assert_eq!(v.len(), 2, "the same program is replaced, not added again");
        assert_eq!(lookup(&v, "a.exe"), Some([50, 60, 640, 360]));
        assert_eq!(lookup(&v, "c.exe"), None);
        remember(&mut v, "explorer.exe", [0, 0, 600, 300]);
        remember(&mut v, "tiny.exe", [0, 0, 10, 10]);
        assert_eq!(v.len(), 2, "the desktop shell and tiny rectangles are not remembered");
        for i in 0..(MAX_PLACES + 5) {
            remember(&mut v, &format!("g{i}.exe"), [0, 0, 100, 100]);
        }
        assert_eq!(v.len(), MAX_PLACES);
        assert_eq!(lookup(&v, "a.exe"), None, "the oldest went first");
    }

    #[test]
    fn a_hand_edited_list_is_cleaned() {
        let mut v = vec![
            Place { app: r"C:\x\Game.exe".into(), rect: [1, 2, 300, 200] },
            Place { app: "game.exe".into(), rect: [5, 6, 310, 210] },
            Place { app: "".into(), rect: [0, 0, 300, 200] },
            Place { app: "bad.exe".into(), rect: [0, 0, 1, 1] },
        ];
        sanitize(&mut v);
        assert_eq!(v, vec![Place { app: "game.exe".into(), rect: [5, 6, 310, 210] }]);
    }
}

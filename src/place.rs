//! Window placement math: corner docking, size presets and magnetic edge snapping.
//! Pure functions on plain rectangles so they can be unit-tested without a window.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl Rect {
    pub fn new(l: i32, t: i32, r: i32, b: i32) -> Self {
        Self { l, t, r, b }
    }
    pub fn w(&self) -> i32 {
        self.r - self.l
    }
    pub fn h(&self) -> i32 {
        self.b - self.t
    }
}

/// Corners in clockwise order starting top-left.
pub const CORNERS: usize = 4;

/// Which corner of the work area the window is nearest to (0 = top-left, 1 = top-right,
/// 2 = bottom-right, 3 = bottom-left), judged from the window's centre.
pub fn corner_of(win: Rect, work: Rect) -> usize {
    let cx = (win.l + win.r) / 2;
    let cy = (win.t + win.b) / 2;
    let right = cx > (work.l + work.r) / 2;
    let bottom = cy > (work.t + work.b) / 2;
    match (right, bottom) {
        (false, false) => 0,
        (true, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    }
}

/// Top-left position that puts a `w` x `h` window in `corner` of the work area, `margin` pixels in.
/// A window larger than the work area is pinned to its top-left corner instead of leaving the screen.
pub fn corner_pos(corner: usize, w: i32, h: i32, work: Rect, margin: i32) -> (i32, i32) {
    let left = work.l + margin;
    let top = work.t + margin;
    let right = (work.r - margin - w).max(left);
    let bottom = (work.b - margin - h).max(top);
    match corner % CORNERS {
        0 => (left, top),
        1 => (right, top),
        2 => (right, bottom),
        _ => (left, bottom),
    }
}

/// The corner `step` places after `from` (clockwise for +1, counter-clockwise for -1).
pub fn step_corner(from: usize, step: i32) -> usize {
    ((from as i32 + step).rem_euclid(CORNERS as i32)) as usize
}

/// Index of the preset closest in area to the current size, then the next one (wrapping).
pub fn next_size(sizes: &[(i32, i32)], cur_w: i32, cur_h: i32) -> usize {
    if sizes.is_empty() {
        return 0;
    }
    let cur_area = cur_w as i64 * cur_h as i64;
    let nearest = sizes
        .iter()
        .enumerate()
        .min_by_key(|(_, (w, h))| (*w as i64 * *h as i64 - cur_area).abs())
        .map(|(i, _)| i)
        .unwrap_or(0);
    // Only advance when the window already is (about) that preset; otherwise go to it first.
    let (pw, ph) = sizes[nearest];
    let close = (pw - cur_w).abs() <= 8 && (ph - cur_h).abs() <= 8;
    if close { (nearest + 1) % sizes.len() } else { nearest }
}

/// Snap a window being dragged to the work area's edges: when an edge is within `distance` pixels of the
/// edge-plus-`margin` line, move the window so it sits exactly on it. The size never changes.
pub fn snap_rect(win: Rect, work: Rect, distance: i32, margin: i32) -> Rect {
    if distance <= 0 {
        return win;
    }
    let (w, h) = (win.w(), win.h());
    let mut x = win.l;
    let mut y = win.t;
    let (wl, wr, wt, wb) = (work.l + margin, work.r - margin, work.t + margin, work.b - margin);
    if (win.l - wl).abs() <= distance {
        x = wl;
    } else if (win.r - wr).abs() <= distance {
        x = wr - w;
    }
    if (win.t - wt).abs() <= distance {
        y = wt;
    } else if (win.b - wb).abs() <= distance {
        y = wb - h;
    }
    Rect::new(x, y, x + w, y + h)
}

#[cfg(test)]
mod tests {
    use super::*;
    const WORK: Rect = Rect { l: 0, t: 0, r: 1920, b: 1040 };

    #[test]
    fn corners_are_detected_from_the_centre() {
        assert_eq!(corner_of(Rect::new(10, 10, 600, 400), WORK), 0);
        assert_eq!(corner_of(Rect::new(1300, 10, 1900, 400), WORK), 1);
        assert_eq!(corner_of(Rect::new(1300, 700, 1900, 1030), WORK), 2);
        assert_eq!(corner_of(Rect::new(10, 700, 600, 1030), WORK), 3);
    }

    #[test]
    fn corner_positions_respect_the_margin() {
        assert_eq!(corner_pos(0, 900, 520, WORK, 8), (8, 8));
        assert_eq!(corner_pos(1, 900, 520, WORK, 8), (1920 - 8 - 900, 8));
        assert_eq!(corner_pos(2, 900, 520, WORK, 8), (1920 - 8 - 900, 1040 - 8 - 520));
        assert_eq!(corner_pos(3, 900, 520, WORK, 8), (8, 1040 - 8 - 520));
        // negative-coordinate monitor to the left of the primary
        let left = Rect::new(-1920, 0, 0, 1080);
        assert_eq!(corner_pos(1, 800, 500, left, 0), (-800, 0));
    }

    #[test]
    fn a_window_bigger_than_the_screen_is_pinned_top_left() {
        assert_eq!(corner_pos(2, 5000, 3000, WORK, 8), (8, 8));
    }

    #[test]
    fn corner_stepping_wraps_both_ways() {
        assert_eq!(step_corner(3, 1), 0);
        assert_eq!(step_corner(0, -1), 3);
        assert_eq!(step_corner(1, 1), 2);
        let mut c = 0;
        for _ in 0..4 {
            c = step_corner(c, 1);
        }
        assert_eq!(c, 0);
    }

    #[test]
    fn size_presets_cycle_and_enter_from_a_custom_size() {
        let s = [(640, 360), (900, 520), (1280, 720)];
        assert_eq!(next_size(&s, 640, 360), 1);
        assert_eq!(next_size(&s, 900, 520), 2);
        assert_eq!(next_size(&s, 1280, 720), 0);
        // a hand-resized window first goes to the nearest preset instead of skipping past it
        assert_eq!(next_size(&s, 870, 500), 1);
        assert_eq!(next_size(&[], 100, 100), 0);
    }

    #[test]
    fn dragging_near_an_edge_snaps_without_resizing() {
        let near_left_top = Rect::new(12, 14, 912, 534);
        assert_eq!(snap_rect(near_left_top, WORK, 16, 8), Rect::new(8, 8, 908, 528));
        let near_right_bottom = Rect::new(1000, 506, 1912 + 4, 1040 - 8 + 4 + 0);
        let s = snap_rect(near_right_bottom, WORK, 16, 8);
        assert_eq!((s.r, s.b), (1920 - 8, 1040 - 8));
        assert_eq!((s.w(), s.h()), (near_right_bottom.w(), near_right_bottom.h()));
        let far = Rect::new(400, 300, 1300, 820);
        assert_eq!(snap_rect(far, WORK, 16, 8), far, "far from every edge: untouched");
        assert_eq!(snap_rect(near_left_top, WORK, 0, 8), near_left_top, "distance 0 turns snapping off");
    }
}

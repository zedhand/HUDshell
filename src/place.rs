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

/// The size presets as they can really be shown: each cut to the work area, and a preset that ends up the same size as the one before it left out
/// (so the size hotkey always has a next size to go to, and a window that fills the work area is recognised as the preset it was cut from).
pub fn fit_sizes(sizes: &[(i32, i32)], work: Rect) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = Vec::new();
    for &(w, h) in sizes {
        let s = (w.min(work.w()), h.min(work.h()));
        if out.last() != Some(&s) {
            out.push(s);
        }
    }
    out
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

/// Corner snap: when the window is within `zone` pixels of a corner of the work area on BOTH axes, it goes flush into that corner
/// (no margin). `None` when it is not near a corner, or `zone` is 0.
pub fn corner_snap(win: Rect, work: Rect, zone: i32) -> Option<Rect> {
    if zone <= 0 {
        return None;
    }
    let (w, h) = (win.w(), win.h());
    // near an edge means close to it on either side: a window far beyond an edge (on its way to another screen) is not near it
    let left = (win.l - work.l).abs() <= zone;
    let right = (work.r - win.r).abs() <= zone;
    let top = (win.t - work.t).abs() <= zone;
    let bottom = (work.b - win.b).abs() <= zone;
    if !(left || right) || !(top || bottom) {
        return None;
    }
    let x = if left { work.l } else { work.r - w };
    let y = if top { work.t } else { work.b - h };
    Some(Rect::new(x, y, x + w, y + h))
}

/// The window touches a corner of the work area (within 1 px on both axes), or fills it: the corner of the window is then a corner of the
/// screen's usable area and should be square, not rounded.
pub fn touches_corner(win: Rect, work: Rect) -> bool {
    (win.l <= work.l + 1 || win.r >= work.r - 1) && (win.t <= work.t + 1 || win.b >= work.b - 1)
}

/// Snap a dragged window to the edges of other HUDshell windows: it goes flush against one (its left edge to the other's right edge, and so on)
/// when within `distance` pixels and next to it along the other axis, and then lines up with that window's top / bottom or left / right edge
/// when those are within `distance` too. The size never changes.
pub fn snap_to_rects(win: Rect, others: &[Rect], distance: i32) -> Rect {
    if distance <= 0 || others.is_empty() {
        return win;
    }
    let (w, h) = (win.w(), win.h());
    let near_v = |o: &Rect| win.t < o.b + distance && win.b > o.t - distance; // vertical ranges (nearly) overlap
    let near_h = |o: &Rect| win.l < o.r + distance && win.r > o.l - distance;
    // against a side: nearest candidate within distance (distance, new left, index of the window)
    let mut x_adj: Option<(i32, i32, usize)> = None;
    let mut y_adj: Option<(i32, i32, usize)> = None;
    for (i, o) in others.iter().enumerate() {
        if near_v(o) {
            for (d, x) in [((win.l - o.r).abs(), o.r), ((win.r - o.l).abs(), o.l - w)] {
                if d <= distance && x_adj.is_none_or(|(bd, _, _)| d < bd) {
                    x_adj = Some((d, x, i));
                }
            }
        }
        if near_h(o) {
            for (d, y) in [((win.t - o.b).abs(), o.b), ((win.b - o.t).abs(), o.t - h)] {
                if d <= distance && y_adj.is_none_or(|(bd, _, _)| d < bd) {
                    y_adj = Some((d, y, i));
                }
            }
        }
    }
    let mut x = x_adj.map_or(win.l, |(_, x, _)| x);
    let mut y = y_adj.map_or(win.t, |(_, y, _)| y);
    // lined up with the window it was put against
    if let Some((_, _, i)) = x_adj {
        let o = &others[i];
        if (win.t - o.t).abs() <= distance {
            y = o.t;
        } else if (win.b - o.b).abs() <= distance {
            y = o.b - h;
        }
    }
    if let Some((_, _, i)) = y_adj {
        let o = &others[i];
        if (win.l - o.l).abs() <= distance {
            x = o.l;
        } else if (win.r - o.r).abs() <= distance {
            x = o.r - w;
        }
    }
    Rect::new(x, y, x + w, y + h)
}

/// Where a window goes when its monitor is gone: its size is scaled to the scaling of the monitor it lands on (so it keeps its logical size), cut
/// to the work area, and it sits at the top-left corner, each window of the same rescue `index` further along the diagonal by `step` pixels so
/// they do not hide each other.
pub fn rescue_rect(size: (i32, i32), dpi: (i32, i32), work: Rect, margin: i32, index: i32, step: i32) -> Rect {
    let ((w, h), (old_dpi, new_dpi)) = (size, dpi);
    let (w, h) = if old_dpi > 0 && new_dpi > 0 { (w * new_dpi / old_dpi, h * new_dpi / old_dpi) } else { (w, h) };
    let (w, h) = (w.min(work.w()).max(1), h.min(work.h()).max(1));
    let (x0, y0) = corner_pos(0, w, h, work, margin);
    let off = (index * step).min((work.w() - w).max(0)).min((work.h() - h).max(0));
    Rect::new(x0 + off, y0 + off, x0 + off + w, y0 + off + h)
}

/// Two windows touch: an edge of one lies on the opposite edge of the other (within `tol` pixels) and they overlap along that edge.
pub fn touching(a: Rect, b: Rect, tol: i32) -> bool {
    let side = ((a.r - b.l).abs() <= tol || (a.l - b.r).abs() <= tol) && a.t < b.b && a.b > b.t;
    let stack = ((a.b - b.t).abs() <= tol || (a.t - b.b).abs() <= tol) && a.l < b.r && a.r > b.l;
    side || stack
}

/// The windows that move with window `start`: everything that touches it, directly or through other touching windows (not `start` itself).
pub fn group_of(rects: &[Rect], start: usize, tol: i32) -> Vec<usize> {
    let mut seen = vec![false; rects.len()];
    let mut order = vec![start];
    if start < seen.len() {
        seen[start] = true;
    }
    let mut k = 0;
    while k < order.len() {
        let cur = order[k];
        k += 1;
        for j in 0..rects.len() {
            if !seen[j] && touching(rects[cur], rects[j], tol) {
                seen[j] = true;
                order.push(j);
            }
        }
    }
    order.into_iter().filter(|&i| i != start).collect()
}

/// Snapping while a window is being dragged. Windows proposes every new rectangle relative to the one that was
/// *last applied*, not to where the drag began. If the snapped rectangle were fed back as the new position, a window
/// sitting on an edge could never be pulled away (each step is smaller than the snap distance and lands inside it
/// again). So this keeps the position the window would have without snapping, and snaps that.
pub struct SnapDrag {
    virt: Rect,
    last: Rect,
}

impl SnapDrag {
    pub fn new(start: Rect) -> Self {
        Self { virt: start, last: start }
    }

    /// Where the window would be for this proposal without any snapping (used to pick the screen the drag is heading for).
    pub fn peek(&self, proposed: Rect) -> Rect {
        let (dx, dy) = (proposed.l - self.last.l, proposed.t - self.last.t);
        Rect::new(self.virt.l + dx, self.virt.t + dy, self.virt.r + dx, self.virt.b + dy)
    }

    /// The rectangle to apply for the proposed one. `others` are the other HUDshell windows the dragged one may snap to; a screen edge
    /// wins over a window on the same axis.
    pub fn step(&mut self, proposed: Rect, work: Rect, distance: i32, margin: i32, others: &[Rect]) -> Rect {
        let (dx, dy) = (proposed.l - self.last.l, proposed.t - self.last.t);
        self.virt = Rect::new(self.virt.l + dx, self.virt.t + dy, self.virt.r + dx, self.virt.b + dy);
        // a corner zone three times the snap distance (plus the margin) pulls the window flush into the corner
        let mut out = corner_snap(self.virt, work, distance * 3 + margin).unwrap_or_else(|| snap_rect(self.virt, work, distance, margin));
        let win = snap_to_rects(self.virt, others, distance);
        if out.l == self.virt.l && win.l != self.virt.l {
            out = Rect::new(win.l, out.t, win.l + out.w(), out.b);
        }
        if out.t == self.virt.t && win.t != self.virt.t {
            out = Rect::new(out.l, win.t, out.r, win.t + out.h());
        }
        self.last = out;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const WORK: Rect = Rect { l: 0, t: 0, r: 1920, b: 1040 };

    #[test]
    fn a_slow_drag_gets_away_from_an_edge() {
        // 8 px steps downwards from a window flush at the top margin; the snap distance is 16, so a naive snap would pin it forever.
        let mut d = SnapDrag::new(Rect::new(100, 8, 740, 368));
        let mut applied = Rect::new(100, 8, 740, 368);
        let mut tops = vec![];
        for _ in 0..6 {
            // what Windows proposes: the last applied rectangle moved by the cursor's step
            let proposed = Rect::new(applied.l, applied.t + 8, applied.r, applied.b + 8);
            applied = d.step(proposed, WORK, 16, 8, &[]);
            tops.push(applied.t);
        }
        assert_eq!(tops[0], 8, "still held by the magnet after 8 px");
        assert_eq!(tops[1], 8, "and after 16 px");
        assert!(tops[2] > 8, "released once past the distance: {tops:?}");
        assert_eq!(*tops.last().unwrap(), 8 + 6 * 8, "and then it follows the cursor exactly: {tops:?}");
    }

    #[test]
    fn a_drag_towards_a_corner_snaps_into_it_on_both_axes() {
        let start = Rect::new(500, 500, 1140, 860);
        let mut d = SnapDrag::new(start);
        let mut applied = start;
        // the cursor path ends 10 px short of the bottom-right margin line on each axis
        let (gx, gy) = (1920 - 8 - 10 - 1140, 1040 - 8 - 10 - 860);
        let mut prev = (0, 0);
        for i in 1..=10 {
            let off = (gx * i / 10, gy * i / 10);
            let (dx, dy) = (off.0 - prev.0, off.1 - prev.1);
            prev = off;
            // what Windows proposes: the last applied rectangle moved by this step
            let proposed = Rect::new(applied.l + dx, applied.t + dy, applied.r + dx, applied.b + dy);
            applied = d.step(proposed, WORK, 16, 8, &[]);
        }
        assert_eq!((applied.r, applied.b), (1920, 1040), "flush into the corner, no margin: {applied:?}");
        assert_eq!((applied.w(), applied.h()), (640, 360), "the size never changes");
        assert!(touches_corner(applied, WORK));
    }

    #[test]
    fn a_window_far_beyond_an_edge_is_not_pulled_into_that_corner() {
        let work = Rect::new(2560, 913, 4480, 1345);
        // up and to the left of the work area, on its way to another screen: not near its top-left corner
        assert_eq!(corner_snap(Rect::new(1000, 400, 1640, 760), work, 56), None);
        assert_eq!(corner_snap(Rect::new(2530, 880, 3170, 1240), work, 56), Some(Rect::new(2560, 913, 3200, 1273)), "a few pixels beyond the edge still snaps");
        assert_eq!(corner_snap(Rect::new(2363, 849, 3003, 1209), work, 56), None, "200 pixels beyond it does not");
        // below and to the right
        assert_eq!(corner_snap(Rect::new(5200, 1500, 5840, 1860), work, 56), None);
    }

    #[test]
    fn corner_zone_pulls_flush_and_squares_the_corner() {
        let w = Rect::new(30, 40, 670, 400);
        assert_eq!(corner_snap(w, WORK, 56), Some(Rect::new(0, 0, 640, 360)));
        assert_eq!(corner_snap(Rect::new(1250, 700, 1890, 1060), WORK, 56), Some(Rect::new(1280, 680, 1920, 1040)));
        assert_eq!(corner_snap(Rect::new(30, 300, 670, 660), WORK, 56), None, "near only one edge");
        assert_eq!(corner_snap(w, WORK, 0), None, "zone 0 turns it off");
        assert!(touches_corner(Rect::new(0, 0, 640, 360), WORK));
        assert!(touches_corner(WORK, WORK), "a maximized window touches every corner");
        assert!(!touches_corner(Rect::new(8, 8, 648, 368), WORK), "a window inset by the margin keeps rounded corners");
        assert!(!touches_corner(Rect::new(0, 300, 640, 660), WORK), "on one edge only");
    }

    #[test]
    fn a_window_snaps_flush_against_another_and_lines_up_with_it() {
        let other = Rect::new(300, 300, 700, 560);
        // 6 px to the right of it, 5 px lower: flush against its right edge, same top
        let w = Rect::new(706, 305, 1106, 565);
        assert_eq!(snap_to_rects(w, &[other], 16), Rect::new(700, 300, 1100, 560));
        // 8 px under it, 4 px to the left: flush under it, same left edge
        let w = Rect::new(296, 568, 696, 828);
        assert_eq!(snap_to_rects(w, &[other], 16), Rect::new(300, 560, 700, 820));
        // against its left side
        let w = Rect::new(-110, 297, 290, 557);
        assert_eq!(snap_to_rects(w, &[other], 16), Rect::new(-100, 300, 300, 560));
        // too far, or not next to it at all
        let far = Rect::new(760, 300, 1160, 560);
        assert_eq!(snap_to_rects(far, &[other], 16), far);
        let apart = Rect::new(706, 700, 1106, 960);
        assert_eq!(snap_to_rects(apart, &[other], 16), apart, "beside it in x but a whole window lower: not snapped");
        assert_eq!(snap_to_rects(w, &[], 16), w);
        assert_eq!(snap_to_rects(w, &[other], 0), w, "distance 0 turns it off");
    }

    #[test]
    fn a_drag_snaps_to_another_window_but_a_screen_edge_wins_on_its_axis() {
        let other = Rect::new(500, 400, 900, 660);
        let start = Rect::new(1000, 405, 1400, 665);
        let mut d = SnapDrag::new(start);
        // 6 px right of the other window's right edge: flush against it
        let applied = d.step(Rect::new(906, 405, 1306, 665), WORK, 16, 8, &[other]);
        assert_eq!((applied.l, applied.t), (900, 400));
        // next to the left screen edge and the other window at once: the screen edge wins horizontally
        let mut d = SnapDrag::new(Rect::new(200, 300, 400, 500));
        let applied = d.step(Rect::new(10, 300, 210, 500), WORK, 16, 8, &[Rect::new(220, 300, 420, 500)]);
        assert_eq!(applied.l, 8, "the screen margin wins over the window beside it");
    }

    #[test]
    fn a_rescued_window_keeps_its_logical_size_and_does_not_hide_the_others() {
        let work = Rect::new(0, 0, 2560, 1440);
        // 700x420 (logical) on a 150% monitor is 1050x630 pixels; on a 100% monitor it is 700x420 again
        assert_eq!(rescue_rect((1050, 630), (144, 96), work, 12, 0, 36), Rect::new(12, 12, 712, 432));
        assert_eq!(rescue_rect((1050, 630), (144, 96), work, 12, 2, 36), Rect::new(84, 84, 784, 504), "the third window of the rescue is two steps along");
        // onto a bigger scaling the window grows with it; one too big for the screen is cut to it
        assert_eq!(rescue_rect((700, 420), (96, 144), work, 0, 0, 36), Rect::new(0, 0, 1050, 630));
        assert_eq!(rescue_rect((5000, 4000), (96, 96), work, 0, 0, 36), Rect::new(0, 0, 2560, 1440));
        assert_eq!(rescue_rect((700, 420), (0, 96), work, 0, 0, 36), Rect::new(0, 0, 700, 420), "an unknown scaling leaves the size alone");
    }

    #[test]
    fn windows_that_touch_move_together() {
        let a = Rect::new(0, 0, 400, 300);
        let b = Rect::new(400, 0, 800, 300); // beside a
        let c = Rect::new(400, 300, 800, 600); // under b
        let far = Rect::new(1000, 0, 1400, 300);
        let corner = Rect::new(800, 300, 1000, 500); // only touches c at a corner
        assert!(touching(a, b, 1) && touching(b, c, 1));
        assert!(!touching(a, c, 1), "a and c only meet at a point");
        assert!(!touching(a, far, 1));
        assert!(touching(c, corner, 1) && touching(corner, c, 1), "touching is symmetric");
        assert_eq!(group_of(&[a, b, c, far], 0, 1), vec![1, 2], "through b to c");
        assert_eq!(group_of(&[a, b, c, far], 3, 1), Vec::<usize>::new());
        assert!(touching(Rect::new(0, 0, 400, 300), Rect::new(402, 10, 800, 300), 2), "within the tolerance");
    }

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
    fn presets_bigger_than_the_work_area_still_cycle() {
        // 200%: the presets are 1280x720, 1800x1040 and 2560x1440 pixels, but a visible taskbar leaves a work area 1344 high
        let work = Rect::new(0, 0, 2560, 1344);
        let sizes = fit_sizes(&[(1280, 720), (1800, 1040), (2560, 1440)], work);
        assert_eq!(sizes, vec![(1280, 720), (1800, 1040), (2560, 1344)]);
        // the window that fills the work area is the last preset, and the next press wraps to the first
        assert_eq!(next_size(&sizes, 2560, 1344), 0);
        assert_eq!(next_size(&sizes, 1280, 720), 1);
        // two presets that end up the same size are one
        assert_eq!(fit_sizes(&[(1280, 720), (3000, 2000), (4000, 3000)], work), vec![(1280, 720), (2560, 1344)]);
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
        let near_right_bottom = Rect::new(1000, 506, 1912 + 4, 1040 - 8 + 4);
        let s = snap_rect(near_right_bottom, WORK, 16, 8);
        assert_eq!((s.r, s.b), (1920 - 8, 1040 - 8));
        assert_eq!((s.w(), s.h()), (near_right_bottom.w(), near_right_bottom.h()));
        let far = Rect::new(400, 300, 1300, 820);
        assert_eq!(snap_rect(far, WORK, 16, 8), far, "far from every edge: untouched");
        assert_eq!(snap_rect(near_left_top, WORK, 0, 8), near_left_top, "distance 0 turns snapping off");
    }
}

//! Per-window state for several HUDshell windows in one process.
//!
//! Every window has a serial number (not its HWND, which Windows may reuse). A `PerWin<T>` static holds one `T` per window and
//! dereferences to the one of the *current* window, so code written for a single window keeps its names and call sites:
//! `CLICK_THROUGH.load(..)`, `shells_lock()`, and so on. Everything runs on the UI thread; the current window is set on entry to the
//! window procedure and to each page message (`enter` / `enter_hwnd`), and put back when the guard is dropped.

use std::cell::Cell;
use std::ops::Deref;
use std::sync::Mutex;

/// One open window: serial number and handle, in creation order.
static WINDOWS: Mutex<Vec<(u32, isize)>> = Mutex::new(Vec::new());
static NEXT_SERIAL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

thread_local! {
    static CUR: Cell<u32> = const { Cell::new(0) };
}

fn windows_lock() -> std::sync::MutexGuard<'static, Vec<(u32, isize)>> {
    WINDOWS.lock().unwrap_or_else(|p| p.into_inner())
}

/// Most windows one process opens.
pub const MAX_WINDOWS: usize = 4;

/// Register a new window and return its serial number. The first one is the primary window.
pub fn register(hwnd: isize) -> u32 {
    let serial = NEXT_SERIAL.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    windows_lock().push((serial, hwnd));
    serial
}

/// Remove a window from the table (its per-window values are dropped with it by `forget_all`).
pub fn unregister(hwnd: isize) -> Option<u32> {
    let mut w = windows_lock();
    let i = w.iter().position(|(_, h)| *h == hwnd)?;
    Some(w.remove(i).0)
}

pub fn serial_of(hwnd: isize) -> Option<u32> {
    windows_lock().iter().find(|(_, h)| *h == hwnd).map(|(s, _)| *s)
}

pub fn hwnd_of(serial: u32) -> Option<isize> {
    windows_lock().iter().find(|(s, _)| *s == serial).map(|(_, h)| *h)
}

/// Every open window (serial, handle), oldest first.
pub fn all() -> Vec<(u32, isize)> {
    windows_lock().clone()
}

pub fn count() -> usize {
    windows_lock().len()
}

/// The serial number of the window code is running for (the primary window when none was entered).
pub fn current() -> u32 {
    let c = CUR.with(|c| c.get());
    if c != 0 {
        return c;
    }
    windows_lock().first().map(|(s, _)| *s).unwrap_or(1)
}

pub fn current_hwnd() -> isize {
    hwnd_of(current()).unwrap_or(0)
}

/// Restores the previous current window when dropped.
pub struct Guard(u32);

impl Drop for Guard {
    fn drop(&mut self) {
        CUR.with(|c| c.set(self.0));
    }
}

/// Make window `serial` the current one until the guard is dropped.
pub fn enter(serial: u32) -> Guard {
    let prev = CUR.with(|c| c.replace(serial));
    Guard(prev)
}

/// Make the window with this handle the current one (a handle that is not registered keeps the current window).
pub fn enter_hwnd(hwnd: isize) -> Guard {
    match serial_of(hwnd) {
        Some(s) => enter(s),
        None => Guard(CUR.with(|c| c.get())),
    }
}

/// A value per window. `T` must be usable from a static (`Sync`), like an atomic or a `Mutex`.
pub struct PerWin<T: 'static> {
    init: fn() -> T,
    slots: Mutex<Vec<(u32, &'static T)>>,
}

impl<T: 'static> PerWin<T> {
    pub const fn new(init: fn() -> T) -> Self {
        Self { init, slots: Mutex::new(Vec::new()) }
    }

    /// The value of window `serial` (made from the initial value on first use).
    pub fn of(&'static self, serial: u32) -> &'static T {
        let mut s = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((_, v)) = s.iter().find(|(k, _)| *k == serial) {
            return v;
        }
        // Leaked on purpose: a window's few values live for the rest of the run (at most MAX_WINDOWS windows are ever opened at once).
        let v: &'static T = Box::leak(Box::new((self.init)()));
        s.push((serial, v));
        v
    }

    /// Start window `serial` from the initial value again (its number is new, so this only matters for tests of reuse).
    #[allow(dead_code)]
    pub fn forget(&self, serial: u32) {
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).retain(|(k, _)| *k != serial);
    }
}

impl<T: 'static> Deref for PerWin<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // The statics live for the whole run, so borrowing `self` as 'static is what they are.
        let me: &'static PerWin<T> = unsafe { &*(self as *const PerWin<T>) };
        me.of(current())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: PerWin<AtomicU32> = PerWin::new(|| AtomicU32::new(7));

    #[test]
    fn each_window_has_its_own_value_and_the_current_one_is_restored() {
        let a = register(0x1001);
        let b = register(0x1002);
        {
            let _g = enter(a);
            COUNTER.store(1, Ordering::SeqCst);
            {
                let _g2 = enter(b);
                assert_eq!(COUNTER.load(Ordering::SeqCst), 7, "a new window starts from the initial value");
                COUNTER.store(2, Ordering::SeqCst);
            }
            assert_eq!(COUNTER.load(Ordering::SeqCst), 1, "back in the first window after the inner guard");
        }
        assert_eq!(COUNTER.of(a).load(Ordering::SeqCst), 1);
        assert_eq!(COUNTER.of(b).load(Ordering::SeqCst), 2);
        assert_eq!(serial_of(0x1002), Some(b));
        assert_eq!(unregister(0x1001), Some(a));
        assert_eq!(serial_of(0x1001), None);
        unregister(0x1002);
    }
}

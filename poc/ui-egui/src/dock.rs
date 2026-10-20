//! When a panel leaves the main window and when it docks back.
//!
//! All rectangles are in monitor space, in egui points.

use eframe::egui::{Pos2, Rect, Vec2, vec2};

/// How far past the window edge the pointer must go before a panel detaches.
pub const DETACH_MARGIN: f32 = 8.0;
/// How far a floating window must move before it may dock again. Stops a
/// panel detached with the keyboard from docking right away.
pub const MOVE_THRESHOLD: f32 = 4.0;
/// Distance from the top of a floating window to the middle of its title bar.
pub const TITLE_BAR_PROBE: f32 = 12.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Placement {
    #[default]
    Docked,
    Floating {
        /// Where the window was first seen; `None` until the OS reports it.
        first_seen: Option<Pos2>,
    },
}

impl Placement {
    pub fn floating() -> Self {
        Self::Floating { first_seen: None }
    }

    pub fn is_floating(self) -> bool {
        matches!(self, Self::Floating { .. })
    }
}

/// True when a dragged pointer has left the main window.
pub fn pointer_left_window(window: Rect, pointer: Pos2) -> bool {
    !window.expand(DETACH_MARGIN).contains(pointer)
}

/// Top-left position for a newly detached window so it appears under the
/// pointer, keeping the grab point where the user grabbed the panel.
pub fn detached_position(pointer_on_monitor: Pos2, grab_offset: Vec2) -> Pos2 {
    pointer_on_monitor - grab_offset - vec2(0.0, TITLE_BAR_PROBE)
}

/// True when a floating window should dock back: the user moved it and its
/// title bar is now over the main window.
pub fn should_dock(main_window: Rect, floating: Rect, first_seen: Pos2) -> bool {
    let moved = (floating.min - first_seen).length() > MOVE_THRESHOLD;
    let title_bar = floating.center_top() + vec2(0.0, TITLE_BAR_PROBE);
    moved && main_window.contains(title_bar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    fn main_window() -> Rect {
        Rect::from_min_size(pos2(100.0, 100.0), vec2(1200.0, 800.0))
    }

    #[test]
    fn pointer_inside_or_near_edge_does_not_detach() {
        assert!(!pointer_left_window(main_window(), pos2(500.0, 500.0)));
        assert!(!pointer_left_window(main_window(), pos2(95.0, 500.0)));
    }

    #[test]
    fn pointer_past_edge_detaches() {
        assert!(pointer_left_window(main_window(), pos2(80.0, 500.0)));
        assert!(pointer_left_window(main_window(), pos2(500.0, 950.0)));
        assert!(pointer_left_window(main_window(), pos2(2000.0, 500.0)));
    }

    #[test]
    fn detached_window_keeps_grab_point_under_pointer() {
        let pos = detached_position(pos2(1500.0, 300.0), vec2(40.0, 10.0));
        assert_eq!(pos, pos2(1460.0, 290.0 - TITLE_BAR_PROBE));
    }

    #[test]
    fn moved_window_over_main_docks() {
        let first = pos2(1400.0, 300.0);
        let floating = Rect::from_min_size(pos2(400.0, 300.0), vec2(260.0, 400.0));
        assert!(should_dock(main_window(), floating, first));
    }

    #[test]
    fn window_that_did_not_move_stays_floating() {
        let first = pos2(400.0, 300.0);
        let floating = Rect::from_min_size(first, vec2(260.0, 400.0));
        assert!(!should_dock(main_window(), floating, first));
    }

    #[test]
    fn window_on_other_monitor_stays_floating() {
        let first = pos2(1400.0, 300.0);
        let floating = Rect::from_min_size(pos2(2200.0, 300.0), vec2(260.0, 400.0));
        assert!(!should_dock(main_window(), floating, first));
    }

    #[test]
    fn placement_helpers() {
        assert!(Placement::floating().is_floating());
        assert!(!Placement::Docked.is_floating());
    }
}

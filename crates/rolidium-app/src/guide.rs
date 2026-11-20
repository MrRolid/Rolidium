// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The guide of the first steps: a chain of short hints that leads a person without 3D experience to the first body.
//! This module is the state machine only (no drawing): which hint is due, given what the person has done. The window code
//! (`app/guide_ui.rs`) shows it.
//!
//! The progress is a set of milestones that are only ever added: drew a closed shape, started the Pull tool, made a body,
//! finished the last hint. The due hint is the one of the first milestone that is missing. A hint can be closed (it does not
//! come back until the next milestone is reached); the whole guide can be switched off, and started again from the About window.

/// The hints, in the order they appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    /// No shape yet: click for the centre of the circle, then for its size.
    Draw,
    /// A closed shape exists: press P to pull it into space (the Pull button pulses).
    PressPull,
    /// In the Pull tool: grab the shape and drag, or type the height.
    UsePull,
    /// The first body exists: how to turn, move and zoom the view, and that every flat face can be drawn on.
    Navigate,
}

pub const M_SHAPE: u8 = 1;
pub const M_PULL: u8 = 2;
pub const M_BODY: u8 = 4;
pub const M_DONE: u8 = 8;
const ALL: u8 = M_SHAPE | M_PULL | M_BODY | M_DONE;

/// What the program tells the guide about the document.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
    /// A closed shape (a sketch region) exists.
    pub closed_shape: bool,
    /// The Pull tool is the active tool (or a pull is going on).
    pub pull_tool: bool,
    /// A body exists.
    pub body: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guide {
    /// Milestones reached (remembered in the settings).
    pub mask: u8,
    /// Switched off by the person ("Do not show the guide again").
    pub enabled: bool,
    /// The guide is running in this document: a new empty document started it (an opened file does not).
    pub active: bool,
    /// The hint of this milestone was closed.
    closed: Option<Hint>,
}

impl Guide {
    pub fn new(mask: u8, enabled: bool) -> Guide {
        Guide {
            mask: mask & ALL,
            enabled,
            active: false,
            closed: None,
        }
    }

    pub fn finished(&self) -> bool {
        self.mask & M_DONE != 0
    }

    /// The hint that is due now, if any.
    pub fn due(&self) -> Option<Hint> {
        if !self.enabled || !self.active || self.finished() {
            return None;
        }
        let h = if self.mask & M_SHAPE == 0 {
            Hint::Draw
        } else if self.mask & M_PULL == 0 {
            Hint::PressPull
        } else if self.mask & M_BODY == 0 {
            Hint::UsePull
        } else {
            Hint::Navigate
        };
        (self.closed != Some(h)).then_some(h)
    }

    /// Takes in what the person did. Returns true when the mask changed (to be saved).
    pub fn observe(&mut self, f: Facts) -> bool {
        if !self.active || !self.enabled || self.finished() {
            return false;
        }
        let before = self.mask;
        // a body implies the earlier steps: somebody who made a body by other means needs no hints about drawing
        if f.body {
            self.mask |= M_SHAPE | M_PULL | M_BODY;
        }
        if f.closed_shape {
            self.mask |= M_SHAPE;
        }
        if f.pull_tool && self.mask & M_SHAPE != 0 {
            self.mask |= M_PULL;
        }
        if self.mask != before {
            // a new milestone: its hint is shown even when the previous one was closed
            self.closed = None;
        }
        self.mask != before
    }

    /// The close button of the shown hint. The last hint ends the guide.
    pub fn close(&mut self) -> bool {
        match self.due() {
            Some(Hint::Navigate) => {
                self.mask |= M_DONE;
                true
            }
            Some(h) => {
                self.closed = Some(h);
                false
            }
            None => false,
        }
    }

    /// "Do not show the guide again".
    pub fn switch_off(&mut self) {
        self.enabled = false;
    }

    /// "Run the guide again" (About window): forgets the progress, switches the guide on and starts it.
    pub fn restart(&mut self) {
        self.mask = 0;
        self.enabled = true;
        self.active = true;
        self.closed = None;
    }

    /// A new empty document starts the guide (unless it is finished or off); an opened file stops it.
    pub fn document_started(&mut self, empty_new: bool) {
        self.active = empty_new;
        self.closed = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> Guide {
        let mut g = Guide::new(0, true);
        g.document_started(true);
        g
    }

    #[test]
    fn the_hints_come_in_order_as_the_person_acts() {
        let mut g = running();
        assert_eq!(g.due(), Some(Hint::Draw));
        assert!(!g.observe(Facts::default()));
        assert!(g.observe(Facts { closed_shape: true, ..Facts::default() }));
        assert_eq!(g.due(), Some(Hint::PressPull));
        assert!(g.observe(Facts { closed_shape: true, pull_tool: true, ..Facts::default() }));
        assert_eq!(g.due(), Some(Hint::UsePull));
        assert!(g.observe(Facts { body: true, ..Facts::default() }));
        assert_eq!(g.due(), Some(Hint::Navigate));
        assert!(g.close());
        assert_eq!(g.due(), None);
        assert!(g.finished());
    }

    #[test]
    fn a_closed_hint_stays_closed_until_the_next_step() {
        let mut g = running();
        g.close();
        assert_eq!(g.due(), None);
        g.observe(Facts { closed_shape: true, ..Facts::default() });
        assert_eq!(g.due(), Some(Hint::PressPull), "the next hint is shown even though the one before was closed");
    }

    #[test]
    fn switching_off_and_running_again() {
        let mut g = running();
        g.switch_off();
        assert_eq!(g.due(), None);
        assert!(!g.observe(Facts { closed_shape: true, ..Facts::default() }), "nothing is recorded while it is off");
        g.restart();
        assert_eq!(g.mask, 0);
        assert_eq!(g.due(), Some(Hint::Draw));
    }

    #[test]
    fn an_opened_file_does_not_start_the_guide_and_a_new_document_does() {
        let mut g = Guide::new(0, true);
        g.document_started(false);
        assert_eq!(g.due(), None);
        g.document_started(true);
        assert_eq!(g.due(), Some(Hint::Draw));
    }

    #[test]
    fn the_progress_is_remembered_and_a_finished_guide_stays_finished() {
        let g = Guide::new(M_SHAPE | M_PULL, true);
        let mut g2 = g.clone();
        g2.document_started(true);
        assert_eq!(g2.due(), Some(Hint::UsePull), "continues where it stopped");
        let mut done = Guide::new(ALL, true);
        done.document_started(true);
        assert_eq!(done.due(), None);
        assert_eq!(Guide::new(255, true).mask, ALL, "unknown bits are dropped");
    }

    #[test]
    fn someone_who_makes_a_body_at_once_skips_the_early_hints() {
        let mut g = running();
        g.observe(Facts { body: true, ..Facts::default() });
        assert_eq!(g.due(), Some(Hint::Navigate));
    }
}

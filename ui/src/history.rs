//! The editor's in-memory undo/redo stack (spec section 12.1). It holds
//! whole `Cv` snapshots and is separate from the database snapshots.

use cv_core::Cv;

/// At most this many undo steps are kept; the oldest go first.
pub const MAX_STEPS: usize = 100;
/// Edits closer together than this (milliseconds) are one undo step, so
/// typing a word is undone at once rather than letter by letter.
pub const GROUP_MS: f64 = 1000.0;

#[derive(Default)]
pub struct History {
    undo: Vec<Cv>,
    redo: Vec<Cv>,
    /// When the last edit was recorded, for grouping.
    last_edit_ms: Option<f64>,
}

impl History {
    /// Records the CV as it was before an edit made at `now_ms`. An edit
    /// soon after the previous one joins its step. Any edit clears redo.
    pub fn record(&mut self, before: &Cv, now_ms: f64) {
        let joins = self
            .last_edit_ms
            .is_some_and(|last| (0.0..GROUP_MS).contains(&(now_ms - last)));
        self.last_edit_ms = Some(now_ms);
        self.redo.clear();
        if joins && !self.undo.is_empty() {
            return;
        }
        self.undo.push(before.clone());
        if self.undo.len() > MAX_STEPS {
            self.undo.remove(0);
        }
    }

    /// The CV to go back to, given the current one; `None` if there is
    /// nothing to undo.
    pub fn undo(&mut self, current: &Cv) -> Option<Cv> {
        let previous = self.undo.pop()?;
        self.redo.push(current.clone());
        self.last_edit_ms = None;
        Some(previous)
    }

    /// The CV to go forward to, given the current one.
    pub fn redo(&mut self, current: &Cv) -> Option<Cv> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        self.last_edit_ms = None;
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// What a key press in the editor asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    Undo,
    Redo,
    Save,
    Export,
}

/// Maps a key press to a shortcut. `ctrl` is Ctrl, or Cmd on macOS. `key`
/// is `KeyboardEvent.key`, which is upper case while Shift is held.
pub fn shortcut(key: &str, ctrl: bool, shift: bool, alt: bool) -> Option<Shortcut> {
    if !ctrl || alt {
        return None;
    }
    match (key.to_ascii_lowercase().as_str(), shift) {
        ("z", false) => Some(Shortcut::Undo),
        ("z", true) | ("y", false) => Some(Shortcut::Redo),
        ("s", false) => Some(Shortcut::Save),
        ("e", false) => Some(Shortcut::Export),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(title: &str) -> Cv {
        let mut c: Cv =
            serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("cv");
        c.title = title.into();
        c
    }

    #[test]
    fn undo_and_redo_walk_the_steps() {
        let mut h = History::default();
        assert!(!h.can_undo());
        h.record(&cv("a"), 0.0);
        h.record(&cv("b"), 5000.0);
        // Now showing "c".
        assert_eq!(h.undo(&cv("c")).map(|c| c.title), Some("b".into()));
        assert_eq!(h.undo(&cv("b")).map(|c| c.title), Some("a".into()));
        assert!(h.undo(&cv("a")).is_none());
        assert_eq!(h.redo(&cv("a")).map(|c| c.title), Some("b".into()));
        assert_eq!(h.redo(&cv("b")).map(|c| c.title), Some("c".into()));
        assert!(!h.can_redo());
    }

    #[test]
    fn quick_edits_group_and_new_edits_clear_redo() {
        let mut h = History::default();
        h.record(&cv("a"), 0.0);
        h.record(&cv("ab"), 300.0);
        h.record(&cv("abc"), 600.0);
        assert_eq!(h.undo(&cv("abcd")).map(|c| c.title), Some("a".into()));
        assert!(h.can_redo());
        h.record(&cv("a"), 700.0);
        assert!(!h.can_redo());
        // An edit right after an undo starts its own step.
        assert_eq!(h.undo(&cv("x")).map(|c| c.title), Some("a".into()));
    }

    #[test]
    fn keeps_at_most_max_steps() {
        let mut h = History::default();
        for i in 0..=MAX_STEPS {
            h.record(&cv(&i.to_string()), i as f64 * 2.0 * GROUP_MS);
        }
        let mut last = None;
        while let Some(c) = h.undo(&cv("now")) {
            last = Some(c.title);
        }
        // Step 0 was dropped to make room.
        assert_eq!(last, Some("1".into()));
    }

    #[test]
    fn shortcuts_need_ctrl_and_no_alt() {
        assert_eq!(shortcut("z", true, false, false), Some(Shortcut::Undo));
        assert_eq!(shortcut("Z", true, true, false), Some(Shortcut::Redo));
        assert_eq!(shortcut("y", true, false, false), Some(Shortcut::Redo));
        assert_eq!(shortcut("s", true, false, false), Some(Shortcut::Save));
        assert_eq!(shortcut("e", true, false, false), Some(Shortcut::Export));
        assert_eq!(shortcut("z", false, false, false), None);
        assert_eq!(shortcut("z", true, false, true), None);
        assert_eq!(shortcut("x", true, false, false), None);
    }
}

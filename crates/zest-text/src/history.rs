use crate::Selection;
use std::collections::VecDeque;

/// Bounds the combined undo/redo history. Payload bytes count inserted and
/// removed UTF-8, excluding allocation overhead. Zero in either field disables it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryLimits {
    pub edits: usize,
    pub bytes: usize,
}
impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            edits: 128,
            bytes: 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryStats {
    pub undo_edits: usize,
    pub redo_edits: usize,
    pub payload_bytes: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Edit {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
    pub before: Selection,
    pub after: Selection,
}
impl Edit {
    fn bytes(&self) -> usize {
        self.removed.len().saturating_add(self.inserted.len())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct History {
    pub undo: VecDeque<Edit>,
    pub redo: Vec<Edit>,
    limits: HistoryLimits,
    bytes: usize,
}
impl History {
    pub fn new(limits: HistoryLimits) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limits,
            bytes: 0,
        }
    }
    pub fn stats(&self) -> HistoryStats {
        HistoryStats {
            undo_edits: self.undo.len(),
            redo_edits: self.redo.len(),
            payload_bytes: self.bytes,
        }
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.bytes = 0;
    }
    pub fn record(&mut self, edit: Edit) {
        for redo in self.redo.drain(..) {
            self.bytes -= redo.bytes();
        }
        let cost = edit.bytes();
        if self.limits.edits == 0 || cost > self.limits.bytes || self.limits.bytes == 0 {
            // An unrecordable edit breaks the chain: older deltas no longer
            // describe the current document and must never be replayed on it.
            self.clear();
            return;
        }
        while self.undo.len() >= self.limits.edits || cost > self.limits.bytes - self.bytes {
            self.bytes -= self
                .undo
                .pop_front()
                .expect("history exceeds budget")
                .bytes();
        }
        self.bytes += cost;
        self.undo.push_back(edit);
    }
}

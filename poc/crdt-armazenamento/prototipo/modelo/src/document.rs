//! The document as the rest of the program sees it: a store plus the undo
//! layer (ADR-0002: undo is the author's own list of operations; ADR-0021,
//! owner's condition 2).
//!
//! Before each operation, its inverse is computed from the current state.
//! Undo applies an inverse only if nobody changed that value since; when
//! someone did, that part is skipped and reported, never overwritten
//! silently (Loro's own undo overwrote another copy's edit in phase 0.6).

use std::collections::VecDeque;

use crate::op::{Edge, NodeId, Op, Value};
use crate::store::{Anchor, Applied, OpError, Store};

/// Undo steps kept per author.
pub const MAX_UNDO_STEPS: usize = 10_000;

#[derive(Clone, Debug)]
enum Inverse {
    Param {
        node: NodeId,
        key: String,
        wrote: Option<Value>,
        old: Option<Value>,
    },
    Created {
        node: NodeId,
    },
    Removed {
        node: NodeId,
    },
    Restored {
        node: NodeId,
    },
    Moved {
        node: NodeId,
        wrote: Option<NodeId>,
        old_parent: Option<NodeId>,
        old_index: usize,
    },
    Connected {
        id: String,
    },
    Disconnected {
        id: String,
        edge: Edge,
    },
    TextInserted {
        node: NodeId,
        start: Anchor,
        text: String,
    },
    TextDeleted {
        node: NodeId,
        before: Option<Anchor>,
        text: String,
    },
}

/// Result of one undo step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UndoOutcome {
    pub undone: bool,
    /// Parts of the step that were not undone, and why. Shown as a warning.
    pub skipped: Vec<String>,
}

pub struct Document<S: Store> {
    pub store: S,
    steps: VecDeque<Vec<Inverse>>,
    pending: Vec<Inverse>,
}

impl<S: Store> Document<S> {
    pub fn new(store: S) -> Self {
        Document {
            store,
            steps: VecDeque::new(),
            pending: Vec::new(),
        }
    }

    /// Applies one operation inside the current transaction.
    pub fn apply(&mut self, op: Op) -> Result<Applied, OpError> {
        let inverse = self.inverse_of(&op)?;
        let applied = self.store.apply(&op)?;
        match (inverse, &op) {
            (Some(inv), _) => self.pending.push(inv),
            (None, Op::CreateNode { .. }) => {
                let node = applied.created.clone().expect("create returns the id");
                self.pending.push(Inverse::Created { node });
            }
            (None, Op::Connect { .. }) => {
                let id = applied.edge.clone().expect("connect returns the id");
                self.pending.push(Inverse::Connected { id });
            }
            (None, Op::InsertText { node, pos, text }) if !text.is_empty() => {
                if let Some(start) = self.store.text_anchor(node, *pos) {
                    self.pending.push(Inverse::TextInserted {
                        node: node.clone(),
                        start,
                        text: text.clone(),
                    });
                }
            }
            _ => {}
        }
        Ok(applied)
    }

    /// Ends the transaction: everything since the last commit is one undo
    /// step (a script of 500 operations undoes with one Ctrl+Z).
    pub fn commit(&mut self) {
        self.store.commit();
        if !self.pending.is_empty() {
            self.steps.push_back(std::mem::take(&mut self.pending));
            if self.steps.len() > MAX_UNDO_STEPS {
                self.steps.pop_front();
            }
        }
    }

    fn inverse_of(&self, op: &Op) -> Result<Option<Inverse>, OpError> {
        let s = &self.store;
        Ok(match op {
            Op::SetParam { node, key, value } => Some(Inverse::Param {
                node: node.clone(),
                key: key.clone(),
                wrote: value.clone(),
                old: s.param(node, key),
            }),
            Op::RemoveNode { node } => Some(Inverse::Removed { node: node.clone() }),
            Op::RestoreNode { node } => Some(Inverse::Restored { node: node.clone() }),
            Op::MoveNode { node, parent, .. } => {
                let old_parent = s
                    .parent(node)
                    .ok_or_else(|| OpError::NotFound(node.0.clone()))?;
                let old_index = s
                    .children(old_parent.as_ref())
                    .iter()
                    .position(|c| c == node)
                    .unwrap_or(0);
                Some(Inverse::Moved {
                    node: node.clone(),
                    wrote: parent.clone(),
                    old_parent,
                    old_index,
                })
            }
            Op::Disconnect { id } => {
                let edge = s
                    .edges()
                    .remove(id)
                    .ok_or_else(|| OpError::NotFound(id.clone()))?;
                Some(Inverse::Disconnected {
                    id: id.clone(),
                    edge,
                })
            }
            Op::DeleteText { node, pos, len } => {
                let text = s
                    .text(node)
                    .ok_or_else(|| OpError::NotFound(node.0.clone()))?;
                let removed: String = text.chars().skip(*pos).take(*len).collect();
                let before = pos.checked_sub(1).and_then(|p| s.text_anchor(node, p));
                Some(Inverse::TextDeleted {
                    node: node.clone(),
                    before,
                    text: removed,
                })
            }
            Op::CreateNode { .. } | Op::Connect { .. } | Op::InsertText { .. } => None,
        })
    }

    /// Undoes this author's last step, never someone else's edit.
    pub fn undo(&mut self) -> UndoOutcome {
        self.commit();
        let Some(step) = self.steps.pop_back() else {
            return UndoOutcome::default();
        };
        let mut out = UndoOutcome::default();
        for inv in step.into_iter().rev() {
            match self.undo_one(inv) {
                Ok(None) => out.undone = true,
                Ok(Some(why)) => out.skipped.push(why),
                Err(e) => out.skipped.push(format!("{e:?}")),
            }
        }
        // The undo is a normal change in the history; it is not itself put
        // on the undo stack.
        self.store.commit();
        self.pending.clear();
        out
    }

    fn undo_one(&mut self, inv: Inverse) -> Result<Option<String>, OpError> {
        let s = &mut self.store;
        let skip = |why: String| Ok(Some(why));
        match inv {
            Inverse::Param {
                node,
                key,
                wrote,
                old,
            } => {
                if s.param(&node, &key) != wrote {
                    return skip(format!(
                        "{node}.{key}: changed by another copy after this edit"
                    ));
                }
                s.apply(&Op::SetParam {
                    node,
                    key,
                    value: old,
                })?;
            }
            Inverse::Created { node } => {
                if !s.is_removed(&node) {
                    s.apply(&Op::RemoveNode { node })?;
                }
            }
            Inverse::Removed { node } => {
                if s.is_removed(&node) {
                    s.apply(&Op::RestoreNode { node })?;
                }
            }
            Inverse::Restored { node } => {
                if !s.is_removed(&node) {
                    s.apply(&Op::RemoveNode { node })?;
                }
            }
            Inverse::Moved {
                node,
                wrote,
                old_parent,
                old_index,
            } => {
                if s.stored_parent(&node) != Some(wrote) {
                    return skip(format!(
                        "{node}: moved again by another copy after this edit"
                    ));
                }
                if let Some(p) = &old_parent
                    && (s.parent(p).is_none() || is_ancestor(s, &node, p))
                {
                    return skip(format!(
                        "{node}: its old place no longer exists or would form a cycle"
                    ));
                }
                s.apply(&Op::MoveNode {
                    node,
                    parent: old_parent,
                    index: old_index,
                })?;
            }
            Inverse::Connected { id } => {
                if s.edges().contains_key(&id) {
                    s.apply(&Op::Disconnect { id })?;
                }
            }
            Inverse::Disconnected { id, edge } => {
                if s.edges().contains_key(&id) {
                    return skip(format!("connection {id}: already back"));
                }
                s.apply(&Op::Connect { id: Some(id), edge })?;
            }
            Inverse::TextInserted { node, start, text } => {
                let Some(pos) = s.anchor_pos(&node, &start) else {
                    return skip(format!("{node}: the text is gone"));
                };
                let len = text.chars().count();
                let now: String = s
                    .text(&node)
                    .unwrap_or_default()
                    .chars()
                    .skip(pos)
                    .take(len)
                    .collect();
                if now != text {
                    return skip(format!("{node}: another copy wrote inside this text"));
                }
                s.apply(&Op::DeleteText { node, pos, len })?;
            }
            Inverse::TextDeleted { node, before, text } => {
                let pos = match before {
                    Some(a) => match s.anchor_pos(&node, &a) {
                        Some(p) => p + 1,
                        None => return skip(format!("{node}: the text is gone")),
                    },
                    None => 0,
                };
                s.apply(&Op::InsertText { node, pos, text })?;
            }
        }
        Ok(None)
    }
}

/// Whether `ancestor` is `node` or above it.
pub fn is_ancestor<S: Store>(s: &S, ancestor: &NodeId, node: &NodeId) -> bool {
    let mut cur = Some(node.clone());
    let mut steps = 0;
    while let Some(n) = cur {
        if &n == ancestor {
            return true;
        }
        steps += 1;
        if steps > 1_000_000 {
            return false;
        }
        cur = s.stored_parent(&n).flatten();
    }
    false
}

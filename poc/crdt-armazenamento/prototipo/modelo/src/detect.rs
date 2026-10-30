//! Conflict detection after a merge (ADR-0019, decision 3; ADR-0021).
//!
//! The store reads back, from its own history, what each side did since the
//! other last saw it (`Touch`). These functions turn the two lists into the
//! merge report. They hold no library knowledge; the store decides which
//! operations count as concurrent (causality) and in which order they ran.

use std::collections::{BTreeMap, BTreeSet};

use crate::op::{NodeId, Value};
use crate::store::{ConcurrentWrite, LostMove, Store};

/// Key under which remove/restore races are reported.
pub const REMOVED_KEY: &str = "@removed";

#[derive(Clone, Debug, PartialEq)]
pub enum Touch {
    Create {
        node: NodeId,
    },
    Param {
        node: NodeId,
        key: String,
        value: Option<Value>,
    },
    Text {
        node: NodeId,
    },
    /// `parent == None` is the root. `pos` is the sibling position key.
    Move {
        node: NodeId,
        parent: Option<NodeId>,
        pos: String,
    },
    Remove {
        node: NodeId,
    },
    Restore {
        node: NodeId,
    },
}

impl Touch {
    pub fn node(&self) -> &NodeId {
        match self {
            Touch::Create { node }
            | Touch::Param { node, .. }
            | Touch::Text { node }
            | Touch::Move { node, .. }
            | Touch::Remove { node }
            | Touch::Restore { node } => node,
        }
    }
}

/// Last write per parameter (and per removal flag), in history order.
fn last_writes(touches: &[Touch]) -> BTreeMap<(NodeId, String), Option<Value>> {
    let mut out = BTreeMap::new();
    for t in touches {
        let (node, key, value) = match t {
            Touch::Param { node, key, value } => (node, key.as_str(), value.clone()),
            Touch::Remove { node } => (node, REMOVED_KEY, Some(Value::Str("removed".into()))),
            Touch::Restore { node } => (node, REMOVED_KEY, Some(Value::Str("restored".into()))),
            _ => continue,
        };
        out.insert((node.clone(), key.to_owned()), value);
    }
    out
}

/// The same parameter, or the removal flag, written by both sides.
pub fn concurrent_writes<S: Store>(
    s: &S,
    local: &[Touch],
    remote: &[Touch],
) -> Vec<ConcurrentWrite> {
    let (l, rm) = (last_writes(local), last_writes(remote));
    let mut out = Vec::new();
    for (k, lv) in &l {
        let Some(rv) = rm.get(k) else { continue };
        if lv == rv {
            continue; // both wrote the same value: nothing was lost
        }
        let kept = if k.1 == REMOVED_KEY {
            Some(Value::Str(
                if s.is_removed(&k.0) {
                    "removed"
                } else {
                    "restored"
                }
                .into(),
            ))
        } else {
            s.param(&k.0, &k.1)
        };
        let lost = [lv, rv]
            .into_iter()
            .filter(|v| **v != kept)
            .filter_map(|v| v.clone())
            .collect();
        out.push(ConcurrentWrite {
            node: k.0.clone(),
            key: k.1.clone(),
            kept,
            lost,
        });
    }
    out
}

fn last_moves(touches: &[Touch]) -> BTreeMap<NodeId, (Option<NodeId>, String)> {
    let mut out = BTreeMap::new();
    for t in touches {
        if let Touch::Move { node, parent, pos } = t {
            out.insert(node.clone(), (parent.clone(), pos.clone()));
        }
    }
    out
}

/// Moves whose wish is not what the merged document shows. `pos_of`
/// returns the node's current sibling position key.
pub fn lost_moves<S: Store>(
    s: &S,
    local: &[Touch],
    remote: &[Touch],
    pos_of: impl Fn(&NodeId) -> Option<String>,
) -> Vec<LostMove> {
    let (l, rm) = (last_moves(local), last_moves(remote));
    let mut out = Vec::new();
    for (side, other) in [(&l, &rm), (&rm, &l)] {
        for (node, (wanted, pos)) in side {
            let now = s.stored_parent(node);
            let honoured = now.as_ref() == Some(wanted)
                // A reorder by one side only may get a new key when the
                // library rebalances; only a concurrent wish on the same
                // node competes for the exact position.
                && (!other.contains_key(node) || pos_of(node).as_ref() == Some(pos));
            if !honoured {
                out.push(LostMove {
                    node: node.clone(),
                    wanted_parent: wanted.clone(),
                    final_parent: now.flatten(),
                });
            }
        }
    }
    out
}

/// Nodes edited on one side that this merge hid (a removal, or a move
/// into a removed node, that the editing copy had not seen).
///
/// `visible_before` holds every node a side touched that was visible on
/// that side's own copy just before the merge. Asking "was it visible where
/// it was edited, and is it hidden now?" needs no guess about who caused
/// it. An earlier rule that walked up to the removed node and asked who
/// removed it missed cases where a merge re-enabled a move that had been
/// cancelled to avoid a cycle (phase 0.6, part 3).
pub fn newly_hidden<S: Store>(s: &S, visible_before: &BTreeSet<NodeId>) -> Vec<NodeId> {
    visible_before
        .iter()
        .filter(|n| s.parent(n).is_none())
        .cloned()
        .collect()
}

/// The nodes in `touches` that `s` shows as visible.
pub fn visible_touched<S: Store>(s: &S, touches: &[Touch]) -> BTreeSet<NodeId> {
    touches
        .iter()
        .map(Touch::node)
        .filter(|n| s.parent(n).is_some())
        .cloned()
        .collect()
}

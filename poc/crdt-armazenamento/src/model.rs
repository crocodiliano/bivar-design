//! The library-independent "socket" both proofs plug into.
//!
//! Nothing here uses a Loro or Automerge type: the operations follow ADR-0002
//! (create/remove node, connect/disconnect, set parameter, reorder, move in
//! the hierarchy) plus text editing, and the merge report carries what
//! ADR-0019 decision 3 needs to warn and record.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A node identity that is unique across machines.
pub type NodeId = String;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Str(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub from: NodeId,
    pub from_port: String,
    pub to: NodeId,
    pub to_port: String,
}

impl Edge {
    /// Single-string form stored in both libraries (one register per edge).
    pub fn encode(&self) -> String {
        [&self.from, &self.from_port, &self.to, &self.to_port]
            .map(String::as_str)
            .join("\u{1f}")
    }

    pub fn decode(s: &str) -> Option<Edge> {
        let mut it = s.split('\u{1f}');
        let edge = Edge {
            from: it.next()?.to_owned(),
            from_port: it.next()?.to_owned(),
            to: it.next()?.to_owned(),
            to_port: it.next()?.to_owned(),
        };
        it.next().is_none().then_some(edge)
    }
}

/// Canonical view of the visible document, used to compare replicas.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct State {
    pub doc_id: String,
    pub nodes: BTreeMap<NodeId, NodeState>,
    pub roots: Vec<NodeId>,
    pub edges: BTreeMap<String, Edge>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct NodeState {
    pub kind: String,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub params: BTreeMap<String, Value>,
    pub text: Option<String>,
}

/// The same parameter written by both copies without seeing each other.
#[derive(Clone, Debug, PartialEq)]
pub struct ConcurrentWrite {
    pub node: NodeId,
    pub key: String,
    pub kept: Option<Value>,
    pub lost: Vec<Value>,
}

/// A move (or reorder, or removal) that lost to a concurrent one.
#[derive(Clone, Debug, PartialEq)]
pub struct LostMove {
    pub node: NodeId,
    /// Where the losing move wanted the node. `None` means the root;
    /// `Some("<removed>")` means the losing operation was a removal.
    pub wanted_parent: Option<NodeId>,
    pub final_parent: Option<NodeId>,
}

/// What the program needs after a merge to warn and record (ADR-0019, 3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    pub concurrent_writes: Vec<ConcurrentWrite>,
    pub lost_moves: Vec<LostMove>,
    /// Nodes one copy edited while the other removed them (or an ancestor).
    pub hidden_by_removal: Vec<NodeId>,
}

impl MergeReport {
    pub fn is_empty(&self) -> bool {
        self.concurrent_writes.is_empty()
            && self.lost_moves.is_empty()
            && self.hidden_by_removal.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpError {
    NotFound(String),
    WouldCycle,
    DifferentDocument,
    Library(String),
}

/// Result of one undo step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UndoOutcome {
    pub undone: bool,
    /// Parts of the step that were not undone, and why.
    pub skipped: Vec<String>,
}

pub const REMOVED_MARK: &str = "<removed>";

/// One document copy. Both libraries implement this the same way.
pub trait Replica: Sized {
    const NAME: &'static str;

    /// A brand-new document with a new identity.
    fn new_document(peer: u64) -> Self;
    /// Another copy of the same document, as if opened on another computer.
    fn fork_as(&mut self, peer: u64) -> Self;
    fn doc_id(&self) -> String;

    fn create_node(&mut self, parent: Option<&str>, kind: &str) -> Result<NodeId, OpError>;
    fn remove_node(&mut self, id: &str) -> Result<(), OpError>;
    /// Moves under `parent` at `index` among its other children (the moved
    /// node not counted), clamped to the end. Reordering is a move to the
    /// same parent.
    fn move_node(&mut self, id: &str, parent: Option<&str>, index: usize) -> Result<(), OpError>;
    fn set_param(&mut self, id: &str, key: &str, value: Value) -> Result<(), OpError>;
    fn connect(&mut self, edge: &Edge) -> Result<String, OpError>;
    fn disconnect(&mut self, edge_id: &str) -> Result<(), OpError>;
    fn insert_text(&mut self, id: &str, pos: usize, text: &str) -> Result<(), OpError>;
    fn delete_text(&mut self, id: &str, pos: usize, len: usize) -> Result<(), OpError>;
    /// Ends one user action: one undo step and one history entry.
    fn commit(&mut self);

    fn children(&self, parent: Option<&str>) -> Vec<NodeId>;
    /// `None` if the node is not visible; `Some(None)` for a root node.
    fn parent(&self, id: &str) -> Option<Option<NodeId>>;
    /// The stored parent, even when the node or an ancestor was removed.
    /// `None` if the node never existed; `Some(None)` for a root node.
    fn stored_parent(&self, id: &str) -> Option<Option<NodeId>>;
    /// Whether the node itself is removed (not just under a removed node).
    fn is_removed(&self, id: &str) -> bool;
    fn param(&self, id: &str, key: &str) -> Option<Value>;
    fn text(&self, id: &str) -> Option<String>;
    fn edges(&self) -> BTreeMap<String, Edge>;
    fn state(&self) -> State;

    /// Receives everything `other` has that `self` lacks.
    fn merge_from(&mut self, other: &mut Self) -> Result<MergeReport, OpError>;
    fn undo(&mut self) -> UndoOutcome;

    fn save(&mut self) -> Vec<u8>;
    fn load(bytes: &[u8], peer: u64) -> Result<Self, OpError>;
    /// The whole history decoded as text, the way the program reads it.
    fn history_dump(&mut self) -> String;
    fn history_ops(&mut self) -> usize;
    /// Every peer (computer/session) that appears in the history.
    fn history_peers(&mut self) -> BTreeSet<String>;
}

/// "Salvar cópia para enviar" (ADR-0019, decision 5): rebuilds the visible
/// state in a brand-new document, so no history and no deleted content can
/// come along. Returns the copy and the map old id -> new id.
pub fn clean_copy<R: Replica>(src: &R, peer: u64) -> (R, HashMap<NodeId, NodeId>) {
    let state = src.state();
    let mut dst = R::new_document(peer);
    let mut ids = HashMap::new();
    let mut stack: Vec<(Option<NodeId>, NodeId)> = state
        .roots
        .iter()
        .rev()
        .map(|r| (None, r.clone()))
        .collect();
    while let Some((new_parent, old)) = stack.pop() {
        let node = &state.nodes[&old];
        let new = dst
            .create_node(new_parent.as_deref(), &node.kind)
            .expect("parent was just created");
        for (k, v) in &node.params {
            dst.set_param(&new, k, v.clone()).expect("node exists");
        }
        if let Some(t) = &node.text {
            dst.insert_text(&new, 0, t).expect("text node");
        }
        for c in node.children.iter().rev() {
            stack.push((Some(new.clone()), c.clone()));
        }
        ids.insert(old, new);
    }
    for edge in state.edges.values() {
        let (Some(from), Some(to)) = (ids.get(&edge.from), ids.get(&edge.to)) else {
            continue;
        };
        let e = Edge {
            from: from.clone(),
            from_port: edge.from_port.clone(),
            to: to.clone(),
            to_port: edge.to_port.clone(),
        };
        dst.connect(&e).expect("both ends exist");
    }
    dst.commit();
    (dst, ids)
}

/// What one copy did since the other last saw it, read back from the
/// library history. Used to build the merge report.
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
}

impl Touch {
    pub fn node(&self) -> &str {
        match self {
            Touch::Create { node }
            | Touch::Param { node, .. }
            | Touch::Text { node }
            | Touch::Move { node, .. }
            | Touch::Remove { node } => node,
        }
    }
}

/// Last write per parameter, in history order.
fn last_params(touches: &[Touch]) -> BTreeMap<(NodeId, String), Option<Value>> {
    let mut out = BTreeMap::new();
    for t in touches {
        if let Touch::Param { node, key, value } = t {
            out.insert((node.clone(), key.clone()), value.clone());
        }
    }
    out
}

/// Last placement wish per node: `Some((parent, pos))` or `None` = removal.
type Placement = Option<(Option<NodeId>, String)>;

fn last_placements(touches: &[Touch]) -> BTreeMap<NodeId, Placement> {
    let mut out = BTreeMap::new();
    for t in touches {
        match t {
            Touch::Move { node, parent, pos } => {
                out.insert(node.clone(), Some((parent.clone(), pos.clone())));
            }
            Touch::Remove { node } => {
                out.insert(node.clone(), None);
            }
            _ => {}
        }
    }
    out
}

/// Concurrent parameter writes, found by comparing both histories.
/// This is the "built outside the library" detection used for Loro.
pub fn concurrent_writes_from_touches<R: Replica>(
    r: &R,
    local: &[Touch],
    remote: &[Touch],
) -> Vec<ConcurrentWrite> {
    let (l, rm) = (last_params(local), last_params(remote));
    let mut out = Vec::new();
    for (k, lv) in &l {
        let Some(rv) = rm.get(k) else { continue };
        if lv == rv {
            continue; // both wrote the same value: nothing was lost
        }
        let kept = r.param(&k.0, &k.1);
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

/// Moves whose wish is not what the merged document shows. `pos_of`
/// returns the node's current sibling position key.
pub fn lost_moves_from_touches<R: Replica>(
    r: &R,
    local: &[Touch],
    remote: &[Touch],
    pos_of: impl Fn(&str) -> Option<String>,
) -> Vec<LostMove> {
    let (l, rm) = (last_placements(local), last_placements(remote));
    let mut out = Vec::new();
    for (side, other) in [(&l, &rm), (&rm, &l)] {
        for (node, wish) in side {
            let now = r.parent(node);
            let honoured = match (wish, &now) {
                (None, None) => true,
                (Some((p, pos)), Some(cur)) => {
                    // A reorder by only one side may get a new key when the
                    // library rebalances; only a concurrent wish on the same
                    // node competes for the exact position.
                    p == cur && (!other.contains_key(node) || pos_of(node).as_ref() == Some(pos))
                }
                _ => false,
            };
            if honoured {
                continue;
            }
            // A node hidden because another copy removed an ancestor is a
            // structural conflict, reported in hidden_by_removal instead.
            if now.is_none() && wish.is_some() && !other.contains_key(node) {
                continue;
            }
            out.push(LostMove {
                node: node.clone(),
                wanted_parent: match wish {
                    Some((p, _)) => p.clone(),
                    None => Some(REMOVED_MARK.to_owned()),
                },
                final_parent: now.flatten(),
            });
        }
    }
    out
}

/// Nodes one side touched that are now hidden by a removal it did not make.
pub fn hidden_by_removal<R: Replica>(r: &R, local: &[Touch], remote: &[Touch]) -> Vec<NodeId> {
    let mut out = BTreeSet::new();
    for (side, other) in [(local, remote), (remote, local)] {
        let removed_here: BTreeSet<&str> = side
            .iter()
            .filter_map(|t| match t {
                Touch::Remove { node } => Some(node.as_str()),
                _ => None,
            })
            .collect();
        let removed_there: BTreeSet<&str> = other
            .iter()
            .filter_map(|t| match t {
                Touch::Remove { node } => Some(node.as_str()),
                _ => None,
            })
            .collect();
        // Every (node, parent) link a move on the other side asked for. Not
        // just the last one: with three copies, "the other side" mixes the
        // histories of two copies and "last" means nothing.
        let moved_there: BTreeSet<(&str, Option<&str>)> = other
            .iter()
            .filter_map(|t| match t {
                Touch::Move { node, parent, .. } => Some((node.as_str(), parent.as_deref())),
                _ => None,
            })
            .collect();
        // Where this side asked each node to go. A link this side never asked
        // for, on a node it did move, exists because its move lost.
        let mut moved_here: BTreeMap<&str, BTreeSet<Option<&str>>> = BTreeMap::new();
        for t in side {
            if let Touch::Move { node, parent, .. } = t {
                moved_here
                    .entry(node)
                    .or_default()
                    .insert(parent.as_deref());
            }
        }
        if removed_there.is_empty() && removed_here.is_empty() {
            continue;
        }
        for t in side {
            let node = t.node();
            if r.parent(node).is_some() || out.contains(node) {
                continue;
            }
            // Walk up the stored parents to the first removed node. The
            // node was hidden behind this copy's back if the other copy
            // removed it, or if a move by the other copy put it under a node
            // this copy removed (this copy could not have known).
            let mut cur = node.to_owned();
            let mut linked_by_other = false;
            let mut steps = 0;
            let hidden_by_other = loop {
                // Only a removal that is in effect counts: a removal can
                // itself lose to a concurrent move.
                if r.is_removed(&cur) {
                    let here = removed_here.contains(cur.as_str());
                    let there = removed_there.contains(cur.as_str());
                    break there || !here || linked_by_other;
                }
                let Some(Some(p)) = r.stored_parent(&cur) else {
                    break false;
                };
                linked_by_other |= moved_there.contains(&(cur.as_str(), Some(p.as_str())))
                    || moved_here
                        .get(cur.as_str())
                        .is_some_and(|wished| !wished.contains(&Some(p.as_str())));
                cur = p;
                steps += 1;
                if steps > 100_000 {
                    break false; // defensive: a corrupt parent chain
                }
            };
            if hidden_by_other {
                out.insert(node.to_owned());
            }
        }
    }
    out.into_iter().collect()
}

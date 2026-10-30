//! What a document store must provide (ADR-0021). The only implementation
//! lives in `loja-loro`; nothing else names the CRDT library.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::op::{Edge, NodeId, Op, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum OpError {
    NotFound(String),
    WouldCycle,
    DifferentDocument,
    /// Refused by validation before reaching the store.
    Invalid(String),
    Library(String),
    /// The other copy was offline for longer than this document keeps
    /// history: its changes cannot be merged automatically. Nothing is lost;
    /// that copy can be saved as a separate document.
    HistoryTrimmed,
}

/// What applying an operation produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    pub created: Option<NodeId>,
    pub edge: Option<String>,
}

/// Canonical view of the visible document, used to compare copies.
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

/// The same parameter written by two copies without seeing each other.
#[derive(Clone, Debug, PartialEq)]
pub struct ConcurrentWrite {
    pub node: NodeId,
    pub key: String,
    pub kept: Option<Value>,
    pub lost: Vec<Value>,
}

/// A move or reorder that lost to a concurrent one.
#[derive(Clone, Debug, PartialEq)]
pub struct LostMove {
    pub node: NodeId,
    pub wanted_parent: Option<NodeId>,
    pub final_parent: Option<NodeId>,
}

/// What the program needs after a merge to warn and record (ADR-0019, 3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    pub concurrent_writes: Vec<ConcurrentWrite>,
    pub lost_moves: Vec<LostMove>,
    /// Nodes one copy edited while another removed them (or an ancestor).
    pub hidden_by_removal: Vec<NodeId>,
}

impl MergeReport {
    pub fn is_empty(&self) -> bool {
        self.concurrent_writes.is_empty()
            && self.lost_moves.is_empty()
            && self.hidden_by_removal.is_empty()
    }
}

/// A position inside a text that follows the text when others edit it.
/// Opaque bytes issued by the store.
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor(pub Vec<u8>);

pub trait Store: Sized {
    const NAME: &'static str;

    /// A brand-new document with a new identity.
    fn new_document(peer: u64) -> Self;
    /// Another copy of the same document, as if opened on another computer.
    fn fork_as(&mut self, peer: u64) -> Self;
    fn doc_id(&self) -> String;

    /// The only way to change the document (ADR-0002).
    fn apply(&mut self, op: &Op) -> Result<Applied, OpError>;
    /// Ends one user action: one history entry.
    fn commit(&mut self);

    fn children(&self, parent: Option<&NodeId>) -> Vec<NodeId>;
    /// `None` if the node is not visible; `Some(None)` for a root node.
    fn parent(&self, id: &NodeId) -> Option<Option<NodeId>>;
    /// The stored parent, even when the node or an ancestor is removed.
    fn stored_parent(&self, id: &NodeId) -> Option<Option<NodeId>>;
    /// Whether the node itself is marked removed.
    fn is_removed(&self, id: &NodeId) -> bool;
    fn kind(&self, id: &NodeId) -> Option<String>;
    fn param(&self, id: &NodeId, key: &str) -> Option<Value>;
    fn text(&self, id: &NodeId) -> Option<String>;
    fn edges(&self) -> BTreeMap<String, Edge>;
    fn state(&self) -> State;

    fn text_anchor(&self, id: &NodeId, pos: usize) -> Option<Anchor>;
    fn anchor_pos(&self, id: &NodeId, anchor: &Anchor) -> Option<usize>;

    /// Receives everything `other` has that `self` lacks.
    fn merge_from(&mut self, other: &mut Self) -> Result<MergeReport, OpError>;

    fn save(&mut self) -> Vec<u8>;
    fn load(bytes: &[u8], peer: u64) -> Result<Self, OpError>;
    /// The whole history decoded as text, the way the program reads it.
    fn history_dump(&mut self) -> String;
    fn history_peers(&mut self) -> BTreeSet<String>;
    /// The time each change of the history records (0 = none recorded).
    fn history_times(&mut self) -> BTreeSet<i64>;
    /// Every change, in the library's uncompressed exchange form. A
    /// snapshot compresses repeated text, so a byte search of the file alone
    /// can miss it; tests search this form too.
    fn export_all(&mut self) -> Vec<u8>;

    // Storage (phase 0.6, part 4): the file keeps the document as a first
    // snapshot followed by incremental updates.

    /// An opaque version: what this copy has seen.
    fn version(&self) -> Vec<u8>;
    /// Everything this copy has that `version` lacks.
    fn updates_since(&mut self, version: &[u8]) -> Result<Vec<u8>, OpError>;
    /// Adds updates read from the file (or received).
    fn import_updates(&mut self, bytes: &[u8]) -> Result<(), OpError>;
    /// Many updates at once (opening a file): much faster than one by one.
    fn import_updates_batch(&mut self, batch: &[Vec<u8>]) -> Result<(), OpError> {
        for b in batch {
            self.import_updates(b)?;
        }
        Ok(())
    }

    // Cleanup (owner's rules, 2026-10-29): compaction is only suggested,
    // never automatic; named milestones and unresolved conflicts are never
    // cleaned; copies that compaction would cut off are listed first.

    /// Records, inside the document, that copy `copy` has seen everything
    /// here, at time `now` (seconds). Also adds a time checkpoint.
    fn record_seen(&mut self, copy: &str, now: u64);
    /// Names the current version; history needed to reach it is kept.
    fn add_milestone(&mut self, name: &str);
    /// What a compaction at time `now` would clean. With `forget_stale`,
    /// copies not seen for `retention` seconds no longer hold history back,
    /// and are listed in `cut_off`.
    fn plan_compaction(&self, now: u64, retention: u64, forget_stale: bool) -> CompactionPlan;
    /// Applies a confirmed plan and returns the compacted document.
    fn compact(&mut self, plan: &CompactionPlan) -> Result<Vec<u8>, OpError>;
    /// Hashes of large files still referenced (parameter `blob`), by any
    /// node not purged.
    fn blob_hashes(&self) -> BTreeSet<String>;
    /// Conflicts still open in the document (two values left by concurrent
    /// writes), as `node.key`. "Salvar cópia para enviar" warns about them:
    /// the copy keeps only the winning value, without history.
    fn open_conflicts(&self) -> Vec<String>;
}

/// What a compaction would do, shown to the owner before confirming.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompactionPlan {
    /// Removed nodes that can go: removed long enough, seen by every known
    /// copy, no open conflict, no live connection.
    pub purge: Vec<NodeId>,
    /// Removed nodes kept because of an unresolved conflict.
    pub kept_for_conflicts: Vec<NodeId>,
    /// History before this version (opaque) can go; `None` keeps it all.
    pub trim_before: Option<Vec<u8>>,
    /// Known copies that could no longer merge after this compaction.
    pub cut_off: Vec<String>,
}

/// A random number from the operating system's seed for hash tables (no
/// new dependency). Good for identifiers, not for cryptography.
pub fn random_u64() -> u64 {
    use std::hash::BuildHasher;
    std::collections::hash_map::RandomState::new().hash_one(0u8)
}

/// The layer parameter that hides a layer without removing it. The
/// prototype has no yes/no value: `Num(0.0)` means hidden, anything else
/// (or no value) means shown.
pub const VISIBLE_KEY: &str = "visible";

/// A layer switched off with `VISIBLE_KEY`: still in the document, not
/// drawn. Everything inside it is hidden with it.
#[derive(Clone, Debug, PartialEq)]
pub struct HiddenLayer {
    pub node: NodeId,
    pub kind: String,
    /// The layer's `name` parameter, if it has one.
    pub name: Option<String>,
    /// How many layers are inside it (they go or stay with it).
    pub inside: usize,
}

fn is_hidden(node: &NodeState) -> bool {
    node.params.get(VISIBLE_KEY) == Some(&Value::Num(0.0))
}

/// Every hidden layer of the document, in tree order. A hidden layer
/// inside another hidden layer is listed too.
pub fn hidden_layers(state: &State) -> Vec<HiddenLayer> {
    let mut out = Vec::new();
    let mut stack: Vec<&NodeId> = state.roots.iter().rev().collect();
    while let Some(id) = stack.pop() {
        let node = &state.nodes[id];
        if is_hidden(node) {
            out.push(HiddenLayer {
                node: id.clone(),
                kind: node.kind.clone(),
                name: match node.params.get("name") {
                    Some(Value::Str(s)) => Some(s.clone()),
                    _ => None,
                },
                inside: count_inside(state, node),
            });
        }
        stack.extend(node.children.iter().rev());
    }
    out
}

fn count_inside(state: &State, node: &NodeState) -> usize {
    node.children
        .iter()
        .map(|c| 1 + count_inside(state, &state.nodes[c]))
        .sum()
}

/// "Salvar cópia para enviar" (ADR-0019, decision 5): rebuilds the visible
/// state in a brand-new document, so no history and nothing removed comes
/// along. With `drop_hidden`, hidden layers (and what is inside them) stay
/// out too. Returns the copy and the map old id -> new id.
pub fn clean_copy<S: Store>(src: &S, peer: u64, drop_hidden: bool) -> (S, HashMap<NodeId, NodeId>) {
    let state = src.state();
    let mut dst = S::new_document(peer);
    let mut ids = HashMap::new();
    let mut stack: Vec<(Option<NodeId>, NodeId)> = state
        .roots
        .iter()
        .rev()
        .map(|r| (None, r.clone()))
        .collect();
    while let Some((new_parent, old)) = stack.pop() {
        let node = &state.nodes[&old];
        if drop_hidden && is_hidden(node) {
            continue;
        }
        let new = dst
            .apply(&Op::CreateNode {
                parent: new_parent,
                index: None,
                kind: node.kind.clone(),
            })
            .expect("parent was just created")
            .created
            .expect("create returns the id");
        for (k, v) in &node.params {
            let op = Op::SetParam {
                node: new.clone(),
                key: k.clone(),
                value: Some(v.clone()),
            };
            dst.apply(&op).expect("node exists");
        }
        if let Some(t) = &node.text {
            let op = Op::InsertText {
                node: new.clone(),
                pos: 0,
                text: t.clone(),
            };
            dst.apply(&op).expect("text node");
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
        let edge = Edge {
            from: from.clone(),
            from_port: edge.from_port.clone(),
            to: to.clone(),
            to_port: edge.to_port.clone(),
        };
        dst.apply(&Op::Connect { id: None, edge })
            .expect("both ends exist");
    }
    dst.commit();
    (dst, ids)
}

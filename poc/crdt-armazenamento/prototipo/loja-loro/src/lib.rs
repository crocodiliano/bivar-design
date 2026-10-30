//! The document store on Loro (ADR-0021). The only crate that names Loro.
//!
//! Layout inside the Loro document:
//! - root map `meta`: `doc_id` and `schema` (the layout version below);
//! - tree `tree`: the node hierarchy; children order is the stack order
//!   (fractional index). A node's metadata map holds `kind`, `removed`,
//!   `p:<name>` parameters and, for text nodes, a `text` container;
//! - root map `edges`: one string register per connection.
//!
//! Removal marks the node (`removed = true`) instead of deleting it from
//! the tree: Loro cannot bring a deleted tree node back, and a removal must
//! be undoable and restorable from a conflict with the same identity.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bivar_poc_modelo::detect::{self, Touch};
use bivar_poc_modelo::store::{ConcurrentWrite, NodeState};
use bivar_poc_modelo::{
    Anchor, Applied, CompactionPlan, Edge, MergeReport, NodeId, Op, OpError, State, Store, Value,
    random_u64,
};
use loro::cursor::{Cursor, Side};
use loro::{
    Container, ContainerID, ContainerType, ExportMode, Frontiers, ID, Index, JsonOpContent,
    JsonSchema, LoroDoc, LoroError, LoroMap, LoroText, LoroTree, LoroTreeError, LoroValue, TreeID,
    TreeParentId, ValueOrContainer, VersionVector,
};

/// Version of the layout above. Stored in the document; a document with
/// another version is refused, never guessed (migration: storage ADR).
/// 2: the removal mark became entries with the removal time (phase 0.6,
/// part 4). A layout change always raises it, so an older file is refused
/// instead of being read wrong (a file of layout 1 opened silently wrong
/// before this was raised).
pub const SCHEMA_VERSION: i64 = 2;

const SEP: char = '\u{1f}';

/// Loro's rule for "the last write" between concurrent writes to one map
/// key or one tree node: the higher Lamport time wins, then the higher
/// peer id. The conflict detection reproduces it, so a test locks it
/// (`tests/tie_break.rs`): if Loro changes the rule, that test fails.
pub fn last_write_order(lamport: u64, peer: u64) -> (u64, u64) {
    (lamport, peer)
}

/// How parameters are stored. Two ways are compared in phase 0.6 before
/// the owner decides (ADR-0021, section 5).
pub trait ParamMode {
    const ENTRIES: bool;
    const NAME: &'static str;
}

/// One register per parameter, key `p:<name>`. Loro picks the winner of
/// concurrent writes; the Bivar detects them by reproducing Loro's rule.
pub struct Register;

/// One entry per write, key `p:<name>#<counter>.<peer>` (both 16 hex
/// digits). A write removes only the entries it sees and adds its own with
/// counter = highest seen + 1, so concurrent writes leave several entries,
/// visible on every copy. The Bivar picks the winner: the highest key, that
/// is the highest counter, then the highest peer. No Loro rule involved.
/// Clearing a parameter is an entry with no value.
pub struct Entries;

impl ParamMode for Register {
    const ENTRIES: bool = false;
    const NAME: &'static str = "Loro (register)";
}

impl ParamMode for Entries {
    const ENTRIES: bool = true;
    const NAME: &'static str = "Loro (entries)";
}

pub struct LoroStore<M: ParamMode = Entries> {
    doc: LoroDoc,
    mode: std::marker::PhantomData<M>,
    /// Derived index, filled on demand: whether a node is visible (reachable
    /// from the roots, not marked removed, no ancestor marked removed). A
    /// question walks up only to the first node already known. Forgotten
    /// whenever the tree changes; never written back (ADR-0021, section 2).
    /// Built whole on opening, it cost 1.8 s on 1 million operations.
    visible: std::cell::RefCell<HashMap<TreeID, bool>>,
    /// Off only to measure the old way (walking up to the root).
    use_index: bool,
}

/// Parameters and the removal mark as one register each (compared and set
/// aside in phase 0.6; kept for the comparison tests).
pub type RegisterStore = LoroStore<Register>;

/// The chosen way (owner, 2026-10-29): parameters and the removal mark as
/// entries, so only moves depend on Loro's winner rule (ADR-0021).
pub type EntriesStore = LoroStore<Entries>;

/// The parameter name in a metadata key: `p:<name>` or `p:<name>#<entry>`.
fn param_name(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("p:")?;
    Some(rest.split('#').next().unwrap_or(rest))
}

/// A version stored as bytes under `key` in a map.
fn map_vv(map: &LoroMap, key: &str) -> Option<VersionVector> {
    match map.get(key) {
        Some(ValueOrContainer::Value(LoroValue::Binary(b))) => VersionVector::decode(&b).ok(),
        _ => None,
    }
}

/// Two opaque versions: everything in `a` is also in `b`.
pub fn version_within(a: &[u8], b: &[u8]) -> bool {
    match (VersionVector::decode(a), VersionVector::decode(b)) {
        (Ok(a), Ok(b)) => vv_le(&a, &b),
        _ => false,
    }
}

/// `a` is contained in `b`: everything in `a` is also in `b`.
fn vv_le(a: &VersionVector, b: &VersionVector) -> bool {
    a.iter().all(|(p, c)| b.get(p).copied().unwrap_or(0) >= *c)
}

/// Seconds since 1970, for housekeeping (removal age, checkpoints).
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Key prefix of the removal mark's entries (entries mode).
const REMOVED_PREFIX: &str = "removed#";

/// All entries under `prefix`, sorted so the winner is last.
fn raw_entries(meta: &LoroMap, prefix: &str) -> Vec<(String, LoroValue)> {
    let mut out: Vec<(String, LoroValue)> = meta
        .keys()
        .filter(|k| k.starts_with(prefix))
        .filter_map(|k| match meta.get(&k) {
            Some(ValueOrContainer::Value(v)) => Some((k.to_string(), v)),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Writes one entry under `prefix`: removes only the entries this copy
/// sees, then adds its own with counter = highest seen + 1.
fn write_entry(meta: &LoroMap, prefix: &str, value: LoroValue, peer: u64) -> Result<(), OpError> {
    let seen = raw_entries(meta, prefix);
    let counter = seen
        .iter()
        .filter_map(|(k, _)| k.get(prefix.len()..prefix.len() + 16))
        .filter_map(|c| u64::from_str_radix(c, 16).ok())
        .max()
        .map_or(0, |c| c + 1);
    for (k, _) in &seen {
        meta.delete(k).map_err(lib_err)?;
    }
    meta.insert(&format!("{prefix}{counter:016x}.{peer:016x}"), value)
        .map_err(lib_err)
}

/// All entries of one parameter, sorted so the winner is last.
fn param_entries(meta: &LoroMap, key: &str) -> Vec<(String, Option<Value>)> {
    raw_entries(meta, &format!("p:{key}#"))
        .into_iter()
        .map(|(k, v)| (k, from_loro(&v)))
        .collect()
}

fn lib_err(e: LoroError) -> OpError {
    match e {
        LoroError::TreeError(LoroTreeError::CyclicMoveError) => OpError::WouldCycle,
        other => OpError::Library(other.to_string()),
    }
}

fn tree_id(id: &NodeId) -> Result<TreeID, OpError> {
    TreeID::try_from(id.0.as_str()).map_err(|_| OpError::NotFound(id.0.clone()))
}

fn node_id(t: TreeID) -> NodeId {
    NodeId(t.to_string())
}

fn to_loro(v: Value) -> LoroValue {
    match v {
        Value::Num(n) => LoroValue::Double(n),
        Value::Str(s) => LoroValue::from(s),
    }
}

fn from_loro(v: &LoroValue) -> Option<Value> {
    match v {
        LoroValue::Double(n) => Some(Value::Num(*n)),
        LoroValue::String(s) => Some(Value::Str(s.to_string())),
        _ => None,
    }
}

fn encode_edge(e: &Edge) -> String {
    format!(
        "{}{SEP}{}{SEP}{}{SEP}{}",
        e.from, e.from_port, e.to, e.to_port
    )
}

fn decode_edge(s: &str) -> Option<Edge> {
    let mut it = s.split(SEP);
    let e = Edge {
        from: NodeId(it.next()?.to_owned()),
        from_port: it.next()?.to_owned(),
        to: NodeId(it.next()?.to_owned()),
        to_port: it.next()?.to_owned(),
    };
    it.next().is_none().then_some(e)
}

/// Everything a change had seen when it was made: its dependencies and the
/// earlier operations of its own peer.
fn change_past(doc: &LoroDoc, change: &loro::JsonChange) -> VersionVector {
    let deps: Frontiers = change.deps.iter().copied().collect();
    let mut past = doc.frontiers_to_vv(&deps).unwrap_or_default();
    if change.id.counter > 0 {
        let mut own = VersionVector::new();
        own.set_last(ID::new(change.id.peer, change.id.counter - 1));
        past.merge(&own);
    }
    past
}

/// What two writes compete for: one parameter or flag, or one node's place.
fn target(t: &Touch) -> Option<String> {
    match t {
        Touch::Param { node, key, .. } => Some(format!("p{SEP}{node}{SEP}{key}")),
        Touch::Remove { node } | Touch::Restore { node } => Some(format!("r{SEP}{node}")),
        Touch::Move { node, .. } => Some(format!("m{SEP}{node}")),
        _ => None,
    }
}

impl<M: ParamMode> LoroStore<M> {
    fn wrap(doc: LoroDoc) -> Self {
        let mut s = LoroStore {
            doc,
            mode: std::marker::PhantomData,
            visible: std::cell::RefCell::new(HashMap::new()),
            use_index: true,
        };
        s.rebuild_hidden();
        s
    }

    /// The version a named milestone points at (opaque bytes).
    pub fn version_of_milestone(&self, name: &str) -> Vec<u8> {
        map_vv(&self.doc.get_map("milestones"), name)
            .map(|v| v.encode())
            .unwrap_or_default()
    }

    /// Measurement only: turns the hidden-node index on or off.
    pub fn set_hidden_index(&mut self, on: bool) {
        self.use_index = on;
        self.rebuild_hidden();
    }

    /// After opening, merging or importing: forget what was known.
    fn rebuild_hidden(&mut self) {
        self.visible.get_mut().clear();
    }

    /// After a local remove, restore or move: the branch may have changed.
    fn refresh_branch(&mut self, _t: TreeID) {
        self.visible.get_mut().clear();
    }

    /// When the node was removed (seconds), if it is marked removed.
    fn removed_at(&self, t: TreeID) -> Option<u64> {
        let meta = self.tree().get_meta(t).ok()?;
        match raw_entries(&meta, REMOVED_PREFIX).pop()? {
            (_, LoroValue::I64(at)) => Some(at as u64),
            // Marked removed in register mode: no time, treat as old.
            (_, LoroValue::Bool(true)) => Some(0),
            _ => None,
        }
    }

    /// A node and everything under it.
    fn branch(&self, t: TreeID) -> Vec<TreeID> {
        let tree = self.tree();
        let mut out = Vec::new();
        let mut stack = vec![t];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(tree.children(TreeParentId::Node(n)).unwrap_or_default());
        }
        out
    }

    /// A conflict still on the node: two entries left by concurrent writes
    /// to a parameter or to the removal mark. Never cleaned (owner's rule).
    fn has_open_conflict(&self, t: TreeID) -> bool {
        let Ok(meta) = self.tree().get_meta(t) else {
            return false;
        };
        let mut counts: HashMap<String, usize> = HashMap::new();
        for key in meta.keys() {
            if let Some((name, _)) = key.split_once('#') {
                *counts.entry(name.to_owned()).or_default() += 1;
            }
        }
        counts.values().any(|c| *c > 1)
    }

    /// Entries mode: a parameter touched by this merge that now has more
    /// than one entry was written concurrently. The document itself shows
    /// it, so every copy finds the same thing; no history comparison and no
    /// Loro rule. The remove/restore flag is still a Loro register and keeps
    /// the history-based detection.
    fn entry_conflicts(&self, local: &[Touch], remote: &[Touch]) -> Vec<ConcurrentWrite> {
        let touched: BTreeSet<(NodeId, String)> = local
            .iter()
            .chain(remote)
            .filter_map(|t| match t {
                Touch::Param { node, key, .. } => Some((node.clone(), key.clone())),
                _ => None,
            })
            .collect();
        let flagged: BTreeSet<NodeId> = local
            .iter()
            .chain(remote)
            .filter_map(|t| match t {
                Touch::Remove { node } | Touch::Restore { node } => Some(node.clone()),
                _ => None,
            })
            .collect();
        let mut out = Vec::new();
        let word = |v: &LoroValue| {
            Value::Str(
                if matches!(v, LoroValue::Bool(true) | LoroValue::I64(_)) {
                    "removed"
                } else {
                    "restored"
                }
                .into(),
            )
        };
        let restored: BTreeSet<&NodeId> = local
            .iter()
            .chain(remote)
            .filter_map(|t| match t {
                Touch::Restore { node } => Some(node),
                _ => None,
            })
            .collect();
        for node in flagged {
            let Ok(t) = tree_id(&node) else { continue };
            // A restore of a node a compaction already purged cannot take
            // effect: it is reported, never dropped silently.
            if !self.exists(t) {
                if restored.contains(&node) {
                    out.push(ConcurrentWrite {
                        node,
                        key: detect::REMOVED_KEY.into(),
                        kept: Some(Value::Str("purged".into())),
                        lost: vec![Value::Str("restored".into())],
                    });
                }
                continue;
            }
            let Ok(meta) = self.tree().get_meta(t).map_err(lib_err) else {
                continue;
            };
            let mut entries = raw_entries(&meta, REMOVED_PREFIX);
            let Some((_, last)) = entries.pop() else {
                continue;
            };
            let kept = word(&last);
            // Two copies that both removed (or both restored) lost nothing.
            let lost: Vec<Value> = entries
                .iter()
                .map(|(_, v)| word(v))
                .filter(|v| *v != kept)
                .collect();
            if !lost.is_empty() {
                out.push(ConcurrentWrite {
                    node,
                    key: detect::REMOVED_KEY.into(),
                    kept: Some(kept),
                    lost: vec![lost[0].clone()],
                });
            }
        }
        for (node, key) in touched {
            let Ok(meta) = self.meta(&node) else { continue };
            let mut entries = param_entries(&meta, &key);
            if entries.len() < 2 {
                continue;
            }
            let kept = entries.pop().and_then(|e| e.1);
            let lost = entries.into_iter().filter_map(|e| e.1).collect();
            out.push(ConcurrentWrite {
                node,
                key,
                kept,
                lost,
            });
        }
        out
    }

    /// `(Lamport time, peer)` of the last write to a parameter in this
    /// copy's history, computed the way the conflict detection does. Used
    /// by the test that locks Loro's tie-break rule.
    pub fn last_param_write(&mut self, node: &NodeId, key: &str) -> Option<(u64, u64)> {
        self.doc.commit();
        let t = tree_id(node).ok()?;
        let all = self
            .json_between(&VersionVector::default(), &self.doc.oplog_vv())
            .ok()?;
        let k = format!("p:{key}");
        let mut last = None;
        for change in &all.changes {
            for op in &change.ops {
                let ContainerID::Normal { peer, counter, .. } = op.container else {
                    continue;
                };
                if (peer, counter) != (t.peer, t.counter) {
                    continue;
                }
                if let JsonOpContent::Map(loro::JsonMapOp::Insert { key, .. }) = &op.content
                    && *key == k
                {
                    let tick = (op.counter - change.id.counter) as u64;
                    let at = last_write_order(change.lamport as u64 + tick, change.id.peer);
                    last = last.max(Some(at));
                }
            }
        }
        last
    }

    fn tree(&self) -> LoroTree {
        self.doc.get_tree("tree")
    }

    fn exists(&self, t: TreeID) -> bool {
        let tree = self.tree();
        tree.contains(t) && !tree.is_node_deleted(&t).unwrap_or(true)
    }

    fn flag_removed(&self, t: TreeID) -> bool {
        let Ok(meta) = self.tree().get_meta(t) else {
            return true;
        };
        if M::ENTRIES {
            return matches!(
                raw_entries(&meta, REMOVED_PREFIX).pop(),
                Some((_, LoroValue::Bool(true) | LoroValue::I64(_)))
            );
        }
        matches!(
            meta.get("removed"),
            Some(ValueOrContainer::Value(LoroValue::Bool(true)))
        )
    }

    /// Exists, not marked removed, and no ancestor marked removed.
    fn is_visible(&self, t: TreeID) -> bool {
        if !self.use_index {
            return self.walk_visible(t);
        }
        if let Some(v) = self.visible.borrow().get(&t) {
            return *v;
        }
        let tree = self.tree();
        let mut chain = Vec::new();
        let mut cur = t;
        let known = loop {
            if let Some(v) = self.visible.borrow().get(&cur).copied() {
                break v;
            }
            chain.push(cur);
            match tree.parent(cur) {
                Some(TreeParentId::Node(p)) => cur = p,
                Some(TreeParentId::Root) => break true,
                // Deleted by a compaction, or unknown.
                _ => break false,
            }
        };
        let mut v = known;
        let mut memo = self.visible.borrow_mut();
        for n in chain.iter().rev() {
            v = v && !self.flag_removed(*n);
            memo.insert(*n, v);
        }
        memo[&t]
    }

    /// The same answer by walking up to the root (no index).
    fn walk_visible(&self, t: TreeID) -> bool {
        let tree = self.tree();
        let mut cur = t;
        loop {
            if !self.exists(cur) || self.flag_removed(cur) {
                return false;
            }
            match tree.parent(cur) {
                Some(TreeParentId::Node(p)) => cur = p,
                Some(TreeParentId::Root) => return true,
                _ => return false,
            }
        }
    }

    fn meta(&self, id: &NodeId) -> Result<LoroMap, OpError> {
        let t = tree_id(id)?;
        if !self.is_visible(t) {
            return Err(OpError::NotFound(id.0.clone()));
        }
        self.tree().get_meta(t).map_err(lib_err)
    }

    fn parent_arg(&self, parent: &Option<NodeId>) -> Result<TreeParentId, OpError> {
        Ok(match parent {
            None => TreeParentId::Root,
            Some(p) => {
                self.meta(p)?;
                TreeParentId::Node(tree_id(p)?)
            }
        })
    }

    /// Turns "position `index` among the visible siblings" into Loro's
    /// position among all tree children, which include nodes marked
    /// removed. Without this, a move lands in the wrong place, or on the
    /// node's own place, which Loro records as nothing.
    fn tree_index(&self, parent: TreeParentId, index: usize, moving: Option<TreeID>) -> usize {
        let all: Vec<TreeID> = self
            .tree()
            .children(parent)
            .unwrap_or_default()
            .into_iter()
            .filter(|c| Some(*c) != moving)
            .collect();
        let mut seen = 0;
        for (i, c) in all.iter().enumerate() {
            if !self.flag_removed(*c) {
                if seen == index {
                    return i;
                }
                seen += 1;
            }
        }
        all.len()
    }

    fn text_of(&self, id: &NodeId) -> Result<LoroText, OpError> {
        match self.meta(id)?.get("text") {
            Some(ValueOrContainer::Container(Container::Text(t))) => Ok(t),
            _ => Err(OpError::NotFound(format!("{id}: not a text node"))),
        }
    }

    fn set_removed(&self, id: &NodeId, removed: bool) -> Result<(), OpError> {
        let t = tree_id(id)?;
        if !self.exists(t) {
            return Err(OpError::NotFound(id.0.clone()));
        }
        let meta = self.tree().get_meta(t).map_err(lib_err)?;
        if M::ENTRIES {
            // A removal keeps its time (seconds), for the 30-day cleanup rule.
            // Housekeeping only: who wins is never decided by this clock.
            let v = if removed {
                LoroValue::I64(unix_now() as i64)
            } else {
                LoroValue::Bool(false)
            };
            return write_entry(&meta, REMOVED_PREFIX, v, self.doc.peer_id());
        }
        meta.insert("removed", removed).map_err(lib_err)
    }

    /// History between two versions. Fails with `HistoryTrimmed` when part of
    /// that range was cut by a compaction.
    fn json_between(
        &self,
        from: &VersionVector,
        to: &VersionVector,
    ) -> Result<JsonSchema, OpError> {
        self.doc
            .export_json_updates_without_peer_compression(from, to)
            .map_err(|e| self.trim_err(e))
    }

    /// Every change of the kept history, decoded.
    fn history_changes(&mut self) -> Vec<loro::JsonChange> {
        self.doc.commit();
        self.json_between(&self.history_start(), &self.doc.oplog_vv())
            .map(|all| all.changes)
            .unwrap_or_default()
    }

    /// Where the kept history starts: the beginning, or the point a
    /// compaction cut it at.
    fn history_start(&self) -> VersionVector {
        if self.doc.is_shallow() {
            self.doc
                .frontiers_to_vv(&self.doc.shallow_since_frontiers())
                .unwrap_or_default()
        } else {
            VersionVector::default()
        }
    }

    fn trim_err(&self, e: LoroError) -> OpError {
        match e {
            LoroError::ImportUpdatesThatDependsOnOutdatedVersion => OpError::HistoryTrimmed,
            _ if self.doc.is_shallow() => OpError::HistoryTrimmed,
            other => lib_err(other),
        }
    }

    fn node_of_container(&self, cid: &ContainerID) -> Option<NodeId> {
        let path = self.doc.get_path_to_container(cid)?;
        path.iter().rev().find_map(|(_, idx)| match idx {
            Index::Node(t) => Some(node_id(*t)),
            _ => None,
        })
    }

    /// Reads one copy's history between two versions as touches, each with
    /// its operation id and, if `past_from` is given, its causal past.
    fn touches(
        &self,
        schema: &JsonSchema,
        past_from: Option<&LoroDoc>,
    ) -> Vec<(Touch, ID, VersionVector)> {
        let mut keyed = Vec::new();
        for change in &schema.changes {
            let past = past_from
                .map(|d| change_past(d, change))
                .unwrap_or_default();
            for op in &change.ops {
                // The op's own Lamport time: a text insert spans one tick per
                // character, so the op index inside the change is not enough.
                let tick = (op.counter - change.id.counter) as u64;
                let order = last_write_order(change.lamport as u64 + tick, change.id.peer);
                let id = ID::new(change.id.peer, op.counter);
                let mut op_past = past.clone();
                if op.counter > 0 {
                    let mut own = VersionVector::new();
                    own.set_last(ID::new(change.id.peer, op.counter - 1));
                    op_past.merge(&own);
                }
                let touch = match (&op.container, &op.content) {
                    (
                        ContainerID::Normal {
                            peer,
                            counter,
                            container_type: ContainerType::Map,
                        },
                        JsonOpContent::Map(m),
                    ) => {
                        let node = node_id(TreeID::new(*peer, *counter));
                        let (key, value) = match m {
                            loro::JsonMapOp::Insert { key, value } => (key.as_str(), Some(value)),
                            loro::JsonMapOp::Delete { key } => (key.as_str(), None),
                        };
                        let flag = key == "removed" || key.starts_with(REMOVED_PREFIX);
                        match (key, value) {
                            (_, Some(LoroValue::Bool(true) | LoroValue::I64(_))) if flag => {
                                Touch::Remove { node }
                            }
                            (_, Some(_)) if flag => Touch::Restore { node },
                            // Entries mode: deleting an old entry is part of
                            // the write that replaces it, not an edit.
                            (k, None) if k.starts_with(REMOVED_PREFIX) => continue,
                            ("removed", None) => Touch::Restore { node },
                            (k, v) => match param_name(k) {
                                Some(k) => Touch::Param {
                                    node,
                                    key: k.to_owned(),
                                    value: v.and_then(from_loro),
                                },
                                None => continue,
                            },
                        }
                    }
                    (cid @ ContainerID::Normal { .. }, JsonOpContent::Text(_)) => {
                        match self.node_of_container(cid) {
                            Some(node) => Touch::Text { node },
                            None => continue,
                        }
                    }
                    (_, JsonOpContent::Tree(t)) => match t {
                        loro::JsonTreeOp::Create { target, .. } => Touch::Create {
                            node: node_id(*target),
                        },
                        loro::JsonTreeOp::Move {
                            target,
                            parent,
                            fractional_index,
                        } => Touch::Move {
                            node: node_id(*target),
                            parent: parent.map(node_id),
                            pos: fractional_index.to_string(),
                        },
                        // Bivar never deletes tree nodes (see the module docs).
                        loro::JsonTreeOp::Delete { .. } => continue,
                    },
                    _ => continue,
                };
                keyed.push((order, (touch, id, op_past)));
            }
        }
        keyed.sort_by_key(|k| k.0);
        keyed.into_iter().map(|(_, t)| t).collect()
    }
}

impl<M: ParamMode> Store for LoroStore<M> {
    const NAME: &'static str = M::NAME;

    fn new_document(peer: u64) -> Self {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).expect("valid peer id");
        doc.get_tree("tree").enable_fractional_index(0);
        let meta = doc.get_map("meta");
        // Random, so the identity says nothing about who made the document
        // or when (owner's request for the copy to send, 2026-10-30).
        meta.insert(
            "doc_id",
            format!("doc-{:016x}{:016x}", random_u64(), random_u64()),
        )
        .expect("doc id");
        meta.insert("schema", SCHEMA_VERSION).expect("schema");
        doc.commit();
        Self::wrap(doc)
    }

    fn fork_as(&mut self, peer: u64) -> Self {
        let bytes = self.save();
        Self::load(&bytes, peer).expect("own snapshot loads")
    }

    fn doc_id(&self) -> String {
        match self.doc.get_map("meta").get("doc_id") {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
            _ => String::new(),
        }
    }

    fn apply(&mut self, op: &Op) -> Result<Applied, OpError> {
        let tree = self.tree();
        let mut out = Applied::default();
        match op {
            Op::CreateNode {
                parent,
                index,
                kind,
            } => {
                let p = self.parent_arg(parent)?;
                let t = match index {
                    Some(i) => tree
                        .create_at(p, self.tree_index(p, *i, None))
                        .map_err(lib_err)?,
                    None => tree.create(p).map_err(lib_err)?,
                };
                let meta = tree.get_meta(t).map_err(lib_err)?;
                meta.insert("kind", kind.as_str()).map_err(lib_err)?;
                if kind == "text" {
                    meta.insert_container("text", LoroText::new())
                        .map_err(lib_err)?;
                }
                out.created = Some(node_id(t));
            }
            Op::RemoveNode { node } => {
                self.meta(node)?;
                self.set_removed(node, true)?;
                self.refresh_branch(tree_id(node)?);
            }
            Op::RestoreNode { node } => {
                if !self.flag_removed(tree_id(node)?) {
                    return Err(OpError::Invalid(format!("{node} is not removed")));
                }
                self.set_removed(node, false)?;
                self.refresh_branch(tree_id(node)?);
            }
            Op::MoveNode {
                node,
                parent,
                index,
            } => {
                self.meta(node)?;
                let p = self.parent_arg(parent)?;
                // The index counts visible siblings without the moved node.
                let t = tree_id(node)?;
                tree.mov_to(t, p, self.tree_index(p, *index, Some(t)))
                    .map_err(lib_err)?;
                self.refresh_branch(t);
            }
            Op::SetParam { node, key, value } => {
                let meta = self.meta(node)?;
                if M::ENTRIES {
                    let v = value.clone().map_or(LoroValue::Null, to_loro);
                    write_entry(&meta, &format!("p:{key}#"), v, self.doc.peer_id())?;
                } else {
                    let k = format!("p:{key}");
                    match value {
                        Some(v) => meta.insert(&k, to_loro(v.clone())),
                        None => meta.delete(&k),
                    }
                    .map_err(lib_err)?;
                }
            }
            Op::Connect { id, edge } => {
                self.meta(&edge.from)?;
                self.meta(&edge.to)?;
                let id = id
                    .clone()
                    .unwrap_or_else(|| format!("e{}.{:x}", self.doc.len_ops(), self.doc.peer_id()));
                self.doc
                    .get_map("edges")
                    .insert(&id, encode_edge(edge))
                    .map_err(lib_err)?;
                out.edge = Some(id);
            }
            Op::Disconnect { id } => {
                let edges = self.doc.get_map("edges");
                if edges.get(id).is_none() {
                    return Err(OpError::NotFound(id.clone()));
                }
                edges.delete(id).map_err(lib_err)?;
            }
            Op::InsertText { node, pos, text } => {
                self.text_of(node)?.insert(*pos, text).map_err(lib_err)?;
            }
            Op::DeleteText { node, pos, len } => {
                self.text_of(node)?.delete(*pos, *len).map_err(lib_err)?;
            }
        }
        Ok(out)
    }

    fn commit(&mut self) {
        self.doc.commit();
    }

    fn children(&self, parent: Option<&NodeId>) -> Vec<NodeId> {
        let p = match parent {
            None => TreeParentId::Root,
            Some(p) => match tree_id(p) {
                Ok(t) if self.is_visible(t) => TreeParentId::Node(t),
                _ => return Vec::new(),
            },
        };
        self.tree()
            .children(p)
            .unwrap_or_default()
            .into_iter()
            .filter(|c| !self.flag_removed(*c))
            .map(node_id)
            .collect()
    }

    fn parent(&self, id: &NodeId) -> Option<Option<NodeId>> {
        let t = tree_id(id).ok()?;
        if !self.is_visible(t) {
            return None;
        }
        self.stored_parent(id)
    }

    fn stored_parent(&self, id: &NodeId) -> Option<Option<NodeId>> {
        let t = tree_id(id).ok()?;
        match self.tree().parent(t)? {
            TreeParentId::Node(p) => Some(Some(node_id(p))),
            TreeParentId::Root | TreeParentId::Deleted => Some(None),
            TreeParentId::Unexist => None,
        }
    }

    fn is_removed(&self, id: &NodeId) -> bool {
        tree_id(id).is_ok_and(|t| self.flag_removed(t))
    }

    fn kind(&self, id: &NodeId) -> Option<String> {
        match self.meta(id).ok()?.get("kind")? {
            ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
            _ => None,
        }
    }

    fn param(&self, id: &NodeId, key: &str) -> Option<Value> {
        if M::ENTRIES {
            return param_entries(&self.meta(id).ok()?, key).pop()?.1;
        }
        match self.meta(id).ok()?.get(&format!("p:{key}"))? {
            ValueOrContainer::Value(v) => from_loro(&v),
            ValueOrContainer::Container(_) => None,
        }
    }

    fn text(&self, id: &NodeId) -> Option<String> {
        self.text_of(id).ok().map(|t| t.to_string())
    }

    fn edges(&self) -> BTreeMap<String, Edge> {
        let edges = self.doc.get_map("edges");
        let mut out = BTreeMap::new();
        for key in edges.keys() {
            if let Some(ValueOrContainer::Value(LoroValue::String(s))) = edges.get(&key)
                && let Some(e) = decode_edge(&s)
            {
                out.insert(key.to_string(), e);
            }
        }
        out
    }

    fn state(&self) -> State {
        let mut st = State {
            doc_id: self.doc_id(),
            roots: self.children(None),
            edges: self.edges(),
            ..State::default()
        };
        // One pass from the roots: a node reached from a visible parent is
        // visible unless marked, so no node walks up to the root again (a
        // per-node walk made reading 1M operations take 14 s).
        let tree = self.tree();
        let visible_children = |p: TreeParentId| -> Vec<TreeID> {
            tree.children(p)
                .unwrap_or_default()
                .into_iter()
                .filter(|c| !self.flag_removed(*c))
                .collect()
        };
        let mut stack: Vec<(Option<NodeId>, TreeID)> = visible_children(TreeParentId::Root)
            .into_iter()
            .map(|t| (None, t))
            .collect();
        while let Some((parent, t)) = stack.pop() {
            let id = node_id(t);
            let meta = tree.get_meta(t).expect("node exists");
            let children = visible_children(TreeParentId::Node(t));
            let mut ns = NodeState {
                parent,
                children: children.iter().map(|c| node_id(*c)).collect(),
                ..NodeState::default()
            };
            // Entries mode: keys sort so the last entry of each name wins.
            let mut keys: Vec<String> = meta.keys().map(|k| k.to_string()).collect();
            keys.sort();
            for key in keys {
                match (key.as_str(), meta.get(&key)) {
                    ("kind", Some(ValueOrContainer::Value(LoroValue::String(s)))) => {
                        ns.kind = s.to_string();
                    }
                    ("text", Some(ValueOrContainer::Container(Container::Text(txt)))) => {
                        ns.text = Some(txt.to_string());
                    }
                    (k, Some(ValueOrContainer::Value(v))) => {
                        if let Some(name) = param_name(k) {
                            match from_loro(&v) {
                                Some(v) => ns.params.insert(name.to_owned(), v),
                                None => ns.params.remove(name), // a cleared entry
                            };
                        }
                    }
                    _ => {}
                }
            }
            for c in children {
                stack.push((Some(id.clone()), c));
            }
            st.nodes.insert(id, ns);
        }
        st
    }

    fn text_anchor(&self, id: &NodeId, pos: usize) -> Option<Anchor> {
        let c = self.text_of(id).ok()?.get_cursor(pos, Side::Middle)?;
        Some(Anchor(c.encode()))
    }

    fn anchor_pos(&self, _id: &NodeId, anchor: &Anchor) -> Option<usize> {
        let c = Cursor::decode(&anchor.0).ok()?;
        Some(self.doc.get_cursor_pos(&c).ok()?.current.pos)
    }

    fn merge_from(&mut self, other: &mut Self) -> Result<MergeReport, OpError> {
        if self.doc_id() != other.doc_id() {
            return Err(OpError::DifferentDocument);
        }
        self.doc.commit();
        other.doc.commit();
        let mine = self.doc.oplog_vv();
        let theirs = other.doc.oplog_vv();
        // A copy that last met the other before that one cut its history (or
        // the other way round) cannot be merged: refused before importing
        // anything, so neither copy changes (the copy can be saved apart).
        if (other.doc.is_shallow() && !vv_le(&other.history_start(), &mine))
            || (self.doc.is_shallow() && !vv_le(&self.history_start(), &theirs))
        {
            return Err(OpError::HistoryTrimmed);
        }
        let updates = other.doc.export(ExportMode::updates(&mine)).map_err(|e| {
            if other.doc.is_shallow() {
                OpError::HistoryTrimmed
            } else {
                OpError::Library(e.to_string())
            }
        })?;
        let remote_json = other.json_between(&mine, &theirs)?;
        // Local operations concurrent with the incoming ones: those the
        // other copy had not seen. Starting points are the incoming changes
        // whose dependencies are all here already.
        let mut base: Option<VersionVector> = None;
        for c in &remote_json.changes {
            if c.deps.iter().any(|d| !mine.includes_id(*d)) {
                continue;
            }
            let past = change_past(&other.doc, c);
            base = Some(match base {
                None => past,
                Some(b) => b.intersection(&past),
            });
        }
        let base = base.unwrap_or_else(|| theirs.clone());
        let local_json = self.json_between(&base, &mine)?;
        let mut local = self.touches(&local_json, None);
        let local_plain: Vec<Touch> = local.iter().map(|x| x.0.clone()).collect();
        let mut visible_before = detect::visible_touched(self, &local_plain);
        self.doc.import(&updates).map_err(|e| self.trim_err(e))?;
        self.rebuild_hidden();
        // Text containers are resolved after the import, when they exist.
        let remote = self.touches(&remote_json, Some(&self.doc));
        let remote_plain: Vec<Touch> = remote.iter().map(|x| x.0.clone()).collect();
        // What the other copy showed when it made its edits: the other copy
        // without this copy's concurrent operations (it may have merged
        // them already). Those operations are, per peer, the range after
        // `base`, so cutting each such peer back to `base` is a valid version.
        let mut cut = theirs.clone();
        for (peer, end) in mine.iter() {
            let b = base.get(peer).copied().unwrap_or(0);
            if *end > b && cut.get(peer).is_some_and(|t| *t > b) {
                cut.set_end(ID::new(*peer, b));
            }
        }
        let frontiers = other.doc.vv_to_frontiers(&cut);
        let then = match other.doc.fork_at(&frontiers) {
            Ok(d) => d,
            // Loro has no `fork_at` for a compacted (shallow) document; a
            // copy moved back in time gives the same view.
            Err(_) => {
                let d = other.doc.fork();
                d.checkout(&frontiers).map_err(|e| self.trim_err(e))?;
                d
            }
        };
        visible_before.extend(detect::visible_touched(&Self::wrap(then), &remote_plain));
        // A local write counts only if the last incoming write to the same
        // target had not seen it.
        let mut remote_past: HashMap<String, VersionVector> = HashMap::new();
        for (t, _, past) in &remote {
            if let Some(k) = target(t) {
                remote_past.insert(k, past.clone());
            }
        }
        local.retain(|(t, id, _)| {
            target(t)
                .and_then(|k| remote_past.get(&k))
                .is_none_or(|p| !p.includes_id(*id))
        });
        let local: Vec<Touch> = local.into_iter().map(|x| x.0).collect();
        let remote: Vec<Touch> = remote.into_iter().map(|x| x.0).collect();
        let tree = self.tree();
        Ok(MergeReport {
            concurrent_writes: if M::ENTRIES {
                self.entry_conflicts(&local, &remote)
            } else {
                detect::concurrent_writes(self, &local, &remote)
            },
            lost_moves: detect::lost_moves(self, &local, &remote, |n| {
                tree.fractional_index(tree_id(n).ok()?)
            }),
            hidden_by_removal: detect::newly_hidden(self, &visible_before),
        })
    }

    fn save(&mut self) -> Vec<u8> {
        self.doc.commit();
        self.doc.export(ExportMode::Snapshot).expect("snapshot")
    }

    fn load(bytes: &[u8], peer: u64) -> Result<Self, OpError> {
        let doc = LoroDoc::from_snapshot(bytes).map_err(lib_err)?;
        match doc.get_map("meta").get("schema") {
            Some(ValueOrContainer::Value(LoroValue::I64(SCHEMA_VERSION))) => {}
            other => {
                return Err(OpError::Invalid(format!(
                    "unsupported document layout {other:?}"
                )));
            }
        }
        doc.set_peer_id(peer).map_err(lib_err)?;
        Ok(Self::wrap(doc))
    }

    fn history_dump(&mut self) -> String {
        self.doc.commit();
        format!(
            "{:?}",
            self.json_between(&self.history_start(), &self.doc.oplog_vv())
        )
    }

    fn history_peers(&mut self) -> BTreeSet<String> {
        self.history_changes()
            .iter()
            .map(|c| format!("{:x}", c.id.peer))
            .collect()
    }

    fn history_times(&mut self) -> BTreeSet<i64> {
        self.history_changes().iter().map(|c| c.timestamp).collect()
    }

    fn export_all(&mut self) -> Vec<u8> {
        self.doc.commit();
        self.doc
            .export(ExportMode::all_updates())
            .expect("own document exports")
    }

    fn version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }

    fn updates_since(&mut self, version: &[u8]) -> Result<Vec<u8>, OpError> {
        self.doc.commit();
        let vv = VersionVector::decode(version).map_err(lib_err)?;
        self.doc
            .export(ExportMode::updates(&vv))
            .map_err(|e| OpError::Library(e.to_string()))
    }

    fn import_updates(&mut self, bytes: &[u8]) -> Result<(), OpError> {
        self.doc.import(bytes).map_err(|e| self.trim_err(e))?;
        self.rebuild_hidden();
        Ok(())
    }

    fn import_updates_batch(&mut self, batch: &[Vec<u8>]) -> Result<(), OpError> {
        self.doc.import_batch(batch).map_err(|e| self.trim_err(e))?;
        self.rebuild_hidden();
        Ok(())
    }

    fn record_seen(&mut self, copy: &str, now: u64) {
        self.doc.commit();
        let vv = self.doc.oplog_vv().encode();
        // Each copy writes only its own keys, so these never conflict.
        let copies = self.doc.get_map("copies");
        copies
            .insert(&format!("{copy}#vv"), LoroValue::from(vv.clone()))
            .expect("copies map");
        copies
            .insert(&format!("{copy}#at"), now as i64)
            .expect("copies map");
        self.doc
            .get_map("checkpoints")
            .insert(
                &format!(
                    "{now:016x}.{:016x}.{:016x}",
                    self.doc.peer_id(),
                    self.doc.len_ops()
                ),
                LoroValue::from(vv),
            )
            .expect("checkpoints map");
        self.doc.commit();
    }

    fn add_milestone(&mut self, name: &str) {
        self.doc.commit();
        let vv = self.doc.oplog_vv().encode();
        self.doc
            .get_map("milestones")
            .insert(name, LoroValue::from(vv))
            .expect("milestones map");
        self.doc.commit();
    }

    fn plan_compaction(&self, now: u64, retention: u64, forget_stale: bool) -> CompactionPlan {
        let cutoff = now.saturating_sub(retention);
        // Known copies: what each has seen, and when it last said so.
        let copies = self.doc.get_map("copies");
        let mut known: Vec<(String, VersionVector, u64)> = Vec::new();
        for key in copies.keys() {
            let Some(name) = key.strip_suffix("#vv") else {
                continue;
            };
            let at = match copies.get(&format!("{name}#at")) {
                Some(ValueOrContainer::Value(LoroValue::I64(t))) => t as u64,
                _ => 0,
            };
            if let Some(vv) = map_vv(&copies, &key) {
                known.push((name.to_owned(), vv, at));
            }
        }
        // With `forget_stale`, copies silent for longer than the retention
        // no longer hold history back.
        let holders: Vec<&(String, VersionVector, u64)> = known
            .iter()
            .filter(|c| !forget_stale || c.2 >= cutoff)
            .collect();
        // Everything every holder has seen.
        let mut ack = self.doc.oplog_vv();
        for (_, vv, _) in &holders {
            ack = ack.intersection(vv);
        }
        let milestones: Vec<VersionVector> = {
            let m = self.doc.get_map("milestones");
            m.keys().filter_map(|k| map_vv(&m, &k)).collect()
        };
        // Trim point: the latest time checkpoint older than the retention
        // that every holder has seen and that no milestone needs.
        let cps = self.doc.get_map("checkpoints");
        let start = self.history_start();
        let trim = cps
            .keys()
            .filter_map(|k| {
                let time = u64::from_str_radix(k.get(..16)?, 16).ok()?;
                Some((time, map_vv(&cps, &k)?))
            })
            .filter(|(time, vv)| {
                *time <= cutoff
                    && vv_le(vv, &ack)
                    && milestones.iter().all(|m| vv_le(vv, m))
                    && !vv_le(vv, &start)
            })
            .max_by_key(|(time, _)| *time)
            .map(|(_, vv)| vv);
        let cut_off = match &trim {
            Some(t) => known
                .iter()
                .filter(|(_, vv, _)| !vv_le(t, vv))
                .map(|(name, _, _)| name.clone())
                .collect(),
            None => Vec::new(),
        };

        // When each removal happened, from the kept history.
        let mut removal_op: HashMap<TreeID, ID> = HashMap::new();
        if let Ok(all) = self.json_between(&start, &self.doc.oplog_vv()) {
            for change in &all.changes {
                for op in &change.ops {
                    let ContainerID::Normal {
                        peer,
                        counter,
                        container_type: ContainerType::Map,
                    } = op.container
                    else {
                        continue;
                    };
                    if let JsonOpContent::Map(loro::JsonMapOp::Insert {
                        key,
                        value: LoroValue::I64(_),
                    }) = &op.content
                        && key.starts_with(REMOVED_PREFIX)
                    {
                        removal_op.insert(
                            TreeID::new(peer, counter),
                            ID::new(change.id.peer, op.counter),
                        );
                    }
                }
            }
        }
        let tree = self.tree();
        let edges = self.edges();
        let mut plan = CompactionPlan {
            trim_before: trim.map(|v| v.encode()),
            cut_off,
            ..CompactionPlan::default()
        };
        for t in tree.nodes() {
            if !self.exists(t) || !self.flag_removed(t) {
                continue;
            }
            // Only the top removed node of a branch; the branch goes with it.
            if let Some(TreeParentId::Node(p)) = tree.parent(t)
                && !self.walk_visible(p)
            {
                continue;
            }
            let Some(removed_at) = self.removed_at(t) else {
                continue;
            };
            if removed_at > cutoff {
                continue;
            }
            // Every holder has seen the removal (or it is older than the kept
            // history, so everyone has).
            if let Some(id) = removal_op.get(&t)
                && !ack.includes_id(*id)
            {
                continue;
            }
            let branch = self.branch(t);
            if branch.iter().any(|n| self.has_open_conflict(*n)) {
                plan.kept_for_conflicts.push(node_id(t));
                continue;
            }
            let ids: HashSet<NodeId> = branch.iter().map(|n| node_id(*n)).collect();
            if edges
                .values()
                .any(|e| ids.contains(&e.from) || ids.contains(&e.to))
            {
                continue;
            }
            plan.purge.push(node_id(t));
        }
        plan
    }

    fn compact(&mut self, plan: &CompactionPlan) -> Result<Vec<u8>, OpError> {
        let tree = self.tree();
        for n in &plan.purge {
            let t = tree_id(n)?;
            if self.exists(t) {
                tree.delete(t).map_err(lib_err)?;
            }
        }
        self.doc.commit();
        let bytes = match &plan.trim_before {
            Some(v) => {
                let vv = VersionVector::decode(v).map_err(lib_err)?;
                let f = self.doc.vv_to_frontiers(&vv);
                self.doc
                    .export(ExportMode::ShallowSnapshot(std::borrow::Cow::Owned(f)))
            }
            None => self.doc.export(ExportMode::Snapshot),
        }
        .map_err(|e| OpError::Library(e.to_string()))?;
        // Continue from exactly what was written.
        let peer = self.doc.peer_id();
        let doc = LoroDoc::from_snapshot(&bytes).map_err(lib_err)?;
        doc.set_peer_id(peer).map_err(lib_err)?;
        *self = Self::wrap(doc);
        Ok(bytes)
    }

    fn open_conflicts(&self) -> Vec<String> {
        let tree = self.tree();
        let mut out = Vec::new();
        for t in tree.nodes() {
            if !self.is_visible(t) && !self.flag_removed(t) {
                continue; // under a removed node: its own removal is the conflict
            }
            let Ok(meta) = tree.get_meta(t) else {
                continue;
            };
            let mut counts: BTreeMap<String, Vec<LoroValue>> = BTreeMap::new();
            for key in meta.keys() {
                if let Some((name, _)) = key.split_once('#')
                    && let Some(ValueOrContainer::Value(v)) = meta.get(&key)
                {
                    // Removals at different times all mean "removed".
                    let v = match v {
                        LoroValue::I64(_) => LoroValue::Bool(true),
                        other => other,
                    };
                    counts.entry(name.to_owned()).or_default().push(v);
                }
            }
            for (name, values) in counts {
                // Two copies that wrote the same value lost nothing.
                let first = &values[0];
                if values.len() > 1 && values.iter().any(|v| v != first) {
                    let name = name.strip_prefix("p:").unwrap_or(&name);
                    out.push(format!("{}.{name}", node_id(t)));
                }
            }
        }
        out.sort();
        out
    }

    fn blob_hashes(&self) -> BTreeSet<String> {
        let tree = self.tree();
        let mut out = BTreeSet::new();
        for t in tree.nodes() {
            if !self.exists(t) {
                continue;
            }
            let Ok(meta) = tree.get_meta(t) else {
                continue;
            };
            for key in meta.keys() {
                if param_name(&key) == Some("blob")
                    && let Some(ValueOrContainer::Value(LoroValue::String(s))) = meta.get(&key)
                {
                    out.insert(s.to_string());
                }
            }
        }
        out
    }
}

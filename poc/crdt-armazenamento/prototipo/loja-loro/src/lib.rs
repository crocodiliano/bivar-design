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
/// 3: one entry per copy instead of one per write (audit 2026-10-31, item 3:
/// every write left a deleted key that Loro keeps forever).
pub const SCHEMA_VERSION: i64 = 3;

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

/// One entry per copy, key `p:<name>#<copy>` (16 hex digits), value
/// `[counter, value, seen]`. A write rewrites only its own copy's entry,
/// with counter = highest seen + 1 and `seen` = the counter of every other
/// copy's entry it saw. Nobody deletes another copy's entry: a delete racing
/// with that copy's new write to the same key could win in Loro and drop
/// the write silently. So no write leaves a deleted key behind, and a
/// parameter has at most one entry per copy that ever wrote it.
///
/// The winner is the highest counter, then the highest copy; no Loro rule
/// involved. Another copy's entry the winner had not seen is a concurrent
/// write, visible on every copy. Clearing a parameter is an entry with no
/// value. Two sessions with the same copy name write the same key; that is
/// found from the history on merging (`same_key_writes`).
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
    /// The data of each node read once, while `read_once` runs (the passes
    /// over the whole document share one reading of each node; owner,
    /// 2026-10-31). `None` outside it. Nothing can change the document
    /// while it is on: the passes only get a shared borrow.
    views: std::cell::RefCell<Option<HashMap<TreeID, std::rc::Rc<NodeView>>>>,
    /// The copy name in entry keys: stable across sessions of the same file
    /// on the same computer (kept outside the file, in the list of trusted
    /// files), unlike the Loro peer id, which is new for every session.
    /// Defaults to the peer id.
    copy: u64,
    /// Connections this session has made: with the session identity (never
    /// reused, I6), each new connection gets an identity of its own. The
    /// old one came from the operation count, which repeats inside one
    /// transaction and after a compaction (audit 2026-10-31, I1).
    edges_made: u64,
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

/// What the passes read of one node: its kind and its entries (entries
/// mode), read from Loro once.
struct NodeView {
    kind: Option<String>,
    entries: BTreeMap<String, Vec<Entry>>,
}

/// One copy's entry of a parameter or of the removal mark (layout 3).
struct Entry {
    copy: u64,
    counter: u64,
    value: LoroValue,
    /// Counter of each other copy's entry this write had seen.
    seen: HashMap<u64, u64>,
}

/// An entry's stored form `[counter, value, seen]`, decoded.
fn decode_entry(key: &str, prefix: &str, v: &LoroValue) -> Option<Entry> {
    let copy = u64::from_str_radix(key.strip_prefix(prefix)?, 16).ok()?;
    let LoroValue::List(parts) = v else {
        return None;
    };
    let [LoroValue::I64(counter), value, LoroValue::Map(seen)] = parts.as_slice() else {
        return None;
    };
    let seen = seen
        .iter()
        .filter_map(|(c, n)| match n {
            LoroValue::I64(n) => Some((u64::from_str_radix(c, 16).ok()?, *n as u64)),
            _ => None,
        })
        .collect();
    Some(Entry {
        copy,
        counter: *counter as u64,
        value: value.clone(),
        seen,
    })
}

/// The value inside a stored entry (layout 3), or the value itself (a
/// register). Reading history needs this: an op holds the stored form.
fn entry_value(v: &LoroValue) -> &LoroValue {
    match v {
        LoroValue::List(parts) if parts.len() == 3 => &parts[1],
        other => other,
    }
}

/// All entries under `prefix`, sorted so the winner is last.
fn entries(meta: &LoroMap, prefix: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = meta
        .keys()
        .filter(|k| k.starts_with(prefix))
        .filter_map(|k| match meta.get(&k) {
            Some(ValueOrContainer::Value(v)) => decode_entry(&k, prefix, &v),
            _ => None,
        })
        .collect();
    out.sort_by_key(|e| (e.counter, e.copy));
    out
}

/// All entries under `prefix` as `(copy, value)`, sorted so the winner is
/// last.
fn raw_entries(meta: &LoroMap, prefix: &str) -> Vec<(u64, LoroValue)> {
    entries(meta, prefix)
        .into_iter()
        .map(|e| (e.copy, e.value))
        .collect()
}

/// Every entry of a node, grouped by prefix (`p:<name>#`, `removed#`), each
/// group sorted so the winner is last. One pass over the keys: reading a
/// node with many parameters one prefix at a time cost the square of their
/// number (184 ms for 1,000 parameters, measured 2026-10-31).
fn entries_by_prefix(meta: &LoroMap) -> BTreeMap<String, Vec<Entry>> {
    let mut out: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for k in meta.keys() {
        if !(k.starts_with("p:") || k.starts_with(REMOVED_PREFIX)) {
            continue;
        }
        let Some(prefix) = k.len().checked_sub(16).and_then(|n| k.get(..n)) else {
            continue;
        };
        if !prefix.ends_with('#') {
            continue;
        }
        if let Some(ValueOrContainer::Value(v)) = meta.get(&k)
            && let Some(e) = decode_entry(&k, prefix, &v)
        {
            out.entry(prefix.to_owned()).or_default().push(e);
        }
    }
    for group in out.values_mut() {
        group.sort_by_key(|e| (e.counter, e.copy));
    }
    out
}

/// Values of the entries the winner (the last one) had not seen:
/// concurrent writes still open. Empty when there is no conflict.
fn unseen_of(sorted: &[Entry]) -> Vec<LoroValue> {
    let Some((winner, rest)) = sorted.split_last() else {
        return Vec::new();
    };
    rest.iter()
        .filter(|e| winner.seen.get(&e.copy).is_none_or(|n| *n < e.counter))
        .map(|e| e.value.clone())
        .collect()
}

/// `unseen_of` for the entries under one prefix.
fn unseen_values(meta: &LoroMap, prefix: &str) -> Vec<LoroValue> {
    unseen_of(&entries(meta, prefix))
}

/// Writes this copy's entry under `prefix`: counter = highest seen + 1, and
/// it records every other entry it saw. Never deletes another copy's entry.
fn write_entry(meta: &LoroMap, prefix: &str, value: LoroValue, copy: u64) -> Result<(), OpError> {
    let all = entries(meta, prefix);
    let counter = all.iter().map(|e| e.counter + 1).max().unwrap_or(0);
    let seen: HashMap<String, LoroValue> = all
        .iter()
        .filter(|e| e.copy != copy)
        .map(|e| (format!("{:016x}", e.copy), LoroValue::I64(e.counter as i64)))
        .collect();
    let stored = LoroValue::List(
        vec![
            LoroValue::I64(counter as i64),
            value,
            LoroValue::Map(seen.into()),
        ]
        .into(),
    );
    meta.insert(&format!("{prefix}{copy:016x}"), stored)
        .map_err(lib_err)
}

/// All entries of one parameter as `(copy, value)`, sorted so the winner is
/// last.
fn param_entries(meta: &LoroMap, key: &str) -> Vec<(u64, Option<Value>)> {
    raw_entries(meta, &format!("p:{key}#"))
        .into_iter()
        .map(|(c, v)| (c, from_loro(&v)))
        .collect()
}

/// How much a decoded value holds (bytes of text and data, plus one per
/// item), walked with an explicit stack: values can nest (I4).
fn value_size(v: &LoroValue) -> usize {
    let mut n = 0;
    let mut stack = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            LoroValue::String(s) => n += s.len(),
            LoroValue::Binary(b) => n += b.len(),
            LoroValue::List(l) => {
                n += 1;
                stack.extend(l.iter());
            }
            LoroValue::Map(m) => {
                n += 1;
                for (k, x) in m.iter() {
                    n += k.len();
                    stack.push(x);
                }
            }
            _ => n += 1,
        }
    }
    n
}

/// The whole content of a document as text, part by part, every part
/// decoded: each top-level part in full, then each tree node (removed or
/// not) with its parent, its place among its siblings and all its data.
/// Never the whole document as one value: that nests as deep as the tree,
/// and building it overflowed the stack at 10,000 levels (audit I4). Used
/// to decode everything (B1) and to compare two documents (re-rooting).
fn content_parts(doc: &LoroDoc) -> Vec<String> {
    let mut out = Vec::new();
    if let LoroValue::Map(roots) = doc.get_value() {
        let mut roots: Vec<(&String, &LoroValue)> = roots.iter().collect();
        roots.sort_by(|a, b| a.0.cmp(b.0));
        for (name, v) in roots {
            let LoroValue::Container(id) = v else {
                out.push(format!("{name}={v:?}"));
                continue;
            };
            out.push(match doc.get_container(id.clone()) {
                Some(Container::Map(m)) => format!("{name}={:?}", m.get_deep_value()),
                Some(Container::List(l)) => format!("{name}={:?}", l.get_deep_value()),
                Some(Container::MovableList(l)) => format!("{name}={:?}", l.get_deep_value()),
                Some(Container::Text(t)) => format!("{name}={}", t.to_string()),
                // The tree, node by node below.
                Some(Container::Tree(_)) => continue,
                other => format!("{name}={other:?}"),
            });
        }
    }
    let tree = doc.get_tree("tree");
    let mut nodes = tree.nodes();
    nodes.sort();
    for t in nodes {
        let meta = tree
            .get_meta(t)
            .map(|m| format!("{:?}", m.get_deep_value()))
            .unwrap_or_default();
        out.push(format!(
            "{t} {:?} {:?} {meta}",
            tree.parent(t),
            tree.fractional_index(t)
        ));
    }
    out
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
        let copy = doc.peer_id();
        let mut s = LoroStore {
            doc,
            mode: std::marker::PhantomData,
            visible: std::cell::RefCell::new(HashMap::new()),
            use_index: true,
            views: std::cell::RefCell::new(None),
            copy,
            edges_made: 0,
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
    /// A node's kind and entries: read from Loro, or, while `read_once`
    /// runs, from what was read the first time (reading a node's map costs
    /// more in a larger document, so each pass reading it again multiplied
    /// that cost; measured 2026-10-31).
    fn view(&self, t: TreeID) -> Option<std::rc::Rc<NodeView>> {
        if let Some(v) = self.views.borrow().as_ref().and_then(|m| m.get(&t)) {
            return Some(v.clone());
        }
        let meta = self.tree().get_meta(t).ok()?;
        let kind = match meta.get("kind") {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => Some(s.to_string()),
            _ => None,
        };
        let entries = if M::ENTRIES {
            entries_by_prefix(&meta)
        } else {
            BTreeMap::new()
        };
        let v = std::rc::Rc::new(NodeView { kind, entries });
        if let Some(m) = self.views.borrow_mut().as_mut() {
            m.insert(t, v.clone());
        }
        Some(v)
    }

    /// The winning entry's value under `prefix` of a node (entries mode).
    fn winner(&self, t: TreeID, prefix: &str) -> Option<LoroValue> {
        self.view(t)?
            .entries
            .get(prefix)?
            .last()
            .map(|e| e.value.clone())
    }

    fn removed_at(&self, t: TreeID) -> Option<u64> {
        // Register mode keeps no removal time (as before).
        match self.winner(t, REMOVED_PREFIX)? {
            LoroValue::I64(at) => Some(at as u64),
            // Marked removed in register mode: no time, treat as old.
            LoroValue::Bool(true) => Some(0),
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

    /// A conflict still on the node: an entry of a parameter or of the
    /// removal mark that the winning write had not seen. Never cleaned
    /// (owner's rule).
    fn has_open_conflict(&self, t: TreeID) -> bool {
        self.view(t)
            .is_some_and(|v| v.entries.values().any(|group| !unseen_of(group).is_empty()))
    }

    /// Entries mode: a parameter (or removal mark) touched by this merge
    /// with an entry the winner had not seen was written concurrently. The
    /// document itself shows it, so every copy finds the same thing; no
    /// history comparison and no Loro rule. Two sessions with the same copy
    /// name are found apart (`same_key_writes`).
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
            let Some((_, last)) = raw_entries(&meta, REMOVED_PREFIX).pop() else {
                continue;
            };
            let kept = word(&last);
            // Two copies that both removed (or both restored) lost nothing.
            let lost: Vec<Value> = unseen_values(&meta, REMOVED_PREFIX)
                .iter()
                .map(word)
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
        // Each node's entries read once (`touched` is sorted by node).
        let mut groups: Option<(NodeId, BTreeMap<String, Vec<Entry>>)> = None;
        for (node, key) in touched {
            if groups.as_ref().is_none_or(|(n, _)| *n != node) {
                let Ok(meta) = self.meta(&node) else { continue };
                groups = Some((node.clone(), entries_by_prefix(&meta)));
            }
            let Some((_, by_prefix)) = &groups else {
                continue;
            };
            let Some(group) = by_prefix.get(&format!("p:{key}#")) else {
                continue;
            };
            let kept = group.last().and_then(|e| from_loro(&e.value));
            let lost: Vec<Value> = unseen_of(group).iter().filter_map(from_loro).collect();
            if lost.is_empty() {
                continue;
            }
            out.push(ConcurrentWrite {
                node,
                key,
                kept,
                lost,
            });
        }
        out
    }

    /// Two sessions with the same copy name write the same entry key, and
    /// Loro keeps one write in silence. Found here from the history: the
    /// last incoming write to a key had not seen the last local write to
    /// that key. The losing value comes from the history, so it is reported,
    /// never dropped (owner's rule, 2026-10-31).
    fn same_key_writes(&self, local: &JsonSchema, remote: &JsonSchema) -> Vec<ConcurrentWrite> {
        type Key = (TreeID, String);
        let entry_ops = |schema: &JsonSchema, with_past: bool| {
            let mut last: HashMap<Key, (u64, ID, LoroValue, VersionVector)> = HashMap::new();
            for change in &schema.changes {
                let past = if with_past {
                    change_past(&self.doc, change)
                } else {
                    VersionVector::default()
                };
                for op in &change.ops {
                    let ContainerID::Normal {
                        peer,
                        counter,
                        container_type: ContainerType::Map,
                    } = op.container
                    else {
                        continue;
                    };
                    let JsonOpContent::Map(loro::JsonMapOp::Insert { key, value }) = &op.content
                    else {
                        continue;
                    };
                    if !(key.starts_with("p:") || key.starts_with(REMOVED_PREFIX)) {
                        continue;
                    }
                    let tick = (op.counter - change.id.counter) as u64;
                    let order = change.lamport as u64 + tick;
                    let k = (TreeID::new(peer, counter), key.clone());
                    if last.get(&k).is_none_or(|(o, ..)| *o <= order) {
                        let id = ID::new(change.id.peer, op.counter);
                        last.insert(k, (order, id, value.clone(), past.clone()));
                    }
                }
            }
            last
        };
        let mine = entry_ops(local, false);
        let theirs = entry_ops(remote, true);
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
        let mut out = Vec::new();
        for ((t, key), (_, my_id, my_value, _)) in &mine {
            let Some((_, _, their_value, their_past)) = theirs.get(&(*t, key.clone())) else {
                continue;
            };
            if their_past.includes_id(*my_id) || my_value == their_value {
                continue;
            }
            let Ok(meta) = self.tree().get_meta(*t) else {
                continue;
            };
            let now = match meta.get(key) {
                Some(ValueOrContainer::Value(v)) => v,
                _ => continue,
            };
            let lost = if now == *my_value {
                their_value
            } else {
                my_value
            };
            let node = node_id(*t);
            out.push(if key.starts_with(REMOVED_PREFIX) {
                ConcurrentWrite {
                    kept: Some(word(entry_value(&now))),
                    lost: vec![word(entry_value(lost))],
                    key: detect::REMOVED_KEY.into(),
                    node,
                }
            } else {
                let name = param_name(key).unwrap_or_default().to_owned();
                ConcurrentWrite {
                    kept: self.param(&node, &name),
                    lost: from_loro(entry_value(lost)).into_iter().collect(),
                    key: name,
                    node,
                }
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

    /// Every node still stored (removed or not, but not deleted by a
    /// compaction), in one call: `exists` on each node asks Loro whether it
    /// was deleted, which walks up the tree inside Loro every time.
    fn stored_nodes(&self) -> Vec<TreeID> {
        self.tree()
            .get_nodes(false)
            .into_iter()
            .map(|n| n.id)
            .collect()
    }

    fn exists(&self, t: TreeID) -> bool {
        let tree = self.tree();
        tree.contains(t) && !tree.is_node_deleted(&t).unwrap_or(true)
    }

    fn flag_removed(&self, t: TreeID) -> bool {
        if M::ENTRIES && self.views.borrow().is_some() {
            let Some(v) = self.view(t) else {
                return true;
            };
            return matches!(
                v.entries.get(REMOVED_PREFIX).and_then(|g| g.last()),
                Some(Entry {
                    value: LoroValue::Bool(true) | LoroValue::I64(_),
                    ..
                })
            );
        }
        // Outside `read_once`: only the removal entries are read.
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
            return write_entry(&meta, REMOVED_PREFIX, v, self.copy);
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
        // Only the errors that mean "before the kept history" (M13): any
        // other error of a compacted document was reported as a cut history,
        // which hid real errors.
        match e {
            LoroError::ImportUpdatesThatDependsOnOutdatedVersion
            | LoroError::SwitchToVersionBeforeShallowRoot => OpError::HistoryTrimmed,
            other => lib_err(other),
        }
    }

    /// Runs `look` on this copy moved back to `version`, then returns to the
    /// latest version, also when `look` fails or panics. While moved back
    /// the copy cannot be edited (Loro refuses edits on an old version) nor
    /// saved (`look` gets a shared borrow, and saving needs `&mut`).
    pub fn at_version<T>(
        &self,
        version: &Frontiers,
        look: impl FnOnce(&Self) -> T,
    ) -> Result<T, OpError> {
        /// Returns the document to the latest version when dropped.
        struct Back<'a> {
            doc: &'a LoroDoc,
            visible: &'a std::cell::RefCell<HashMap<TreeID, bool>>,
        }
        impl Drop for Back<'_> {
            fn drop(&mut self) {
                self.doc.checkout_to_latest();
                if let Ok(mut v) = self.visible.try_borrow_mut() {
                    v.clear();
                }
            }
        }
        self.doc.commit();
        let back = Back {
            doc: &self.doc,
            visible: &self.visible,
        };
        self.doc.checkout(version).map_err(|e| self.trim_err(e))?;
        self.visible.borrow_mut().clear();
        // What `read_once` read is of the latest version: set aside while
        // looking at the old one, and given back after.
        let read = self.views.borrow_mut().take();
        let out = look(self);
        *self.views.borrow_mut() = read;
        drop(back);
        Ok(out)
    }

    /// Whether every change of `history` that `point` (one operation, with
    /// version `at`) does not contain came after it: made by a copy that had
    /// received `point`. A change made beside it, by a copy that had not
    /// received it yet, makes the point unusable as the root of the kept
    /// history (ADR-0023, re-rooting; owner's case, 2026-10-31).
    fn everything_after_comes_after(
        &self,
        history: &JsonSchema,
        point: ID,
        at: &VersionVector,
    ) -> bool {
        history.changes.iter().all(|c| {
            let last = c.ops.last().map_or(c.id.counter, |op| op.counter);
            if at.includes_id(ID::new(c.id.peer, last)) {
                return true; // all of it is before the point
            }
            if c.id.peer == point.peer && c.id.counter <= point.counter {
                return true; // the point's own change, continued after it
            }
            change_past(&self.doc, c).includes_id(point)
        })
    }

    /// The document with its history cut at `point` (one operation): the
    /// version at `point` becomes the root, and every later change is
    /// replayed on it. Loro 1.17.1 cuts a history with several copies only
    /// back to a point it finds from the latest heads, which it does not
    /// find (locked by `tests/loro_assumptions.rs`). The state at `point`
    /// alone (`StateOnly`, which also works on a document already cut) is a
    /// document whose history starts at `point`. Checked before it is used:
    /// the same document, and a history that starts at `point`.
    fn reroot(&self, point: &Frontiers) -> Result<Vec<u8>, OpError> {
        let err = |e: loro::LoroEncodeError| OpError::Library(e.to_string());
        let root = self
            .doc
            .export(ExportMode::state_only(Some(point)))
            .map_err(err)?;
        let doc = LoroDoc::from_snapshot(&root).map_err(lib_err)?;
        let since = self
            .doc
            .frontiers_to_vv(point)
            .ok_or_else(|| OpError::Library("cut point not in the history".into()))?;
        let later = self.doc.export(ExportMode::updates(&since)).map_err(err)?;
        doc.import(&later).map_err(lib_err)?;
        if content_parts(&doc) != content_parts(&self.doc)
            || doc.oplog_vv() != self.doc.oplog_vv()
            || doc.shallow_since_frontiers() != *point
        {
            return Err(OpError::Library(
                "re-rooting changed the document; compaction refused".into(),
            ));
        }
        doc.export(ExportMode::Snapshot).map_err(err)
    }

    /// Whether this copy is showing the latest version (not moved back).
    pub fn is_at_latest(&self) -> bool {
        !self.doc.is_detached()
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
                            loro::JsonMapOp::Insert { key, value } => {
                                (key.as_str(), Some(entry_value(value)))
                            }
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
        op.validate()?;
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
                    write_entry(&meta, &format!("p:{key}#"), v, self.copy)?;
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
                let id = match id {
                    Some(id) => id.clone(),
                    None => {
                        self.edges_made += 1;
                        format!("e{:x}.{}", self.doc.peer_id(), self.edges_made)
                    }
                };
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

    fn is_stored(&self, id: &NodeId) -> bool {
        tree_id(id).is_ok_and(|t| self.exists(t))
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
            let children = visible_children(TreeParentId::Node(t));
            let mut ns = NodeState {
                parent,
                children: children.iter().map(|c| node_id(*c)).collect(),
                ..NodeState::default()
            };
            // While `read_once` runs: what was read of the node already; the
            // text container only for text nodes.
            if M::ENTRIES
                && self.views.borrow().is_some()
                && let Some(view) = self.view(t)
            {
                ns.kind = view.kind.clone().unwrap_or_default();
                if ns.kind == "text"
                    && let Ok(meta) = tree.get_meta(t)
                    && let Some(ValueOrContainer::Container(Container::Text(txt))) =
                        meta.get("text")
                {
                    ns.text = Some(txt.to_string());
                }
                for (prefix, group) in &view.entries {
                    if let (Some(name), Some(winner)) = (param_name(prefix), group.last())
                        && let Some(v) = from_loro(&winner.value)
                    {
                        ns.params.insert(name.to_owned(), v);
                    }
                }
                for c in children {
                    stack.push((Some(id.clone()), c));
                }
                st.nodes.insert(id, ns);
                continue;
            }
            let meta = tree.get_meta(t).expect("node exists");
            for key in meta.keys() {
                match (key.as_ref(), meta.get(&key)) {
                    ("kind", Some(ValueOrContainer::Value(LoroValue::String(s)))) => {
                        ns.kind = s.to_string();
                    }
                    ("text", Some(ValueOrContainer::Container(Container::Text(txt)))) => {
                        ns.text = Some(txt.to_string());
                    }
                    (k, Some(ValueOrContainer::Value(v))) if !M::ENTRIES => {
                        if let (Some(name), Some(v)) = (param_name(k), from_loro(&v)) {
                            ns.params.insert(name.to_owned(), v);
                        }
                    }
                    _ => {}
                }
            }
            // Entries mode: the winning entry of each parameter; a cleared
            // parameter's winner has no value.
            if M::ENTRIES {
                for (prefix, group) in entries_by_prefix(&meta) {
                    let Some(name) = param_name(&prefix) else {
                        continue;
                    };
                    if let Some(winner) = group.last()
                        && let Some(v) = from_loro(&winner.value)
                    {
                        ns.params.insert(name.to_owned(), v);
                    }
                }
            }
            for c in children {
                stack.push((Some(id.clone()), c));
            }
            st.nodes.insert(id, ns);
        }
        st
    }

    fn read_once<T>(&self, passes: impl FnOnce() -> T) -> T {
        /// Forgets what was read when dropped, also on a panic.
        struct Forget<'a, M: ParamMode>(&'a LoroStore<M>, bool);
        impl<M: ParamMode> Drop for Forget<'_, M> {
            fn drop(&mut self) {
                if self.1 {
                    *self.0.views.borrow_mut() = None;
                }
            }
        }
        // Only the outermost call starts and ends the reading.
        let outer = self.views.borrow().is_none();
        if outer {
            *self.views.borrow_mut() = Some(HashMap::new());
        }
        let _forget = Forget(self, outer);
        passes()
    }

    fn read_everything(&self) -> usize {
        // Every value is decoded and walked, never turned into text: the
        // text of a whole document of 1 million operations was 282 MB and
        // took 5 s to build (2026-10-31).
        let mut read = 0;
        // Every top-level part, one at a time; the tree node by node below
        // (the whole document as one value nests as deep as the tree).
        if let LoroValue::Map(roots) = self.doc.get_value() {
            for v in roots.values() {
                let LoroValue::Container(id) = v else {
                    read += value_size(v);
                    continue;
                };
                read += match self.doc.get_container(id.clone()) {
                    Some(Container::Map(m)) => value_size(&m.get_deep_value()),
                    Some(Container::List(l)) => value_size(&l.get_deep_value()),
                    Some(Container::MovableList(l)) => value_size(&l.get_deep_value()),
                    Some(Container::Text(t)) => t.len_utf8(),
                    _ => 0,
                };
            }
        }
        // Every node, removed (marked or cleaned) or not, with all its data.
        let tree = self.tree();
        for t in tree.nodes() {
            if let Ok(meta) = tree.get_meta(t) {
                read += value_size(&meta.get_deep_value());
            }
        }
        // The whole kept history, every change body decoded.
        if let Ok(h) = self.json_between(&self.history_start(), &self.doc.oplog_vv()) {
            for c in &h.changes {
                read += c.ops.len();
                for op in &c.ops {
                    if let JsonOpContent::Map(loro::JsonMapOp::Insert { value, .. }) = &op.content {
                        read += value_size(value);
                    }
                }
            }
        }
        read
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
        // A cut history was refused just above; any other error is a real
        // one, never reported as a cut history (M13).
        let updates = other
            .doc
            .export(ExportMode::updates(&mine))
            .map_err(|e| OpError::Library(e.to_string()))?;
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
        // Everything in `cut` is here now, so this copy moved back to `cut`
        // shows it. Moving back costs only the difference; building that
        // version apart (`fork_at`) cost time in proportion to the whole
        // history (278 ms per small merge at 1 million writes, 2026-10-31).
        let frontiers = self.doc.vv_to_frontiers(&cut);
        let then = self.at_version(&frontiers, |s| detect::visible_touched(s, &remote_plain))?;
        visible_before.extend(then);
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
        let concurrent_writes = if M::ENTRIES {
            let mut all = self.entry_conflicts(&local, &remote);
            all.extend(self.same_key_writes(&local_json, &remote_json));
            all
        } else {
            detect::concurrent_writes(self, &local, &remote)
        };
        Ok(MergeReport {
            concurrent_writes,
            lost_moves: detect::lost_moves(self, &local, &remote, |n| {
                tree.fractional_index(tree_id(n).ok()?)
            }),
            hidden_by_removal: detect::newly_hidden(self, &visible_before),
            too_deep: {
                // Only what moved can be deeper now; walked without
                // recursion (I4).
                let moved: Vec<NodeId> = local
                    .iter()
                    .chain(&remote)
                    .filter_map(|t| match t {
                        Touch::Move { node, .. } => Some(node.clone()),
                        _ => None,
                    })
                    .collect();
                let depth = bivar_poc_modelo::depth_through(self, &moved);
                (depth > bivar_poc_modelo::MAX_DEPTH).then_some(depth)
            },
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
        // Audit 2026-10-31, I6: a session never reuses an identity the
        // history already has.
        let used = doc.oplog_vv();
        let mut peer = peer;
        while used.get(&peer).is_some() {
            peer = random_u64();
        }
        doc.set_peer_id(peer).map_err(lib_err)?;
        Ok(Self::wrap(doc))
    }

    fn peer(&self) -> u64 {
        self.doc.peer_id()
    }

    fn copy_name(&self) -> u64 {
        self.copy
    }

    fn set_copy_name(&mut self, copy: u64) {
        self.copy = copy;
    }

    fn note(&self, key: &str) -> Option<String> {
        match self.doc.get_map("meta").get(&format!("note:{key}"))? {
            ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
            _ => None,
        }
    }

    fn set_note(&mut self, key: &str, value: &str) {
        self.doc
            .get_map("meta")
            .insert(&format!("note:{key}"), value)
            .expect("meta map");
        self.doc.commit();
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
        // The checkpoint is this one operation: a version with one head,
        // which a compaction can cut at (ADR-0023, re-rooting). Its key
        // names the operation: time, peer, counter.
        let peer = self.doc.peer_id();
        let counter = self.doc.oplog_vv().get(&peer).copied().unwrap_or(0);
        self.doc
            .get_map("checkpoints")
            .insert(
                &format!("{now:016x}.{peer:016x}.{counter:016x}"),
                LoroValue::Bool(true),
            )
            .expect("checkpoints map");
        self.doc.commit();
        debug_assert!(self.doc.oplog_vv().includes_id(ID::new(peer, counter)));
        // What this copy has seen, the checkpoint included. Each copy writes
        // only its own keys, so these never conflict.
        let vv = self.doc.oplog_vv().encode();
        let copies = self.doc.get_map("copies");
        copies
            .insert(&format!("{copy}#vv"), LoroValue::from(vv))
            .expect("copies map");
        copies
            .insert(&format!("{copy}#at"), now as i64)
            .expect("copies map");
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
        let start = self.history_start();
        let latest = self.doc.oplog_vv();
        // The kept history, decoded once: for the trim point and removals.
        let history = self.json_between(&start, &latest).ok();

        // Trim point: the latest checkpoint older than the retention that
        // every holder has seen, that no milestone needs, and that every
        // later change comes after (no change made beside it, without having
        // seen it): only such a point can be the root of the kept history.
        let cps = self.doc.get_map("checkpoints");
        let mut candidates: Vec<(u64, ID, VersionVector)> = cps
            .keys()
            .filter_map(|k| {
                let mut it = k.split('.');
                let time = u64::from_str_radix(it.next()?, 16).ok()?;
                let peer = u64::from_str_radix(it.next()?, 16).ok()?;
                let counter = u64::from_str_radix(it.next()?, 16).ok()?;
                let id = ID::new(peer, i32::try_from(counter).ok()?);
                if !latest.includes_id(id) {
                    return None;
                }
                let vv = self.doc.frontiers_to_vv(&Frontiers::from(id))?;
                Some((time, id, vv))
            })
            .filter(|(time, _, vv)| {
                *time <= cutoff
                    && vv_le(vv, &ack)
                    && milestones.iter().all(|m| vv_le(vv, m))
                    && !vv_le(vv, &start)
            })
            .collect();
        candidates.sort_by_key(|(time, id, _)| (*time, id.peer, id.counter));
        let mut why_not = None;
        let trim = match &history {
            None => {
                why_not = Some("the kept history could not be read".to_owned());
                None
            }
            Some(h) => {
                let found = candidates
                    .iter()
                    .rev()
                    .find(|(_, id, vv)| self.everything_after_comes_after(h, *id, vv))
                    .map(|(_, _, vv)| vv.clone());
                if found.is_none() {
                    why_not = Some(if candidates.is_empty() {
                        "no saved point older than the kept period has been seen by every \
                         known copy (and comes before every milestone)"
                            .to_owned()
                    } else {
                        "at every such point, some copy was editing without having received it \
                         yet, so no single point comes before everything kept"
                            .to_owned()
                    });
                }
                found
            }
        };
        let cut_off = match &trim {
            Some(t) => known
                .iter()
                .filter(|(_, vv, _)| !vv_le(t, vv))
                .map(|(name, _, _)| name.clone())
                .collect(),
            None => Vec::new(),
        };

        // Each removal, from the kept history. Without proof that every
        // holder received a removal, the node is not cleaned (I3): a removal
        // not found in the kept history, or a history that cannot be read,
        // blocks the cleaning.
        let mut removal_op: HashMap<TreeID, ID> = HashMap::new();
        for change in history.iter().flat_map(|h| &h.changes) {
            for op in &change.ops {
                let ContainerID::Normal {
                    peer,
                    counter,
                    container_type: ContainerType::Map,
                } = op.container
                else {
                    continue;
                };
                // The node's metadata map has the node's own identity
                // (locked by a test: `tests/loro_assumptions.rs`).
                if let JsonOpContent::Map(loro::JsonMapOp::Insert { key, value }) = &op.content
                    && matches!(entry_value(value), LoroValue::I64(_))
                    && key.starts_with(REMOVED_PREFIX)
                {
                    removal_op.insert(
                        TreeID::new(peer, counter),
                        ID::new(change.id.peer, op.counter),
                    );
                }
            }
        }
        // Nodes already marked removed where the kept history starts (a
        // compacted document only), read on that version.
        let removed_at_start: HashSet<TreeID> = if self.doc.is_shallow() {
            let start = self.doc.shallow_since_frontiers();
            self.at_version(&start, |s| {
                s.tree()
                    .nodes()
                    .into_iter()
                    .filter(|t| s.exists(*t) && s.flag_removed(*t))
                    .collect()
            })
            .unwrap_or_default()
        } else {
            HashSet::new()
        };
        // Every node a live connection touches, once: checking each removed
        // node against every connection took 82 s on 1 million operations
        // (measured 2026-10-31, when the verifier started running this plan).
        let connected: HashSet<NodeId> = self
            .edges()
            .into_values()
            .flat_map(|e| [e.from, e.to])
            .collect();
        let tree = self.tree();
        let mut plan = CompactionPlan {
            trim_before: trim.map(|v| v.encode()),
            cut_off,
            trim_refused: why_not,
            ..CompactionPlan::default()
        };
        // The nodes not deleted, in one pass: asking Loro node by node walks
        // up the tree inside it each time (verification of 1 million
        // operations, 2026-10-31).
        for t in self.stored_nodes() {
            if !self.flag_removed(t) {
                continue;
            }
            // Only the top removed node of a branch; the branch goes with it.
            // The visibility index answers for the parent.
            if let Some(TreeParentId::Node(p)) = tree.parent(t)
                && !self.is_visible(p)
            {
                continue;
            }
            let Some(removed_at) = self.removed_at(t) else {
                continue;
            };
            if removed_at > cutoff {
                continue;
            }
            // Proof that every holder has seen the removal: its operation in
            // the kept history and in what each holder confirmed; or, for a
            // removal older than the kept history, the node already removed
            // where that history starts (every copy that can still merge has
            // all of that). Anything else blocks the cleaning (I3).
            let proven = match removal_op.get(&t) {
                Some(id) => ack.includes_id(*id),
                None => removed_at_start.contains(&t),
            };
            if !proven {
                continue;
            }
            let branch = self.branch(t);
            if branch.iter().any(|n| self.has_open_conflict(*n)) {
                plan.kept_for_conflicts.push(node_id(t));
                continue;
            }
            if branch.iter().any(|n| connected.contains(&node_id(*n))) {
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
                self.reroot(&self.doc.vv_to_frontiers(&vv))?
            }
            None => self
                .doc
                .export(ExportMode::Snapshot)
                .map_err(|e| OpError::Library(e.to_string()))?,
        };
        // Continue from exactly what was written.
        let (peer, copy, edges_made) = (self.doc.peer_id(), self.copy, self.edges_made);
        let doc = LoroDoc::from_snapshot(&bytes).map_err(lib_err)?;
        doc.set_peer_id(peer).map_err(lib_err)?;
        *self = Self::wrap(doc);
        self.copy = copy;
        self.edges_made = edges_made;
        Ok(bytes)
    }

    fn open_conflicts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for t in self.stored_nodes() {
            if !self.is_visible(t) && !self.flag_removed(t) {
                continue; // under a removed node: its own removal is the conflict
            }
            let Some(view) = self.view(t) else {
                continue;
            };
            // Removals at different times all mean "removed".
            let norm = |v: LoroValue| match v {
                LoroValue::I64(_) => LoroValue::Bool(true),
                other => other,
            };
            for (prefix, group) in &view.entries {
                let Some(winner) = group.last() else {
                    continue;
                };
                let winner = norm(winner.value.clone());
                // Two copies that wrote the same value lost nothing.
                if unseen_of(group).into_iter().any(|v| norm(v) != winner) {
                    let name = prefix.trim_end_matches('#');
                    let name = name.strip_prefix("p:").unwrap_or(name);
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
        for t in self.stored_nodes() {
            // Every copy's entry, not only the winner: a value that lost a
            // concurrent write keeps its large file until the conflict goes.
            if M::ENTRIES {
                let Some(view) = self.view(t) else { continue };
                for e in view.entries.get("p:blob#").into_iter().flatten() {
                    if let LoroValue::String(s) = &e.value {
                        out.insert(s.to_string());
                    }
                }
                continue;
            }
            let Ok(meta) = tree.get_meta(t) else {
                continue;
            };
            for key in meta.keys() {
                if param_name(&key) == Some("blob")
                    && let Some(ValueOrContainer::Value(v)) = meta.get(&key)
                    && let LoroValue::String(s) = entry_value(&v)
                {
                    out.insert(s.to_string());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_history() -> (EntriesStore, Frontiers, NodeId) {
        let mut s = EntriesStore::new_document(1);
        let n = s
            .apply(&Op::CreateNode {
                parent: None,
                index: None,
                kind: "shape".into(),
            })
            .unwrap()
            .created
            .unwrap();
        s.commit();
        let old = s.doc.oplog_frontiers();
        s.apply(&Op::SetParam {
            node: n.clone(),
            key: "x".into(),
            value: Some(Value::Num(2.0)),
        })
        .unwrap();
        s.commit();
        (s, old, n)
    }

    #[test]
    fn a_look_back_shows_the_old_version_then_returns() {
        let (s, old, n) = doc_with_history();
        let then = s.at_version(&old, |v| v.param(&n, "x")).unwrap();
        assert_eq!(then, None);
        assert!(s.is_at_latest());
        assert_eq!(s.param(&n, "x"), Some(Value::Num(2.0)));
    }

    #[test]
    fn a_look_back_that_fails_still_returns_to_the_latest_version() {
        let (s, old, n) = doc_with_history();
        let r: Result<Result<(), &str>, OpError> = s.at_version(&old, |_| Err("failed"));
        assert_eq!(r, Ok(Err("failed")));
        assert!(s.is_at_latest());
        assert_eq!(s.param(&n, "x"), Some(Value::Num(2.0)));
    }

    #[test]
    fn a_look_back_that_panics_still_returns_to_the_latest_version() {
        let (mut s, old, n) = doc_with_history();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            s.at_version(&old, |_| panic!("in the middle"))
        }));
        assert!(r.is_err());
        assert!(s.is_at_latest());
        assert_eq!(s.param(&n, "x"), Some(Value::Num(2.0)));
        // Still editable, and the edit is saved.
        s.apply(&Op::SetParam {
            node: n.clone(),
            key: "x".into(),
            value: Some(Value::Num(3.0)),
        })
        .unwrap();
        let back = EntriesStore::load(&s.save(), 2).unwrap();
        assert_eq!(back.param(&n, "x"), Some(Value::Num(3.0)));
    }

    #[test]
    fn nothing_can_be_edited_while_moved_back() {
        let (s, old, _) = doc_with_history();
        let edit = s
            .at_version(&old, |v| v.doc.get_map("meta").insert("k", 1))
            .unwrap();
        assert!(edit.is_err(), "an edit on an old version went through");
        assert!(s.is_at_latest());
        assert!(s.doc.get_map("meta").get("k").is_none());
    }
}

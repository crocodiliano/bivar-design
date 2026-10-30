//! The socket implemented with Automerge.
//!
//! Layout: root map `nodes` (one map per node, keyed by node id) and root
//! map `edges` (one string register per edge). Inside a node map: `kind`,
//! `p:<name>` parameters, `text` (a Text object, created with the node),
//! `removed` (soft removal) and `mv`.
//!
//! Missing in Automerge and built here (minimal versions):
//!
//! - **Safe move.** `mv` is one register holding "parent, position, previous
//!   parent". Two concurrent moves of the same node write the same register,
//!   so one wins and the node is never duplicated; `get_all` shows the
//!   loser. Two moves that together form a cycle (A into B while B into A)
//!   are both stored; readers break the cycle the same way on every copy:
//!   the move with the highest operation id goes back to its previous parent.
//! - **Sibling order.** `frac_index` keys inside `mv`.
//! - **Undo of one's own changes.** An inverse is recorded for each local
//!   operation; undo applies it only if nobody changed that value since.
//!
//! Built in: detecting concurrent writes (`get_all`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use automerge::legacy::{Key as LegacyKey, OpType as LegacyOp};
use automerge::transaction::Transactable;
use automerge::{
    ActorId, AutoCommit, Cursor, ObjId, ObjType, ROOT, ReadDoc, ScalarValue, Value as AmValue,
};

use crate::frac_index;
use crate::model::{
    ConcurrentWrite, Edge, LostMove, MergeReport, NodeId, NodeState, OpError, Replica, State,
    Touch, UndoOutcome, Value, hidden_by_removal,
};

const SEP: char = '\u{1f}';
/// Same as `set_max_undo_steps` in the Loro proof.
pub const MAX_UNDO_STEPS: usize = 10_000;

/// The `mv` register: where the node is and where it was before.
#[derive(Clone, Debug, PartialEq)]
struct Placement {
    parent: Option<NodeId>,
    pos: String,
    prev: Option<NodeId>,
}

impl Placement {
    fn encode(&self) -> String {
        format!(
            "{}{SEP}{}{SEP}{}",
            self.parent.as_deref().unwrap_or(""),
            self.pos,
            self.prev.as_deref().unwrap_or("")
        )
    }

    fn decode(s: &str) -> Option<Placement> {
        let mut it = s.split(SEP);
        let opt = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        let p = Placement {
            parent: opt(it.next()?),
            pos: it.next()?.to_owned(),
            prev: opt(it.next()?),
        };
        it.next().is_none().then_some(p)
    }
}

#[derive(Clone, Debug)]
struct NodeInfo {
    obj: ObjId,
    text: Option<ObjId>,
    kind: String,
    removed: bool,
    stored: Placement,
    /// Operation id of the winning `mv` write, used to break cycles.
    mv_op: ObjId,
}

/// Derived tree, rebuilt after merge, load and undo; updated in place for
/// local edits. This index is part of the cost of building the move outside
/// the library.
#[derive(Default)]
struct Index {
    nodes: HashMap<NodeId, NodeInfo>,
    /// Effective parent after cycle breaking.
    parent: HashMap<NodeId, Option<NodeId>>,
    /// Children sorted by (position, id), including removed ones.
    children: HashMap<Option<NodeId>, Vec<(String, NodeId)>>,
    obj_to_node: HashMap<String, NodeId>,
    /// Nodes whose stored move was overridden to break a cycle.
    broken: HashMap<NodeId, Option<NodeId>>,
}

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
    Moved {
        node: NodeId,
        wrote: String,
        old: String,
    },
    Connected {
        edge: String,
    },
    Disconnected {
        edge: String,
        encoded: String,
    },
    TextInserted {
        node: NodeId,
        start: Cursor,
        text: String,
    },
    TextDeleted {
        node: NodeId,
        before: Option<Cursor>,
        text: String,
    },
}

pub struct AutomergeReplica {
    doc: AutoCommit,
    nodes_obj: ObjId,
    edges_obj: ObjId,
    index: Index,
    undo_stack: VecDeque<Vec<Inverse>>,
    pending: Vec<Inverse>,
}

fn lib_err(e: automerge::AutomergeError) -> OpError {
    OpError::Library(e.to_string())
}

fn actor(peer: u64) -> ActorId {
    ActorId::from(peer.to_be_bytes().as_slice())
}

fn to_scalar(v: Value) -> ScalarValue {
    match v {
        Value::Num(n) => ScalarValue::F64(n),
        Value::Str(s) => ScalarValue::Str(s.into()),
    }
}

fn from_scalar(v: &ScalarValue) -> Option<Value> {
    match v {
        ScalarValue::F64(n) => Some(Value::Num(*n)),
        ScalarValue::Str(s) => Some(Value::Str(s.to_string())),
        _ => None,
    }
}

fn from_am(v: &AmValue) -> Option<Value> {
    match v {
        AmValue::Scalar(s) => from_scalar(s),
        AmValue::Object(_) => None,
    }
}

fn get_str(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<(String, ObjId)> {
    match doc.get(obj, key).ok()?? {
        (AmValue::Scalar(s), id) => match s.as_ref() {
            ScalarValue::Str(s) => Some((s.to_string(), id)),
            _ => None,
        },
        _ => None,
    }
}

impl AutomergeReplica {
    fn wrap(doc: AutoCommit) -> Result<Self, OpError> {
        let obj = |doc: &AutoCommit, key: &str| match doc.get(ROOT, key) {
            Ok(Some((AmValue::Object(ObjType::Map), id))) => Ok(id),
            _ => Err(OpError::Library(format!("missing root map {key}"))),
        };
        let nodes_obj = obj(&doc, "nodes")?;
        let edges_obj = obj(&doc, "edges")?;
        let mut r = AutomergeReplica {
            doc,
            nodes_obj,
            edges_obj,
            index: Index::default(),
            undo_stack: VecDeque::new(),
            pending: Vec::new(),
        };
        r.rebuild_index();
        Ok(r)
    }

    fn rebuild_index(&mut self) {
        let mut idx = Index::default();
        let entries: Vec<(String, ObjId)> = self
            .doc
            .map_range(&self.nodes_obj, ..)
            .map(|item| (item.key.to_string(), item.id()))
            .collect();
        for (id, obj) in entries {
            let kind = get_str(&self.doc, &obj, "kind")
                .map(|k| k.0)
                .unwrap_or_default();
            let removed = matches!(
                self.doc.get(&obj, "removed"),
                Ok(Some((AmValue::Scalar(s), _))) if *s == ScalarValue::Boolean(true)
            );
            let Some((mv, mv_op)) = get_str(&self.doc, &obj, "mv") else {
                continue;
            };
            let Some(stored) = Placement::decode(&mv) else {
                continue;
            };
            let text = match self.doc.get(&obj, "text") {
                Ok(Some((AmValue::Object(ObjType::Text), t))) => Some(t),
                _ => None,
            };
            idx.obj_to_node.insert(obj.to_string(), id.clone());
            if let Some(t) = &text {
                idx.obj_to_node.insert(t.to_string(), id.clone());
            }
            idx.nodes.insert(
                id,
                NodeInfo {
                    obj,
                    text,
                    kind,
                    removed,
                    stored,
                    mv_op,
                },
            );
        }
        // Effective parents: stored ones, unknown parents become roots.
        for (id, info) in &idx.nodes {
            let p = info
                .stored
                .parent
                .clone()
                .filter(|p| idx.nodes.contains_key(p));
            idx.parent.insert(id.clone(), p);
        }
        Self::break_cycles(&mut idx);
        for (id, p) in &idx.parent {
            let pos = idx.nodes[id].stored.pos.clone();
            idx.children
                .entry(p.clone())
                .or_default()
                .push((pos, id.clone()));
        }
        for list in idx.children.values_mut() {
            list.sort();
        }
        self.index = idx;
    }

    /// Same result on every copy: it only reads data all copies share.
    fn break_cycles(idx: &mut Index) {
        let ids: Vec<NodeId> = {
            let mut v: Vec<_> = idx.nodes.keys().cloned().collect();
            v.sort();
            v
        };
        let mut attempts: HashMap<NodeId, u8> = HashMap::new();
        loop {
            let Some(cycle) = Self::find_cycle(idx, &ids) else {
                return;
            };
            let loser = cycle
                .iter()
                .max_by(|a, b| idx.nodes[*a].mv_op.cmp(&idx.nodes[*b].mv_op))
                .expect("a cycle has nodes")
                .clone();
            let tries = attempts.entry(loser.clone()).or_default();
            *tries += 1;
            let info = &idx.nodes[&loser];
            let fallback = if *tries == 1 {
                info.stored
                    .prev
                    .clone()
                    .filter(|p| idx.nodes.contains_key(p) && *p != loser)
            } else {
                None
            };
            idx.broken.insert(loser.clone(), info.stored.parent.clone());
            idx.parent.insert(loser, fallback);
        }
    }

    fn find_cycle(idx: &Index, ids: &[NodeId]) -> Option<Vec<NodeId>> {
        // 0 = unseen, 1 = on the current path, 2 = known acyclic.
        let mut color: HashMap<&str, u8> = HashMap::new();
        for start in ids {
            let mut path: Vec<&str> = Vec::new();
            let mut cur = Some(start.as_str());
            while let Some(n) = cur {
                match color.get(n).copied().unwrap_or(0) {
                    2 => break,
                    1 => {
                        let at = path.iter().position(|p| *p == n).expect("on path");
                        return Some(path[at..].iter().map(|s| s.to_string()).collect());
                    }
                    _ => {
                        color.insert(n, 1);
                        path.push(n);
                        cur = idx.parent.get(n).and_then(|p| p.as_deref());
                    }
                }
            }
            for n in path {
                color.insert(n, 2);
            }
        }
        None
    }

    fn info(&self, id: &str) -> Result<&NodeInfo, OpError> {
        if !self.is_visible(id) {
            return Err(OpError::NotFound(id.to_owned()));
        }
        Ok(&self.index.nodes[id])
    }

    fn is_visible(&self, id: &str) -> bool {
        let mut cur = Some(id.to_owned());
        let mut steps = 0;
        while let Some(n) = cur {
            match self.index.nodes.get(&n) {
                Some(info) if !info.removed => {}
                _ => return false,
            }
            steps += 1;
            if steps > self.index.nodes.len() {
                return false;
            }
            cur = self.index.parent.get(&n).cloned().flatten();
        }
        true
    }

    fn is_ancestor(&self, maybe_ancestor: &str, of: &str) -> bool {
        let mut cur = Some(of.to_owned());
        while let Some(n) = cur {
            if n == maybe_ancestor {
                return true;
            }
            cur = self.index.parent.get(&n).cloned().flatten();
        }
        false
    }

    fn visible_children(&self, parent: &Option<NodeId>) -> Vec<(String, NodeId)> {
        self.index
            .children
            .get(parent)
            .map(|v| {
                v.iter()
                    .filter(|(_, c)| !self.index.nodes[c].removed)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn new_id(&mut self, prefix: &str) -> String {
        let ops = self.doc.stats().num_ops + self.doc.pending_ops() as u64;
        format!("{prefix}{ops}.{}", self.doc.get_actor().to_hex_string())
    }

    fn set_placement(&mut self, id: &str, p: Placement) -> Result<String, OpError> {
        let encoded = p.encode();
        let obj = self.index.nodes[id].obj.clone();
        self.doc
            .put(&obj, "mv", encoded.as_str())
            .map_err(lib_err)?;
        // Local index update: detach, reattach, keep order.
        let old_parent = self.index.parent.get(id).cloned().flatten();
        if let Some(list) = self.index.children.get_mut(&old_parent) {
            list.retain(|(_, c)| c != id);
        }
        let list = self.index.children.entry(p.parent.clone()).or_default();
        let at = list.partition_point(|e| *e < (p.pos.clone(), id.to_owned()));
        list.insert(at, (p.pos.clone(), id.to_owned()));
        self.index.parent.insert(id.to_owned(), p.parent.clone());
        let mv_op = get_str(&self.doc, &obj, "mv")
            .map(|x| x.1)
            .expect("just written");
        let info = self.index.nodes.get_mut(id).expect("known node");
        info.stored = p;
        info.mv_op = mv_op;
        Ok(encoded)
    }

    fn position_key(&self, parent: &Option<NodeId>, index: usize, moving: Option<&str>) -> String {
        let sibs: Vec<(String, NodeId)> = self
            .visible_children(parent)
            .into_iter()
            .filter(|(_, c)| Some(c.as_str()) != moving)
            .collect();
        let i = index.min(sibs.len());
        let lo = i.checked_sub(1).map(|j| sibs[j].0.as_str());
        // Skip neighbours that share the key of `lo` (ties).
        let hi = sibs[i..]
            .iter()
            .map(|(p, _)| p.as_str())
            .find(|p| lo.is_none_or(|lo| *p > lo));
        frac_index::between(lo, hi)
    }

    fn put_removed(&mut self, id: &str, removed: bool) -> Result<(), OpError> {
        let obj = self.index.nodes[id].obj.clone();
        self.doc.put(&obj, "removed", removed).map_err(lib_err)?;
        self.index.nodes.get_mut(id).expect("known").removed = removed;
        Ok(())
    }

    fn text_obj(&self, id: &str) -> Result<ObjId, OpError> {
        self.info(id)?
            .text
            .clone()
            .ok_or_else(|| OpError::NotFound(format!("{id}: not a text node")))
    }

    fn touches(&self, changes: &[automerge::Change]) -> Vec<Touch> {
        let nodes_obj = self.nodes_obj.to_string();
        let mut out = Vec::new();
        for change in changes {
            for op in change.decode().operations {
                let obj = op.obj.to_string();
                let key = match &op.key {
                    LegacyKey::Map(k) => Some(k.as_str()),
                    LegacyKey::Seq(_) => None,
                };
                if obj == nodes_obj {
                    if let (Some(k), LegacyOp::Make(_)) = (key, &op.action) {
                        out.push(Touch::Create { node: k.to_owned() });
                    }
                    continue;
                }
                let Some(node) = self.index.obj_to_node.get(&obj).cloned() else {
                    continue;
                };
                let t = match (key, &op.action) {
                    (None, _) => Touch::Text { node },
                    (Some(k), action) if k.starts_with("p:") => Touch::Param {
                        node,
                        key: k[2..].to_owned(),
                        value: match action {
                            LegacyOp::Put(v) => from_scalar(v),
                            _ => None,
                        },
                    },
                    (Some("mv"), LegacyOp::Put(ScalarValue::Str(s))) => {
                        let Some(p) = Placement::decode(s) else {
                            continue;
                        };
                        Touch::Move {
                            node,
                            parent: p.parent,
                            pos: p.pos,
                        }
                    }
                    (Some("removed"), LegacyOp::Put(ScalarValue::Boolean(true))) => {
                        Touch::Remove { node }
                    }
                    _ => continue,
                };
                out.push(t);
            }
        }
        out
    }

    fn apply_inverse(&mut self, inv: Inverse) -> Result<Option<String>, OpError> {
        let skipped = |what: &str| Ok(Some(what.to_owned()));
        match inv {
            Inverse::Param {
                node,
                key,
                wrote,
                old,
            } => {
                if !self.index.nodes.contains_key(&node) {
                    return skipped("parameter of a node that no longer exists");
                }
                let obj = self.index.nodes[&node].obj.clone();
                let k = format!("p:{key}");
                let now = match self.doc.get(&obj, k.as_str()).map_err(lib_err)? {
                    Some((v, _)) => from_am(&v),
                    None => None,
                };
                if now != wrote {
                    return skipped(&format!("{key}: changed by another copy after this edit"));
                }
                match old {
                    Some(v) => self.doc.put(&obj, k.as_str(), to_scalar(v)),
                    None => self.doc.delete(&obj, k.as_str()),
                }
                .map_err(lib_err)?;
            }
            Inverse::Created { node } => self.put_removed(&node, true)?,
            Inverse::Removed { node } => self.put_removed(&node, false)?,
            Inverse::Moved { node, wrote, old } => {
                let now = self.index.nodes.get(&node).map(|i| i.stored.encode());
                if now.as_deref() != Some(wrote.as_str()) {
                    return skipped("move: moved again by another copy after this edit");
                }
                let back = Placement::decode(&old).expect("own encoding");
                if let Some(p) = &back.parent
                    && self.is_ancestor(&node, p)
                {
                    return skipped("move: going back would form a cycle");
                }
                self.set_placement(&node, back)?;
            }
            Inverse::Connected { edge } => {
                if self
                    .doc
                    .get(&self.edges_obj, edge.as_str())
                    .map_err(lib_err)?
                    .is_some()
                {
                    self.doc
                        .delete(&self.edges_obj, edge.as_str())
                        .map_err(lib_err)?;
                }
            }
            Inverse::Disconnected { edge, encoded } => {
                self.doc
                    .put(&self.edges_obj, edge.as_str(), encoded.as_str())
                    .map_err(lib_err)?;
            }
            Inverse::TextInserted { node, start, text } => {
                let Ok(obj) = self.text_obj(&node) else {
                    return skipped("text of a node that no longer exists");
                };
                let pos = self
                    .doc
                    .get_cursor_position(&obj, &start, None)
                    .map_err(lib_err)?;
                let len = text.chars().count();
                let current: String = self
                    .doc
                    .text(&obj)
                    .map_err(lib_err)?
                    .chars()
                    .skip(pos)
                    .take(len)
                    .collect();
                if current != text {
                    return skipped("text: another copy wrote inside the inserted text");
                }
                self.doc
                    .splice_text(&obj, pos, len as isize, "")
                    .map_err(lib_err)?;
            }
            Inverse::TextDeleted { node, before, text } => {
                let Ok(obj) = self.text_obj(&node) else {
                    return skipped("text of a node that no longer exists");
                };
                let pos = match before {
                    Some(c) => {
                        self.doc
                            .get_cursor_position(&obj, &c, None)
                            .map_err(lib_err)?
                            + 1
                    }
                    None => 0,
                };
                self.doc.splice_text(&obj, pos, 0, &text).map_err(lib_err)?;
            }
        }
        Ok(None)
    }
}

impl Replica for AutomergeReplica {
    const NAME: &'static str = "Automerge";

    fn new_document(peer: u64) -> Self {
        let mut doc = AutoCommit::new().with_actor(actor(peer));
        doc.put(ROOT, "doc_id", crate::new_doc_id(peer))
            .expect("put doc id");
        doc.put_object(ROOT, "nodes", ObjType::Map).expect("nodes");
        doc.put_object(ROOT, "edges", ObjType::Map).expect("edges");
        doc.commit();
        Self::wrap(doc).expect("fresh layout")
    }

    fn fork_as(&mut self, peer: u64) -> Self {
        let bytes = self.save();
        Self::load(&bytes, peer).expect("own save loads")
    }

    fn doc_id(&self) -> String {
        get_str(&self.doc, &ROOT, "doc_id")
            .map(|x| x.0)
            .unwrap_or_default()
    }

    fn create_node(&mut self, parent: Option<&str>, kind: &str) -> Result<NodeId, OpError> {
        if let Some(p) = parent {
            self.info(p)?;
        }
        let parent = parent.map(str::to_owned);
        let id = self.new_id("n");
        let obj = self
            .doc
            .put_object(&self.nodes_obj, id.as_str(), ObjType::Map)
            .map_err(lib_err)?;
        self.doc.put(&obj, "kind", kind).map_err(lib_err)?;
        let text = if kind == "text" {
            Some(
                self.doc
                    .put_object(&obj, "text", ObjType::Text)
                    .map_err(lib_err)?,
            )
        } else {
            None
        };
        let pos = self.position_key(&parent, usize::MAX, None);
        self.index.obj_to_node.insert(obj.to_string(), id.clone());
        if let Some(t) = &text {
            self.index.obj_to_node.insert(t.to_string(), id.clone());
        }
        let placement = Placement {
            parent: parent.clone(),
            pos,
            prev: parent,
        };
        self.index.nodes.insert(
            id.clone(),
            NodeInfo {
                obj,
                text,
                kind: kind.to_owned(),
                removed: false,
                stored: placement.clone(),
                mv_op: ObjId::Root,
            },
        );
        self.index.parent.insert(id.clone(), None);
        self.index
            .children
            .entry(None)
            .or_default()
            .push((String::new(), id.clone()));
        self.set_placement(&id, placement)?;
        self.pending.push(Inverse::Created { node: id.clone() });
        Ok(id)
    }

    fn remove_node(&mut self, id: &str) -> Result<(), OpError> {
        self.info(id)?;
        self.put_removed(id, true)?;
        self.pending.push(Inverse::Removed {
            node: id.to_owned(),
        });
        Ok(())
    }

    fn move_node(&mut self, id: &str, parent: Option<&str>, index: usize) -> Result<(), OpError> {
        let old = self.info(id)?.stored.clone();
        if let Some(p) = parent {
            self.info(p)?;
            if self.is_ancestor(id, p) {
                return Err(OpError::WouldCycle);
            }
        }
        let parent = parent.map(str::to_owned);
        let current = self.index.parent.get(id).cloned().flatten();
        if current == parent {
            let sibs = self.visible_children(&parent);
            if sibs.iter().position(|(_, c)| c == id) == Some(index.min(sibs.len() - 1)) {
                return Ok(()); // changes nothing, like Loro
            }
        }
        let pos = self.position_key(&parent, index, Some(id));
        let wrote = self.set_placement(
            id,
            Placement {
                parent,
                pos,
                prev: current,
            },
        )?;
        self.pending.push(Inverse::Moved {
            node: id.to_owned(),
            wrote,
            old: old.encode(),
        });
        Ok(())
    }

    fn set_param(&mut self, id: &str, key: &str, value: Value) -> Result<(), OpError> {
        let obj = self.info(id)?.obj.clone();
        let k = format!("p:{key}");
        let old = match self.doc.get(&obj, k.as_str()).map_err(lib_err)? {
            Some((v, _)) => from_am(&v),
            None => None,
        };
        self.doc
            .put(&obj, k.as_str(), to_scalar(value.clone()))
            .map_err(lib_err)?;
        self.pending.push(Inverse::Param {
            node: id.to_owned(),
            key: key.to_owned(),
            wrote: Some(value),
            old,
        });
        Ok(())
    }

    fn connect(&mut self, edge: &Edge) -> Result<String, OpError> {
        self.info(&edge.from)?;
        self.info(&edge.to)?;
        let id = self.new_id("e");
        self.doc
            .put(&self.edges_obj, id.as_str(), edge.encode())
            .map_err(lib_err)?;
        self.pending.push(Inverse::Connected { edge: id.clone() });
        Ok(id)
    }

    fn disconnect(&mut self, edge_id: &str) -> Result<(), OpError> {
        let Some((encoded, _)) = get_str(&self.doc, &self.edges_obj, edge_id) else {
            return Err(OpError::NotFound(edge_id.to_owned()));
        };
        self.doc.delete(&self.edges_obj, edge_id).map_err(lib_err)?;
        self.pending.push(Inverse::Disconnected {
            edge: edge_id.to_owned(),
            encoded,
        });
        Ok(())
    }

    fn insert_text(&mut self, id: &str, pos: usize, text: &str) -> Result<(), OpError> {
        let obj = self.text_obj(id)?;
        self.doc.splice_text(&obj, pos, 0, text).map_err(lib_err)?;
        if !text.is_empty() {
            let start = self.doc.get_cursor(&obj, pos, None).map_err(lib_err)?;
            self.pending.push(Inverse::TextInserted {
                node: id.to_owned(),
                start,
                text: text.to_owned(),
            });
        }
        Ok(())
    }

    fn delete_text(&mut self, id: &str, pos: usize, len: usize) -> Result<(), OpError> {
        let obj = self.text_obj(id)?;
        let removed: String = self
            .doc
            .text(&obj)
            .map_err(lib_err)?
            .chars()
            .skip(pos)
            .take(len)
            .collect();
        let before = match pos.checked_sub(1) {
            Some(p) => Some(self.doc.get_cursor(&obj, p, None).map_err(lib_err)?),
            None => None,
        };
        self.doc
            .splice_text(&obj, pos, len as isize, "")
            .map_err(lib_err)?;
        self.pending.push(Inverse::TextDeleted {
            node: id.to_owned(),
            before,
            text: removed,
        });
        Ok(())
    }

    fn commit(&mut self) {
        self.doc.commit();
        if !self.pending.is_empty() {
            self.undo_stack.push_back(std::mem::take(&mut self.pending));
            // Same cap as the Loro proof (UndoManager max steps).
            if self.undo_stack.len() > MAX_UNDO_STEPS {
                self.undo_stack.pop_front();
            }
        }
    }

    fn children(&self, parent: Option<&str>) -> Vec<NodeId> {
        if let Some(p) = parent
            && !self.is_visible(p)
        {
            return Vec::new();
        }
        self.visible_children(&parent.map(str::to_owned))
            .into_iter()
            .map(|(_, c)| c)
            .collect()
    }

    fn parent(&self, id: &str) -> Option<Option<NodeId>> {
        if !self.is_visible(id) {
            return None;
        }
        self.index.parent.get(id).cloned()
    }

    fn stored_parent(&self, id: &str) -> Option<Option<NodeId>> {
        self.index.parent.get(id).cloned()
    }

    fn is_removed(&self, id: &str) -> bool {
        self.index.nodes.get(id).is_some_and(|i| i.removed)
    }

    fn param(&self, id: &str, key: &str) -> Option<Value> {
        let obj = &self.info(id).ok()?.obj;
        let (v, _) = self.doc.get(obj, format!("p:{key}").as_str()).ok()??;
        from_am(&v)
    }

    fn text(&self, id: &str) -> Option<String> {
        let obj = self.text_obj(id).ok()?;
        self.doc.text(&obj).ok()
    }

    fn edges(&self) -> BTreeMap<String, Edge> {
        let keys: Vec<String> = self
            .doc
            .map_range(&self.edges_obj, ..)
            .map(|item| item.key.to_string())
            .collect();
        keys.into_iter()
            .filter_map(|k| {
                let (s, _) = get_str(&self.doc, &self.edges_obj, &k)?;
                Some((k, Edge::decode(&s)?))
            })
            .collect()
    }

    fn state(&self) -> State {
        let mut st = State {
            doc_id: self.doc_id(),
            roots: self.children(None),
            edges: self.edges(),
            ..State::default()
        };
        let mut stack: Vec<NodeId> = st.roots.clone();
        while let Some(id) = stack.pop() {
            let info = &self.index.nodes[&id];
            let mut ns = NodeState {
                kind: info.kind.clone(),
                parent: self.index.parent[&id].clone(),
                children: self.children(Some(&id)),
                text: self.text(&id),
                ..NodeState::default()
            };
            for item in self.doc.map_range(&info.obj, ..) {
                if let Some(k) = item.key.strip_prefix("p:")
                    && let Some(v) = self.param(&id, k)
                {
                    ns.params.insert(k.to_owned(), v);
                }
            }
            stack.extend(ns.children.iter().cloned());
            st.nodes.insert(id, ns);
        }
        st
    }

    fn merge_from(&mut self, other: &mut Self) -> Result<MergeReport, OpError> {
        if self.doc_id() != other.doc_id() {
            return Err(OpError::DifferentDocument);
        }
        self.commit();
        other.commit();
        let remote_changes = self.doc.get_changes_added(&mut other.doc);
        // Local changes concurrent with the incoming ones: those the other
        // copy had not seen when it made them. Starting points are the
        // incoming changes whose dependencies are all already here.
        let incoming: HashSet<automerge::ChangeHash> =
            remote_changes.iter().map(|c| c.hash()).collect();
        let mut seen = HashSet::new();
        let mut local_changes = Vec::new();
        for c in &remote_changes {
            if c.deps().iter().any(|d| incoming.contains(d)) {
                continue;
            }
            for l in self.doc.get_changes(c.deps()) {
                if seen.insert(l.hash()) {
                    local_changes.push(l);
                }
            }
        }
        self.doc.merge(&mut other.doc).map_err(lib_err)?;
        let broken_before: HashSet<NodeId> = self.index.broken.keys().cloned().collect();
        self.rebuild_index();
        let local = self.touches(&local_changes);
        let remote = self.touches(&remote_changes);

        let mut report = MergeReport::default();
        let touched_params: BTreeSet<(NodeId, String)> = local
            .iter()
            .chain(&remote)
            .filter_map(|t| match t {
                Touch::Param { node, key, .. } => Some((node.clone(), key.clone())),
                _ => None,
            })
            .collect();
        for (node, key) in touched_params {
            let Some(info) = self.index.nodes.get(&node) else {
                continue;
            };
            let k = format!("p:{key}");
            let all = self.doc.get_all(&info.obj, k.as_str()).map_err(lib_err)?;
            if all.len() < 2 {
                continue;
            }
            let kept = self
                .doc
                .get(&info.obj, k.as_str())
                .map_err(lib_err)?
                .and_then(|(v, _)| from_am(&v));
            let lost: Vec<Value> = all
                .iter()
                .filter_map(|(v, _)| from_am(v))
                .filter(|v| Some(v) != kept.as_ref())
                .collect();
            if !lost.is_empty() {
                report.concurrent_writes.push(ConcurrentWrite {
                    node,
                    key,
                    kept,
                    lost,
                });
            }
        }
        let moved: BTreeSet<NodeId> = local
            .iter()
            .chain(&remote)
            .filter_map(|t| match t {
                Touch::Move { node, .. } => Some(node.clone()),
                _ => None,
            })
            .collect();
        for node in moved {
            let Some(info) = self.index.nodes.get(&node) else {
                continue;
            };
            let final_parent = self.index.parent.get(&node).cloned().flatten();
            let all = self.doc.get_all(&info.obj, "mv").map_err(lib_err)?;
            let winner = info.stored.encode();
            for (v, _) in &all {
                let AmValue::Scalar(s) = v else { continue };
                let ScalarValue::Str(s) = s.as_ref() else {
                    continue;
                };
                let Some(p) = Placement::decode(s) else {
                    continue;
                };
                if p.encode() != winner {
                    report.lost_moves.push(LostMove {
                        node: node.clone(),
                        wanted_parent: p.parent,
                        final_parent: final_parent.clone(),
                    });
                }
            }
        }
        // A cycle broken by this merge, whichever copy made the move.
        let mut newly_broken: Vec<_> = self
            .index
            .broken
            .iter()
            .filter(|(n, _)| !broken_before.contains(*n))
            .collect();
        newly_broken.sort();
        for (node, wanted) in newly_broken {
            report.lost_moves.push(LostMove {
                node: node.clone(),
                wanted_parent: wanted.clone(),
                final_parent: self.index.parent.get(node).cloned().flatten(),
            });
        }
        report.hidden_by_removal = hidden_by_removal(self, &local, &remote);
        Ok(report)
    }

    fn undo(&mut self) -> UndoOutcome {
        self.commit();
        let Some(step) = self.undo_stack.pop_back() else {
            return UndoOutcome::default();
        };
        let mut out = UndoOutcome::default();
        for inv in step.into_iter().rev() {
            match self.apply_inverse(inv) {
                Ok(None) => out.undone = true,
                Ok(Some(why)) => out.skipped.push(why),
                Err(e) => out.skipped.push(format!("{e:?}")),
            }
        }
        // The undo is itself a new change; it must not land on the stack.
        self.doc.commit();
        self.pending.clear();
        self.rebuild_index();
        out
    }

    fn save(&mut self) -> Vec<u8> {
        self.commit();
        self.doc.save()
    }

    fn load(bytes: &[u8], peer: u64) -> Result<Self, OpError> {
        let doc = AutoCommit::load(bytes)
            .map_err(lib_err)?
            .with_actor(actor(peer));
        Self::wrap(doc)
    }

    fn history_dump(&mut self) -> String {
        self.commit();
        // Automerge stores text one character per operation; consecutive
        // inserted characters are joined back into runs, so a search finds
        // text the way it was typed.
        let mut out = String::new();
        for c in self.doc.get_changes(&[]) {
            let mut run = String::new();
            for op in c.decode().operations {
                match (&op.action, op.insert) {
                    (LegacyOp::Put(ScalarValue::Str(s)), true) => run.push_str(s),
                    _ => {
                        if !run.is_empty() {
                            out.push_str(&format!("text run {run:?}\n"));
                            run.clear();
                        }
                        out.push_str(&format!("{op:?}\n"));
                    }
                }
            }
            if !run.is_empty() {
                out.push_str(&format!("text run {run:?}\n"));
            }
        }
        out
    }

    fn history_ops(&mut self) -> usize {
        self.commit();
        self.doc.stats().num_ops as usize
    }

    fn history_peers(&mut self) -> BTreeSet<String> {
        self.commit();
        self.doc
            .get_changes(&[])
            .iter()
            .map(|c| c.actor_id().to_hex_string())
            .collect()
    }
}

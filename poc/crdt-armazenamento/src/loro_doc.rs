//! The socket implemented with Loro.
//!
//! Hierarchy and stacks: `LoroTree` (native move, fractional index).
//! Parameters: the tree node's own metadata map, keys `p:<name>`.
//! Text: a `LoroText` inside the metadata map, created with the node.
//! Connections: one root map, one string register per edge.
//!
//! Missing in Loro and built here: detecting concurrent writes to the same
//! parameter (and which move lost). Both come from comparing the two
//! histories since the copies last met (`touches`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use loro::{
    Container, ContainerID, ContainerType, ExportMode, Frontiers, ID, Index, JsonOpContent,
    JsonSchema, LoroDoc, LoroError, LoroMap, LoroText, LoroTree, LoroTreeError, LoroValue, TreeID,
    TreeParentId, UndoManager, ValueOrContainer, VersionVector,
};

use crate::model::{
    Edge, MergeReport, NodeId, NodeState, OpError, Replica, State, Touch, UndoOutcome, Value,
    concurrent_writes_from_touches, hidden_by_removal, lost_moves_from_touches,
};

pub struct LoroReplica {
    doc: LoroDoc,
    undo: UndoManager,
}

fn lib_err(e: LoroError) -> OpError {
    match e {
        LoroError::TreeError(LoroTreeError::CyclicMoveError) => OpError::WouldCycle,
        other => OpError::Library(other.to_string()),
    }
}

fn tree_id(id: &str) -> Result<TreeID, OpError> {
    TreeID::try_from(id).map_err(|_| OpError::NotFound(id.to_owned()))
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

/// What two writes compete for: one parameter, or one node's place.
fn target(t: &Touch) -> Option<String> {
    match t {
        Touch::Param { node, key, .. } => Some(format!("p\u{1f}{node}\u{1f}{key}")),
        Touch::Move { node, .. } | Touch::Remove { node } => Some(format!("m\u{1f}{node}")),
        _ => None,
    }
}

fn parent_arg(parent: Option<&str>) -> Result<TreeParentId, OpError> {
    Ok(match parent {
        None => TreeParentId::Root,
        Some(p) => TreeParentId::Node(tree_id(p)?),
    })
}

impl LoroReplica {
    fn wrap(doc: LoroDoc) -> Self {
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(0);
        undo.set_max_undo_steps(10_000);
        LoroReplica { doc, undo }
    }

    fn tree(&self) -> LoroTree {
        self.doc.get_tree("tree")
    }

    fn meta(&self, id: &str) -> Result<LoroMap, OpError> {
        let t = tree_id(id)?;
        if !self.is_visible(t) {
            return Err(OpError::NotFound(id.to_owned()));
        }
        self.tree().get_meta(t).map_err(lib_err)
    }

    fn is_visible(&self, t: TreeID) -> bool {
        let tree = self.tree();
        tree.contains(t) && !tree.is_node_deleted(&t).unwrap_or(true)
    }

    fn text_of(&self, id: &str) -> Result<LoroText, OpError> {
        match self.meta(id)?.get("text") {
            Some(ValueOrContainer::Container(Container::Text(t))) => Ok(t),
            _ => Err(OpError::NotFound(format!("{id}: not a text node"))),
        }
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
                let order = (change.lamport as u64 + tick, change.id.peer);
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
                        let node = TreeID::new(*peer, *counter).to_string();
                        let (key, value) = match m {
                            loro::JsonMapOp::Insert { key, value } => (key, from_loro(value)),
                            loro::JsonMapOp::Delete { key } => (key, None),
                        };
                        match key.strip_prefix("p:") {
                            Some(k) => Touch::Param {
                                node,
                                key: k.to_owned(),
                                value,
                            },
                            None => continue,
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
                            node: target.to_string(),
                        },
                        loro::JsonTreeOp::Move {
                            target,
                            parent,
                            fractional_index,
                        } => Touch::Move {
                            node: target.to_string(),
                            parent: parent.map(|p| p.to_string()),
                            pos: fractional_index.to_string(),
                        },
                        loro::JsonTreeOp::Delete { target } => Touch::Remove {
                            node: target.to_string(),
                        },
                    },
                    _ => continue,
                };
                keyed.push((order, (touch, id, op_past)));
            }
        }
        keyed.sort_by_key(|k| k.0);
        keyed.into_iter().map(|(_, t)| t).collect()
    }

    fn node_of_container(&self, cid: &ContainerID) -> Option<NodeId> {
        let path = self.doc.get_path_to_container(cid)?;
        path.iter().rev().find_map(|(_, idx)| match idx {
            Index::Node(t) => Some(t.to_string()),
            _ => None,
        })
    }

    fn json_between(&self, from: &VersionVector, to: &VersionVector) -> JsonSchema {
        self.doc
            .export_json_updates_without_peer_compression(from, to)
            .expect("own history is readable")
    }

    /// Native alternative to `clean_copy`, kept to compare: a snapshot with
    /// only the current state and "a few ops" of history.
    pub fn export_state_only(&self) -> Vec<u8> {
        self.doc
            .export(ExportMode::StateOnly(None))
            .expect("export state")
    }
}

impl Replica for LoroReplica {
    const NAME: &'static str = "Loro";

    fn new_document(peer: u64) -> Self {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).expect("valid peer id");
        doc.get_tree("tree").enable_fractional_index(0);
        doc.get_map("meta")
            .insert("doc_id", crate::new_doc_id(peer))
            .expect("insert doc id");
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

    fn create_node(&mut self, parent: Option<&str>, kind: &str) -> Result<NodeId, OpError> {
        if let Some(p) = parent
            && !self.is_visible(tree_id(p)?)
        {
            return Err(OpError::NotFound(p.to_owned()));
        }
        let tree = self.tree();
        let id = tree.create(parent_arg(parent)?).map_err(lib_err)?;
        let meta = tree.get_meta(id).map_err(lib_err)?;
        meta.insert("kind", kind).map_err(lib_err)?;
        if kind == "text" {
            meta.insert_container("text", LoroText::new())
                .map_err(lib_err)?;
        }
        Ok(id.to_string())
    }

    fn remove_node(&mut self, id: &str) -> Result<(), OpError> {
        self.meta(id)?;
        self.tree().delete(tree_id(id)?).map_err(lib_err)
    }

    fn move_node(&mut self, id: &str, parent: Option<&str>, index: usize) -> Result<(), OpError> {
        self.meta(id)?;
        if let Some(p) = parent {
            self.meta(p)?;
        }
        // Same rule as the socket: the index counts siblings without the
        // moved node, clamped to the end.
        let others = self
            .children(parent)
            .into_iter()
            .filter(|c| c != id)
            .count();
        self.tree()
            .mov_to(tree_id(id)?, parent_arg(parent)?, index.min(others))
            .map_err(lib_err)
    }

    fn set_param(&mut self, id: &str, key: &str, value: Value) -> Result<(), OpError> {
        self.meta(id)?
            .insert(&format!("p:{key}"), to_loro(value))
            .map_err(lib_err)
    }

    fn connect(&mut self, edge: &Edge) -> Result<String, OpError> {
        self.meta(&edge.from)?;
        self.meta(&edge.to)?;
        let id = format!("e{}.{:x}", self.doc.len_ops(), self.doc.peer_id());
        self.doc
            .get_map("edges")
            .insert(&id, edge.encode())
            .map_err(lib_err)?;
        Ok(id)
    }

    fn disconnect(&mut self, edge_id: &str) -> Result<(), OpError> {
        let edges = self.doc.get_map("edges");
        if edges.get(edge_id).is_none() {
            return Err(OpError::NotFound(edge_id.to_owned()));
        }
        edges.delete(edge_id).map_err(lib_err)
    }

    fn insert_text(&mut self, id: &str, pos: usize, text: &str) -> Result<(), OpError> {
        self.text_of(id)?.insert(pos, text).map_err(lib_err)
    }

    fn delete_text(&mut self, id: &str, pos: usize, len: usize) -> Result<(), OpError> {
        self.text_of(id)?.delete(pos, len).map_err(lib_err)
    }

    fn commit(&mut self) {
        self.doc.commit();
    }

    fn children(&self, parent: Option<&str>) -> Vec<NodeId> {
        let Ok(p) = parent_arg(parent) else {
            return Vec::new();
        };
        if let TreeParentId::Node(t) = p
            && !self.is_visible(t)
        {
            return Vec::new();
        }
        self.tree()
            .children(p)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.to_string())
            .collect()
    }

    fn parent(&self, id: &str) -> Option<Option<NodeId>> {
        let t = tree_id(id).ok()?;
        if !self.is_visible(t) {
            return None;
        }
        self.stored_parent(id)
    }

    fn stored_parent(&self, id: &str) -> Option<Option<NodeId>> {
        let t = tree_id(id).ok()?;
        match self.tree().parent(t)? {
            TreeParentId::Node(p) => Some(Some(p.to_string())),
            TreeParentId::Root | TreeParentId::Deleted => Some(None),
            TreeParentId::Unexist => None,
        }
    }

    fn is_removed(&self, id: &str) -> bool {
        tree_id(id)
            .ok()
            .and_then(|t| self.tree().parent(t))
            .is_some_and(|p| p == TreeParentId::Deleted)
    }

    fn param(&self, id: &str, key: &str) -> Option<Value> {
        match self.meta(id).ok()?.get(&format!("p:{key}"))? {
            ValueOrContainer::Value(v) => from_loro(&v),
            ValueOrContainer::Container(_) => None,
        }
    }

    fn text(&self, id: &str) -> Option<String> {
        self.text_of(id).ok().map(|t| t.to_string())
    }

    fn edges(&self) -> BTreeMap<String, Edge> {
        let mut out = BTreeMap::new();
        let edges = self.doc.get_map("edges");
        for key in edges.keys() {
            if let Some(ValueOrContainer::Value(LoroValue::String(s))) = edges.get(&key)
                && let Some(e) = Edge::decode(&s)
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
        let mut stack: Vec<(Option<NodeId>, NodeId)> =
            st.roots.iter().map(|r| (None, r.clone())).collect();
        while let Some((parent, id)) = stack.pop() {
            let meta = self.meta(&id).expect("visible node");
            let mut ns = NodeState {
                parent,
                children: self.children(Some(&id)),
                text: self.text(&id),
                ..NodeState::default()
            };
            for key in meta.keys() {
                if key.as_str() == "kind" {
                    if let Some(ValueOrContainer::Value(LoroValue::String(s))) = meta.get("kind") {
                        ns.kind = s.to_string();
                    }
                } else if let Some(k) = key.strip_prefix("p:")
                    && let Some(v) = self.param(&id, k)
                {
                    ns.params.insert(k.to_owned(), v);
                }
            }
            for c in &ns.children {
                stack.push((Some(id.clone()), c.clone()));
            }
            st.nodes.insert(id, ns);
        }
        st
    }

    fn merge_from(&mut self, other: &mut Self) -> Result<MergeReport, OpError> {
        if self.doc_id() != other.doc_id() {
            return Err(OpError::DifferentDocument);
        }
        self.doc.commit();
        other.doc.commit();
        let mine = self.doc.oplog_vv();
        let theirs = other.doc.oplog_vv();
        let updates = other
            .doc
            .export(ExportMode::updates(&mine))
            .map_err(|e| OpError::Library(e.to_string()))?;
        let remote_json = other.json_between(&mine, &theirs);
        // Local operations concurrent with the incoming ones: those the
        // other copy had not seen when it made them. The starting points
        // are the incoming changes whose dependencies are all here already.
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
        let local_json = self.json_between(&base, &mine);
        self.doc.import(&updates).map_err(lib_err)?;
        // Text containers are resolved after the import, when they exist here.
        let remote = self.touches(&remote_json, Some(&self.doc));
        let mut local = self.touches(&local_json, None);
        // Finer filter: a local write counts only if the last incoming write
        // to the same target had not seen it.
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
            concurrent_writes: concurrent_writes_from_touches(self, &local, &remote),
            lost_moves: lost_moves_from_touches(self, &local, &remote, |n| {
                tree.fractional_index(tree_id(n).ok()?)
            }),
            hidden_by_removal: hidden_by_removal(self, &local, &remote),
        })
    }

    fn undo(&mut self) -> UndoOutcome {
        self.doc.commit();
        match self.undo.undo() {
            Ok(undone) => UndoOutcome {
                undone,
                skipped: Vec::new(),
            },
            Err(e) => UndoOutcome {
                undone: false,
                skipped: vec![e.to_string()],
            },
        }
    }

    fn save(&mut self) -> Vec<u8> {
        self.doc.commit();
        self.doc.export(ExportMode::Snapshot).expect("snapshot")
    }

    fn load(bytes: &[u8], peer: u64) -> Result<Self, OpError> {
        let doc = LoroDoc::from_snapshot(bytes).map_err(lib_err)?;
        doc.set_peer_id(peer).map_err(lib_err)?;
        Ok(Self::wrap(doc))
    }

    fn history_dump(&mut self) -> String {
        self.doc.commit();
        let all = self.json_between(&VersionVector::default(), &self.doc.oplog_vv());
        format!("{all:?}")
    }

    fn history_ops(&mut self) -> usize {
        self.doc.commit();
        self.doc.len_ops()
    }

    fn history_peers(&mut self) -> BTreeSet<String> {
        self.doc.commit();
        let all = self.json_between(&VersionVector::default(), &self.doc.oplog_vv());
        all.changes
            .iter()
            .map(|c| format!("{:x}", c.id.peer))
            .collect()
    }
}

/// Fractional position keys of a node's children (for tests and reports).
pub fn positions(r: &LoroReplica, parent: Option<&str>) -> HashMap<NodeId, String> {
    let tree = r.tree();
    r.children(parent)
        .into_iter()
        .filter_map(|c| {
            let pos = tree.fractional_index(tree_id(&c).ok()?)?;
            Some((c, pos))
        })
        .collect()
}

//! Deterministic workload with the mix the owner approved (2026-10-29):
//! set parameter 40%, edit text 10%, create node 15%, remove node 5%,
//! connect 10%, disconnect 5%, reorder in a stack 10%, move in the
//! hierarchy 5%. Every operation is one user action (one commit).

use std::collections::{HashMap, HashSet};

use crate::model::{Edge, NodeId, Replica, Value};

/// Small deterministic generator (xorshift64*), so both libraries receive
/// exactly the same sequence of choices.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OpKind {
    SetParam,
    EditText,
    Create,
    Remove,
    Connect,
    Disconnect,
    Reorder,
    Move,
}

/// Approved proportions, in percent.
pub const MIX: [(OpKind, usize); 8] = [
    (OpKind::SetParam, 40),
    (OpKind::EditText, 10),
    (OpKind::Create, 15),
    (OpKind::Remove, 5),
    (OpKind::Connect, 10),
    (OpKind::Disconnect, 5),
    (OpKind::Reorder, 10),
    (OpKind::Move, 5),
];

const KEYS: [&str; 6] = ["x", "y", "width", "color", "opacity", "name"];

/// The generator's own view of the document, so choosing a target never
/// reads the library (reads would be measured as part of the operation).
#[derive(Clone, Default)]
pub struct Mirror {
    nodes: Vec<NodeId>,
    slot: HashMap<NodeId, usize>,
    parent: HashMap<NodeId, Option<NodeId>>,
    children: HashMap<Option<NodeId>, Vec<NodeId>>,
    texts: Vec<NodeId>,
    text_slot: HashMap<NodeId, usize>,
    text_len: HashMap<NodeId, usize>,
    edges: Vec<String>,
}

impl Mirror {
    fn add(&mut self, id: NodeId, parent: Option<NodeId>, text: bool) {
        self.slot.insert(id.clone(), self.nodes.len());
        self.nodes.push(id.clone());
        self.children
            .entry(parent.clone())
            .or_default()
            .push(id.clone());
        self.parent.insert(id.clone(), parent);
        if text {
            self.text_slot.insert(id.clone(), self.texts.len());
            self.texts.push(id.clone());
            self.text_len.insert(id, 0);
        }
    }

    fn drop_node(&mut self, id: &str) {
        let i = self.slot.remove(id).expect("known node");
        let last = self.nodes.pop().expect("non-empty");
        if last != id {
            self.nodes[i] = last.clone();
            self.slot.insert(last, i);
        }
        let p = self.parent.remove(id).expect("known");
        self.children
            .get_mut(&p)
            .expect("listed")
            .retain(|c| c != id);
        if let Some(i) = self.text_slot.remove(id) {
            let last = self.texts.pop().expect("non-empty");
            if last != id {
                self.texts[i] = last.clone();
                self.text_slot.insert(last, i);
            }
        }
        self.text_len.remove(id);
    }

    fn is_in_subtree(&self, node: &str, root: &str) -> bool {
        let mut cur = Some(node.to_owned());
        while let Some(n) = cur {
            if n == root {
                return true;
            }
            cur = self.parent.get(&n).cloned().flatten();
        }
        false
    }

    fn reparent(&mut self, id: &str, parent: Option<NodeId>, index: usize) {
        let old = self
            .parent
            .insert(id.to_owned(), parent.clone())
            .expect("known");
        self.children
            .get_mut(&old)
            .expect("listed")
            .retain(|c| c != id);
        let list = self.children.entry(parent).or_default();
        list.insert(index.min(list.len()), id.to_owned());
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// What this copy meant to do, recorded by the generator itself (not read
/// back from the library), so tests can check a merge against it.
#[derive(Clone, Default)]
pub struct Intents {
    pub last_param: HashMap<(NodeId, String), Value>,
    pub last_place: HashMap<NodeId, Option<NodeId>>,
    pub removed: HashSet<NodeId>,
    pub created: HashSet<NodeId>,
}

pub struct Workload {
    pub rng: Rng,
    pub mirror: Mirror,
    pub counts: HashMap<OpKind, usize>,
    pub intents: Intents,
}

impl Workload {
    pub fn new(seed: u64) -> Self {
        Workload {
            rng: Rng::new(seed),
            mirror: Mirror::default(),
            counts: HashMap::new(),
            intents: Intents::default(),
        }
    }

    /// Continues from a mirror (a copy forked from the same base).
    pub fn with_mirror(seed: u64, mirror: Mirror) -> Self {
        Workload {
            rng: Rng::new(seed),
            mirror,
            counts: HashMap::new(),
            intents: Intents::default(),
        }
    }

    fn pick_kind(&mut self) -> OpKind {
        let mut roll = self.rng.below(100);
        for (kind, pct) in MIX {
            if roll < pct {
                return kind;
            }
            roll -= pct;
        }
        unreachable!("MIX sums to 100")
    }

    /// Runs `n` operations, one commit each.
    pub fn run<R: Replica>(&mut self, r: &mut R, n: usize) {
        for _ in 0..n {
            let kind = self.pick_kind();
            let done = self.apply(r, kind);
            *self.counts.entry(done).or_default() += 1;
            r.commit();
        }
    }

    /// Applies one operation; falls back to `Create` when the chosen kind
    /// has no valid target yet. Returns what was actually done.
    fn apply<R: Replica>(&mut self, r: &mut R, kind: OpKind) -> OpKind {
        let m = &self.mirror;
        let rng = &mut self.rng;
        match kind {
            OpKind::SetParam if !m.is_empty() => {
                let node = m.nodes[rng.below(m.len())].clone();
                let key = KEYS[rng.below(KEYS.len())];
                // Never the value already there: a write that changes nothing
                // is not recorded by either library, so it is not an edit.
                let tag = rng.next_u64() >> 16;
                let value = if key == "name" || key == "color" {
                    Value::Str(format!("v{tag:x}"))
                } else {
                    Value::Num(tag as f64 / 16.0)
                };
                r.set_param(&node, key, value.clone()).expect("set param");
                self.intents
                    .last_param
                    .insert((node, key.to_owned()), value);
                kind
            }
            OpKind::EditText if !m.texts.is_empty() => {
                let node = m.texts[rng.below(m.texts.len())].clone();
                let len = m.text_len[&node];
                if len > 0 && rng.below(10) < 3 {
                    let pos = rng.below(len);
                    let del = (1 + rng.below(5)).min(len - pos);
                    r.delete_text(&node, pos, del).expect("delete text");
                    *self.mirror.text_len.get_mut(&node).expect("text") -= del;
                } else {
                    let pos = rng.below(len + 1);
                    let n = 1 + rng.below(8);
                    let s: String = (0..n)
                        .map(|_| (b'a' + rng.below(26) as u8) as char)
                        .collect();
                    r.insert_text(&node, pos, &s).expect("insert text");
                    *self.mirror.text_len.get_mut(&node).expect("text") += n;
                }
                kind
            }
            OpKind::Remove if !m.is_empty() => {
                // Only leaves, so the mirror stays simple; a few random
                // tries keep this cheap on large documents.
                let leaf = (0..8)
                    .map(|_| m.nodes[rng.below(m.len())].clone())
                    .find(|n| m.children.get(&Some(n.clone())).is_none_or(Vec::is_empty));
                let Some(node) = leaf else {
                    return self.apply(r, OpKind::Create);
                };
                r.remove_node(&node).expect("remove");
                self.mirror.drop_node(&node);
                self.intents.removed.insert(node);
                kind
            }
            OpKind::Connect if m.len() >= 2 => {
                let a = m.nodes[rng.below(m.len())].clone();
                let b = m.nodes[rng.below(m.len())].clone();
                if a == b {
                    return self.apply(r, OpKind::Create);
                }
                let e = Edge {
                    from: a,
                    from_port: "out".into(),
                    to: b,
                    to_port: "in".into(),
                };
                let id = r.connect(&e).expect("connect");
                self.mirror.edges.push(id);
                kind
            }
            OpKind::Disconnect if !m.edges.is_empty() => {
                let i = rng.below(m.edges.len());
                let id = self.mirror.edges.swap_remove(i);
                r.disconnect(&id).expect("disconnect");
                kind
            }
            OpKind::Reorder if !m.is_empty() => {
                let stack = (0..8)
                    .map(|_| m.parent[&m.nodes[rng.below(m.len())]].clone())
                    .find(|p| m.children.get(p).is_some_and(|v| v.len() >= 2));
                let Some(parent) = stack else {
                    return self.apply(r, OpKind::Create);
                };
                let list = &m.children[&parent];
                let from = rng.below(list.len());
                let node = list[from].clone();
                // A different place among the other items: a move that
                // changes nothing is not an edit (Loro does not record it).
                let to = (from + 1 + rng.below(list.len() - 1)) % list.len();
                r.move_node(&node, parent.as_deref(), to).expect("reorder");
                self.mirror.reparent(&node, parent.clone(), to);
                self.intents.last_place.insert(node, parent);
                kind
            }
            OpKind::Move if m.len() >= 2 => {
                let node = m.nodes[rng.below(m.len())].clone();
                let parent = if rng.below(20) == 0 {
                    None
                } else {
                    let p = m.nodes[rng.below(m.len())].clone();
                    if m.is_in_subtree(&p, &node) {
                        return self.apply(r, OpKind::SetParam);
                    }
                    Some(p)
                };
                let siblings = m.children.get(&parent).map_or(&[][..], Vec::as_slice);
                let others = siblings.iter().filter(|c| **c != node).count();
                let to = rng.below(others + 1);
                if siblings.iter().position(|c| *c == node) == Some(to) {
                    return self.apply(r, OpKind::SetParam); // would change nothing
                }
                r.move_node(&node, parent.as_deref(), to).expect("move");
                self.mirror.reparent(&node, parent.clone(), to);
                self.intents.last_place.insert(node, parent);
                kind
            }
            _ => {
                let parent = if m.is_empty() || rng.below(100) < 3 {
                    None
                } else {
                    Some(m.nodes[rng.below(m.len())].clone())
                };
                let text = rng.below(5) == 0;
                let id = r
                    .create_node(parent.as_deref(), if text { "text" } else { "shape" })
                    .expect("create");
                self.intents.created.insert(id.clone());
                self.mirror.add(id, parent, text);
                OpKind::Create
            }
        }
    }
}

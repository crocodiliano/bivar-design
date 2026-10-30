//! Test kit shared by the store's tests: the workload from phase 0.6 part 2
//! (mix approved by the owner on 2026-10-29) and the "nothing is lost
//! silently" checks (achado A12; nucleo.md N.1).

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::op::{Edge, NodeId, Op, Value};
use crate::store::{MergeReport, State, Store};

/// Small deterministic generator (xorshift64*).
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
/// reads the store.
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

    fn drop_node(&mut self, id: &NodeId) {
        let i = self.slot.remove(id).expect("known node");
        let last = self.nodes.pop().expect("non-empty");
        if last != *id {
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
            if last != *id {
                self.texts[i] = last.clone();
                self.text_slot.insert(last, i);
            }
        }
        self.text_len.remove(id);
    }

    fn is_in_subtree(&self, node: &NodeId, root: &NodeId) -> bool {
        let mut cur = Some(node.clone());
        while let Some(n) = cur {
            if n == *root {
                return true;
            }
            cur = self.parent.get(&n).cloned().flatten();
        }
        false
    }

    fn reparent(&mut self, id: &NodeId, parent: Option<NodeId>, index: usize) {
        let old = self
            .parent
            .insert(id.clone(), parent.clone())
            .expect("known");
        self.children
            .get_mut(&old)
            .expect("listed")
            .retain(|c| c != id);
        let list = self.children.entry(parent).or_default();
        list.insert(index.min(list.len()), id.clone());
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// What this copy meant to do, recorded by the generator itself (never read
/// back from the store), so tests can check a merge against it.
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
        Self::with_mirror(seed, Mirror::default())
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
    pub fn run<S: Store>(&mut self, s: &mut S, n: usize) {
        for _ in 0..n {
            let kind = self.pick_kind();
            let done = self.apply(s, kind);
            *self.counts.entry(done).or_default() += 1;
            s.commit();
        }
    }

    /// Applies one operation; falls back to `Create` when the chosen kind
    /// has no valid target yet. Returns what was actually done.
    fn apply<S: Store>(&mut self, s: &mut S, kind: OpKind) -> OpKind {
        let m = &self.mirror;
        let rng = &mut self.rng;
        match kind {
            OpKind::SetParam if !m.is_empty() => {
                let node = m.nodes[rng.below(m.len())].clone();
                let key = KEYS[rng.below(KEYS.len())];
                // Never the value already there: a write that changes
                // nothing is not recorded, so it is not an edit.
                let tag = rng.next_u64() >> 16;
                let value = if key == "name" || key == "color" {
                    Value::Str(format!("v{tag:x}"))
                } else {
                    Value::Num(tag as f64 / 16.0)
                };
                let op = Op::SetParam {
                    node: node.clone(),
                    key: key.into(),
                    value: Some(value.clone()),
                };
                s.apply(&op).expect("set param");
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
                    s.apply(&Op::DeleteText {
                        node: node.clone(),
                        pos,
                        len: del,
                    })
                    .expect("delete text");
                    *self.mirror.text_len.get_mut(&node).expect("text") -= del;
                } else {
                    let pos = rng.below(len + 1);
                    let n = 1 + rng.below(8);
                    let text: String = (0..n)
                        .map(|_| (b'a' + rng.below(26) as u8) as char)
                        .collect();
                    s.apply(&Op::InsertText {
                        node: node.clone(),
                        pos,
                        text,
                    })
                    .expect("insert text");
                    *self.mirror.text_len.get_mut(&node).expect("text") += n;
                }
                kind
            }
            OpKind::Remove if !m.is_empty() => {
                // Only leaves, so the mirror stays simple.
                let leaf = (0..8)
                    .map(|_| m.nodes[rng.below(m.len())].clone())
                    .find(|n| m.children.get(&Some(n.clone())).is_none_or(Vec::is_empty));
                let Some(node) = leaf else {
                    return self.apply(s, OpKind::Create);
                };
                s.apply(&Op::RemoveNode { node: node.clone() })
                    .expect("remove");
                self.mirror.drop_node(&node);
                self.intents.removed.insert(node);
                kind
            }
            OpKind::Connect if m.len() >= 2 => {
                let a = m.nodes[rng.below(m.len())].clone();
                let b = m.nodes[rng.below(m.len())].clone();
                if a == b {
                    return self.apply(s, OpKind::Create);
                }
                let edge = Edge {
                    from: a,
                    from_port: "out".into(),
                    to: b,
                    to_port: "in".into(),
                };
                let id = s
                    .apply(&Op::Connect { id: None, edge })
                    .expect("connect")
                    .edge
                    .expect("id");
                self.mirror.edges.push(id);
                kind
            }
            OpKind::Disconnect if !m.edges.is_empty() => {
                let i = rng.below(m.edges.len());
                let id = self.mirror.edges.swap_remove(i);
                s.apply(&Op::Disconnect { id }).expect("disconnect");
                kind
            }
            OpKind::Reorder if !m.is_empty() => {
                let stack = (0..8)
                    .map(|_| m.parent[&m.nodes[rng.below(m.len())]].clone())
                    .find(|p| m.children.get(p).is_some_and(|v| v.len() >= 2));
                let Some(parent) = stack else {
                    return self.apply(s, OpKind::Create);
                };
                let list = &m.children[&parent];
                let from = rng.below(list.len());
                let node = list[from].clone();
                // A different place: a move that changes nothing is not an edit.
                let to = (from + 1 + rng.below(list.len() - 1)) % list.len();
                s.apply(&Op::MoveNode {
                    node: node.clone(),
                    parent: parent.clone(),
                    index: to,
                })
                .expect("reorder");
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
                        return self.apply(s, OpKind::SetParam);
                    }
                    Some(p)
                };
                let siblings = m.children.get(&parent).map_or(&[][..], Vec::as_slice);
                let others = siblings.iter().filter(|c| **c != node).count();
                let to = rng.below(others + 1);
                if siblings.iter().position(|c| *c == node) == Some(to) {
                    return self.apply(s, OpKind::SetParam); // would change nothing
                }
                s.apply(&Op::MoveNode {
                    node: node.clone(),
                    parent: parent.clone(),
                    index: to,
                })
                .expect("move");
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
                let op = Op::CreateNode {
                    parent: parent.clone(),
                    index: None,
                    kind: if text { "text" } else { "shape" }.into(),
                };
                let id = s.apply(&op).expect("create").created.expect("id");
                self.intents.created.insert(id.clone());
                self.mirror.add(id, parent, text);
                OpKind::Create
            }
        }
    }
}

/// Merges both ways and checks the copies end identical.
pub fn sync<S: Store>(a: &mut S, b: &mut S) -> (MergeReport, MergeReport) {
    let ra = a.merge_from(b).expect("merge into a");
    let rb = b.merge_from(a).expect("merge into b");
    assert_eq!(
        a.state(),
        b.state(),
        "{}: copies differ after merging",
        S::NAME
    );
    (ra, rb)
}

/// Every node reachable from the roots exactly once, and nothing else.
pub fn assert_tree_is_sound(st: &State) {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&NodeId> = st.roots.iter().collect();
    while let Some(n) = stack.pop() {
        assert!(seen.insert(n.clone()), "node {n} appears twice");
        stack.extend(st.nodes[n].children.iter());
    }
    assert_eq!(seen.len(), st.nodes.len(), "unreachable nodes");
}

pub fn merged(reports: &[&MergeReport]) -> MergeReport {
    let mut all = MergeReport::default();
    for r in reports {
        all.concurrent_writes
            .extend(r.concurrent_writes.iter().cloned());
        all.lost_moves.extend(r.lost_moves.iter().cloned());
        all.hidden_by_removal
            .extend(r.hidden_by_removal.iter().cloned());
    }
    all
}

/// Every intent of a copy is either in the merged document or in the
/// report the program shows (achado A12: converging is not enough).
pub fn check_intents<S: Store>(s: &S, side: &Intents, report: &MergeReport, who: &str) {
    let hidden = |n: &NodeId| report.hidden_by_removal.contains(n);
    let lost_move = |n: &NodeId| report.lost_moves.iter().any(|m| &m.node == n);
    for ((node, key), v) in &side.last_param {
        if s.parent(node).is_none() {
            assert!(
                side.removed.contains(node) || hidden(node) || lost_move(node),
                "{} {who}: edit of {node}.{key} vanished with its node",
                S::NAME
            );
            continue;
        }
        let now = s.param(node, key);
        let reported = report
            .concurrent_writes
            .iter()
            .any(|w| &w.node == node && &w.key == key && w.lost.contains(v));
        assert!(
            now.as_ref() == Some(v) || reported,
            "{} {who}: {node}.{key} = {v:?} lost silently (now {now:?})",
            S::NAME
        );
    }
    for (node, wish) in &side.last_place {
        let ok = match s.parent(node) {
            None => side.removed.contains(node) || hidden(node) || lost_move(node),
            Some(p) => p == *wish || lost_move(node),
        };
        assert!(
            ok,
            "{} {who}: move of {node} to {wish:?} lost silently",
            S::NAME
        );
    }
    for node in &side.created {
        let ok = s.parent(node).is_some() || side.removed.contains(node) || hidden(node);
        assert!(
            ok,
            "{} {who}: created node {node} vanished silently",
            S::NAME
        );
    }
}

/// Runs `f` for seeds BIVAR_SEED_START..=BIVAR_SEEDS (default 1..=6). With
/// BIVAR_COUNT=1, every seed runs and the failing ones are listed at the end.
pub fn for_each_seed(f: impl Fn(u64) + std::panic::RefUnwindSafe) {
    let env = |name: &str, default: u64| {
        std::env::var(name)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };
    let (start, seeds) = (env("BIVAR_SEED_START", 1), env("BIVAR_SEEDS", 6));
    if std::env::var("BIVAR_COUNT").is_err() {
        (start..=seeds).for_each(f);
        return;
    }
    let failed: Vec<u64> = (start..=seeds)
        .filter(|s| std::panic::catch_unwind(|| f(*s)).is_err())
        .collect();
    assert!(
        failed.is_empty(),
        "{} of {seeds} seeds failed: {failed:?}",
        failed.len()
    );
}

/// Two copies, 300 base operations, 150 on each side, checked both ways.
pub fn nothing_lost_two_copies<S: Store>(seed: u64) {
    let mut a = S::new_document(1);
    let mut w = Workload::new(seed);
    w.run(&mut a, 300);
    let mut b = a.fork_as(2);
    let mut wa = Workload::with_mirror(seed * 1000 + 1, w.mirror.clone());
    let mut wb = Workload::with_mirror(seed * 1000 + 2, w.mirror.clone());
    wa.run(&mut a, 150);
    wb.run(&mut b, 150);
    let (ra, rb) = sync(&mut a, &mut b);
    assert_tree_is_sound(&a.state());
    for (rep, label) in [(&ra, "report on a"), (&rb, "report on b")] {
        check_intents(
            &a,
            &wa.intents,
            rep,
            &format!("seed {seed}, a's edits, {label}"),
        );
        check_intents(
            &a,
            &wb.intents,
            rep,
            &format!("seed {seed}, b's edits, {label}"),
        );
    }
}

/// Three copies: a receives b, then c; then b and c receive a. Checked on
/// the copy that did the merges. Copies that receive already-merged
/// changes rely on conflict records stored in the document (phase 0.7).
pub fn nothing_lost_three_copies<S: Store>(seed: u64) {
    let mut a = S::new_document(1);
    let mut w = Workload::new(seed);
    w.run(&mut a, 300);
    let mut b = a.fork_as(2);
    let mut c = a.fork_as(3);
    let mut wa = Workload::with_mirror(seed * 1000 + 1, w.mirror.clone());
    let mut wb = Workload::with_mirror(seed * 1000 + 2, w.mirror.clone());
    let mut wc = Workload::with_mirror(seed * 1000 + 3, w.mirror.clone());
    wa.run(&mut a, 100);
    wb.run(&mut b, 100);
    wc.run(&mut c, 100);
    let ab = a.merge_from(&mut b).expect("merge");
    let ac = a.merge_from(&mut c).expect("merge");
    b.merge_from(&mut a).expect("merge");
    c.merge_from(&mut a).expect("merge");
    assert_eq!(a.state(), b.state());
    assert_eq!(a.state(), c.state());
    assert_tree_is_sound(&a.state());
    let on_a = merged(&[&ab, &ac]);
    for (intents, who) in [(&wa.intents, "a"), (&wb.intents, "b"), (&wc.intents, "c")] {
        check_intents(&a, intents, &on_a, &format!("seed {seed}, {who}'s edits"));
    }
}

/// Three copies; every copy, not only the one that merged, must warn about
/// every concurrent parameter write that cost an edit (parameters only:
/// moves and hidden nodes still rely on the merging copy, phase 0.7).
pub fn params_warned_on_every_copy<S: Store>(seed: u64) {
    let mut a = S::new_document(1);
    let mut w = Workload::new(seed);
    w.run(&mut a, 300);
    let mut b = a.fork_as(2);
    let mut c = a.fork_as(3);
    let mut wa = Workload::with_mirror(seed * 1000 + 1, w.mirror.clone());
    let mut wb = Workload::with_mirror(seed * 1000 + 2, w.mirror.clone());
    let mut wc = Workload::with_mirror(seed * 1000 + 3, w.mirror.clone());
    wa.run(&mut a, 100);
    wb.run(&mut b, 100);
    wc.run(&mut c, 100);
    let ab = a.merge_from(&mut b).expect("merge");
    let ac = a.merge_from(&mut c).expect("merge");
    let ba = b.merge_from(&mut a).expect("merge");
    let ca = c.merge_from(&mut a).expect("merge");
    let on = [("a", merged(&[&ab, &ac])), ("b", ba), ("c", ca)];
    for (copy, report) in &on {
        for (intents, who) in [(&wa.intents, "a"), (&wb.intents, "b"), (&wc.intents, "c")] {
            for ((node, key), v) in &intents.last_param {
                if a.parent(node).is_none() {
                    continue; // hidden nodes: not what this check is about
                }
                let now = a.param(node, key);
                let warned = report
                    .concurrent_writes
                    .iter()
                    .any(|w| &w.node == node && &w.key == key && w.lost.contains(v));
                assert!(
                    now.as_ref() == Some(v) || warned,
                    "{} seed {seed}: copy {copy} did not warn that {who}'s {node}.{key} lost",
                    S::NAME
                );
            }
        }
    }
}

/// Three copies; every copy must warn about every edit that a merge hid
/// behind a removal (hidden nodes, the third kind of warning).
pub fn hidden_warned_on_every_copy<S: Store>(seed: u64) {
    let mut a = S::new_document(1);
    let mut w = Workload::new(seed);
    w.run(&mut a, 300);
    let mut b = a.fork_as(2);
    let mut c = a.fork_as(3);
    let mut wa = Workload::with_mirror(seed * 1000 + 1, w.mirror.clone());
    let mut wb = Workload::with_mirror(seed * 1000 + 2, w.mirror.clone());
    let mut wc = Workload::with_mirror(seed * 1000 + 3, w.mirror.clone());
    wa.run(&mut a, 100);
    wb.run(&mut b, 100);
    wc.run(&mut c, 100);
    let ab = a.merge_from(&mut b).expect("merge");
    let ac = a.merge_from(&mut c).expect("merge");
    let ba = b.merge_from(&mut a).expect("merge");
    let ca = c.merge_from(&mut a).expect("merge");
    let on = [("a", merged(&[&ab, &ac])), ("b", ba), ("c", ca)];
    for (copy, report) in &on {
        for (intents, who) in [(&wa.intents, "a"), (&wb.intents, "b"), (&wc.intents, "c")] {
            for (node, _) in intents.last_param.keys() {
                // Edits of nodes this copy itself removed, or that moved
                // away (lost moves are the open 0.7 part), are not checked.
                if a.parent(node).is_some() || intents.removed.contains(node) {
                    continue;
                }
                if report.lost_moves.iter().any(|m| &m.node == node) {
                    continue;
                }
                assert!(
                    report.hidden_by_removal.contains(node),
                    "{} seed {seed}: copy {copy} did not warn that {who}'s edit of {node} was hidden",
                    S::NAME
                );
            }
        }
    }
}

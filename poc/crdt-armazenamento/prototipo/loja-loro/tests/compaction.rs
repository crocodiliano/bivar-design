//! Compaction with two copies (audit 2026-10-31, item 3 and decision 2):
//! the history is cut by re-rooting (ADR-0023), and every test checks the
//! effect (the file shrank, the edit survived), not only the plan.

use bivar_poc_loja_loro::EntriesStore as S;
use bivar_poc_modelo::testkit::{Mirror, Workload, sync};
use bivar_poc_modelo::{NodeId, Op, Store, Value};

const DAY: u64 = 24 * 3600;

fn create(s: &mut S) -> NodeId {
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
    n
}

fn set(s: &mut S, n: &NodeId, key: &str, v: f64) {
    s.apply(&Op::SetParam {
        node: n.clone(),
        key: key.into(),
        value: Some(Value::Num(v)),
    })
    .unwrap();
    s.commit();
}

/// Two copies with a long concurrent history.
fn two_busy_copies() -> (S, S, NodeId) {
    let mut a = S::new_document(1);
    let n = create(&mut a);
    let mut b = a.fork_as(2);
    for round in 0..20 {
        for i in 0..200 {
            set(&mut a, &n, "x", f64::from(round * 1000 + i));
            set(&mut b, &n, "y", f64::from(round * 1000 + i));
        }
        sync(&mut a, &mut b);
    }
    (a, b, n)
}

/// Each copy confirms what it has after receiving the other's confirmation.
fn confirm_in_turn(a: &mut S, b: &mut S, at: u64) {
    a.record_seen("a", at);
    b.merge_from(a).unwrap();
    b.record_seen("b", at);
    a.merge_from(b).unwrap();
}

#[test]
fn a_two_copy_history_is_really_cut() {
    let (mut a, mut b, n) = two_busy_copies();
    confirm_in_turn(&mut a, &mut b, 1);
    confirm_in_turn(&mut a, &mut b, 2);
    let before = a.save().len();
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    assert!(plan.trim_before.is_some(), "{plan:?}");
    let state = a.state();
    let after = a.compact(&plan).unwrap();
    // The effect: the file is smaller, the document the same.
    assert!(
        after.len() * 4 < before,
        "history not cut: {before} -> {} bytes",
        after.len()
    );
    assert_eq!(a.state(), state);
    // Both copies keep merging, both ways, and nothing is lost.
    set(&mut a, &n, "x", 1.5);
    set(&mut b, &n, "y", 2.5);
    sync(&mut a, &mut b);
    assert_eq!(a.param(&n, "x"), Some(Value::Num(1.5)));
    assert_eq!(a.param(&n, "y"), Some(Value::Num(2.5)));
    // A second compaction of the cut document works too.
    confirm_in_turn(&mut a, &mut b, 3);
    confirm_in_turn(&mut a, &mut b, 4);
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    assert!(plan.trim_before.is_some(), "{plan:?}");
    let state = a.state();
    a.compact(&plan).unwrap();
    assert_eq!(a.state(), state);
    set(&mut b, &n, "y", 3.5);
    sync(&mut a, &mut b);
    assert_eq!(a.param(&n, "y"), Some(Value::Num(3.5)));
}

/// The owner's case: b edits before receiving a's checkpoint, so its edit
/// is beside the checkpoint, neither before nor after it. The plan must not
/// cut there; and whatever it does, the edit survives.
#[test]
fn an_edit_beside_the_cut_point_is_never_lost() {
    let (mut a, mut b, n) = two_busy_copies();
    a.record_seen("a", 1); // the checkpoint
    set(&mut b, &n, "z", 42.0); // beside it: b has not received it
    b.merge_from(&mut a).unwrap();
    b.record_seen("b", 1); // b confirms, the edit included
    a.merge_from(&mut b).unwrap();
    a.record_seen("a", 1);
    b.merge_from(&mut a).unwrap();
    // The plan cuts at a point after the edit, or refuses with a reason.
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    assert!(
        plan.trim_before.is_some() || plan.trim_refused.is_some(),
        "{plan:?}"
    );
    let state = a.state();
    a.compact(&plan).unwrap();
    assert_eq!(a.state(), state);
    assert_eq!(a.param(&n, "z"), Some(Value::Num(42.0)));
    // And it still merges with b, both ways.
    set(&mut b, &n, "z", 43.0);
    sync(&mut a, &mut b);
    assert_eq!(a.param(&n, "z"), Some(Value::Num(43.0)));
}

/// The checkpoint the edit is beside is never chosen, even when it is the
/// only one old enough; the plan says why.
#[test]
fn a_checkpoint_with_an_edit_beside_it_is_refused_with_a_reason() {
    let (mut a, mut b, n) = two_busy_copies();
    a.record_seen("a", 1); // the only old checkpoint
    set(&mut b, &n, "z", 42.0); // beside it
    b.merge_from(&mut a).unwrap();
    b.record_seen("b", 50 * DAY); // too recent to cut at
    a.merge_from(&mut b).unwrap();
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    assert_eq!(plan.trim_before, None, "{plan:?}");
    let why = plan.trim_refused.clone().expect("a reason");
    assert!(why.contains("without having received it"), "{why}");
    // Compacting with that plan cuts nothing and loses nothing.
    let state = a.state();
    a.compact(&plan).unwrap();
    assert_eq!(a.state(), state);
    assert_eq!(a.param(&n, "z"), Some(Value::Num(42.0)));
}

#[test]
fn the_cut_never_passes_a_named_milestone() {
    let (mut a, mut b, n) = two_busy_copies();
    confirm_in_turn(&mut a, &mut b, 1);
    a.add_milestone("Versão enviada ao cliente");
    let milestone = a.version_of_milestone("Versão enviada ao cliente");
    set(&mut a, &n, "x", 7.0);
    sync(&mut a, &mut b);
    confirm_in_turn(&mut a, &mut b, 2);
    confirm_in_turn(&mut a, &mut b, 3);
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    let trim = plan
        .trim_before
        .clone()
        .expect("a point before the milestone");
    assert!(bivar_poc_loja_loro::version_within(&trim, &milestone));
    a.compact(&plan).unwrap();
    // The milestone's version is still in the kept history.
    let at_milestone = a.updates_since(&milestone);
    assert!(at_milestone.is_ok(), "{at_milestone:?}");
}

/// I3: a removal one known copy has not received is never cleaned, and
/// it is cleaned once that copy has confirmed it.
#[test]
fn a_removal_not_yet_received_by_a_known_copy_is_not_cleaned() {
    let mut a = S::new_document(1);
    let keep = create(&mut a);
    let gone = create(&mut a);
    let mut b = a.fork_as(2);
    confirm_in_turn(&mut a, &mut b, 1);
    a.apply(&Op::RemoveNode { node: gone.clone() }).unwrap();
    a.commit();
    a.record_seen("a", 2);
    // b is known, but has not received the removal.
    let plan = a.plan_compaction(u64::MAX / 2, 0, false);
    assert!(!plan.purge.contains(&gone), "{plan:?}");
    a.compact(&plan).unwrap();
    assert!(a.is_stored(&gone), "the node was cleaned");
    // b receives it and confirms; now it can go.
    confirm_in_turn(&mut a, &mut b, 3);
    let plan = a.plan_compaction(u64::MAX / 2, 0, false);
    assert!(plan.purge.contains(&gone), "{plan:?}");
    a.compact(&plan).unwrap();
    assert!(!a.is_stored(&gone), "not cleaned");
    assert!(a.parent(&keep).is_some());
}

/// A random history of the approved mix, cut in the middle of the work.
#[test]
fn a_cut_in_the_middle_of_random_work_changes_nothing_visible() {
    let mut a = S::new_document(1);
    let mut w = Workload::new(5);
    w.run(&mut a, 300);
    let mut b = a.fork_as(2);
    for t in 1..=3 {
        let mirror = Mirror::from_state(&a.state());
        Workload::with_mirror(50 + t, mirror.clone()).run(&mut a, 100);
        Workload::with_mirror(60 + t, mirror).run(&mut b, 100);
        sync(&mut a, &mut b);
        confirm_in_turn(&mut a, &mut b, t);
    }
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    assert!(plan.trim_before.is_some(), "{plan:?}");
    let state = a.state();
    let before = a.save().len();
    let after = a.compact(&plan).unwrap().len();
    assert!(after < before, "{before} -> {after}");
    assert_eq!(a.state(), state);
    assert_eq!(b.state(), state);
}

/// I1: a connection made after a compaction gets an identity of its own and
/// replaces no live connection.
#[test]
fn a_connection_made_after_a_compaction_replaces_none() {
    let (mut a, mut b, _) = two_busy_copies();
    let nodes: Vec<NodeId> = (0..6).map(|_| create(&mut a)).collect();
    let connect = |s: &mut S, i: usize| {
        s.apply(&Op::Connect {
            id: None,
            edge: bivar_poc_modelo::Edge {
                from: nodes[i].clone(),
                from_port: "out".into(),
                to: nodes[i + 1].clone(),
                to_port: "in".into(),
            },
        })
        .unwrap();
        s.commit();
    };
    for i in 0..3 {
        connect(&mut a, i);
    }
    let before = a.edges();
    assert_eq!(before.len(), 3);
    sync(&mut a, &mut b);
    confirm_in_turn(&mut a, &mut b, 1);
    confirm_in_turn(&mut a, &mut b, 2);
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    a.compact(&plan).unwrap();
    for i in 3..5 {
        connect(&mut a, i);
    }
    let after = a.edges();
    assert_eq!(after.len(), 5, "{after:?}");
    for (id, e) in &before {
        assert_eq!(after.get(id), Some(e), "connection {id} replaced");
    }
}

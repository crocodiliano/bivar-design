//! The "one entry per write" way of storing parameters (owner's request,
//! 2026-10-29), run through the same checks as the register way.

use bivar_poc_loja_loro::{Entries, LoroStore, Register};
use bivar_poc_modelo::testkit::{
    for_each_seed, nothing_lost_three_copies, nothing_lost_two_copies, params_warned_on_every_copy,
    sync,
};
use bivar_poc_modelo::{Document, NodeId, Op, Store, Value};

type Ent = LoroStore<Entries>;

fn s(v: &str) -> Option<Value> {
    Some(Value::Str(v.into()))
}

fn base() -> (Ent, Ent, NodeId) {
    let mut a = Ent::new_document(1);
    let x = a
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "shape".into(),
        })
        .unwrap()
        .created
        .unwrap();
    a.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("green"),
    })
    .unwrap();
    a.commit();
    let b = a.fork_as(2);
    (a, b, x)
}

#[test]
fn concurrent_writes_stay_visible_and_the_bivar_picks_the_winner() {
    let (mut a, mut b, x) = base();
    a.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("red"),
    })
    .unwrap();
    a.commit();
    b.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("blue"),
    })
    .unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    // Same counter on both sides: the higher peer (b = 2) wins.
    assert_eq!(a.param(&x, "color"), s("blue"));
    for r in [ra, rb] {
        assert_eq!(r.concurrent_writes.len(), 1, "{r:?}");
        assert_eq!(r.concurrent_writes[0].lost, vec![Value::Str("red".into())]);
    }
}

#[test]
fn a_later_write_that_saw_both_clears_the_conflict() {
    let (mut a, mut b, x) = base();
    a.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("red"),
    })
    .unwrap();
    a.commit();
    b.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("blue"),
    })
    .unwrap();
    b.commit();
    sync(&mut a, &mut b);
    a.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("black"),
    })
    .unwrap();
    a.commit();
    let (ra, _) = sync(&mut b, &mut a);
    assert!(ra.concurrent_writes.is_empty(), "{ra:?}");
    assert_eq!(b.param(&x, "color"), s("black"));
}

#[test]
fn a_later_counter_wins_over_a_higher_peer() {
    let (mut a, mut b, x) = base();
    for v in ["r1", "r2", "r3"] {
        a.apply(&Op::SetParam {
            node: x.clone(),
            key: "color".into(),
            value: s(v),
        })
        .unwrap();
        a.commit();
    }
    b.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("blue"),
    })
    .unwrap();
    b.commit();
    sync(&mut a, &mut b);
    assert_eq!(a.param(&x, "color"), s("r3"));
}

#[test]
fn undo_never_overwrites_another_copys_edit() {
    let (a, b, x) = base();
    let (mut a, mut b) = (Document::new(a), Document::new(b));
    a.apply(Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("red"),
    })
    .unwrap();
    a.commit();
    b.store.merge_from(&mut a.store).unwrap();
    b.apply(Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: s("blue"),
    })
    .unwrap();
    b.commit();
    a.store.merge_from(&mut b.store).unwrap();
    let out = a.undo();
    assert!(!out.undone, "{out:?}");
    assert_eq!(a.store.param(&x, "color"), s("blue"));
}

#[test]
fn nothing_is_lost_silently_two_copies() {
    for_each_seed(nothing_lost_two_copies::<Ent>);
}

#[test]
fn nothing_is_lost_silently_three_copies() {
    for_each_seed(nothing_lost_three_copies::<Ent>);
}

#[test]
fn every_copy_warns_about_parameters() {
    for_each_seed(params_warned_on_every_copy::<Ent>);
}

/// Measurement for the comparison: how often the register way fails the
/// every-copy check. Expected to fail (known gap, phase 0.7).
#[test]
#[ignore = "comparison measurement"]
fn register_every_copy_warns_about_parameters() {
    for_each_seed(params_warned_on_every_copy::<LoroStore<Register>>);
}

#[test]
fn a_remove_restore_race_is_seen_by_every_copy() {
    let (mut a, mut b, x) = base();
    a.apply(&Op::RemoveNode { node: x.clone() }).unwrap();
    a.commit();
    sync(&mut a, &mut b);
    let mut c = a.fork_as(3);
    a.apply(&Op::RestoreNode { node: x.clone() }).unwrap();
    a.commit();
    c.apply(&Op::RestoreNode { node: x.clone() }).unwrap();
    c.apply(&Op::RemoveNode { node: x.clone() }).unwrap();
    c.commit();
    // a receives c; b receives everything second-hand from a.
    let ra = a.merge_from(&mut c).unwrap();
    let rb = b.merge_from(&mut a).unwrap();
    for r in [ra, rb] {
        assert!(
            r.concurrent_writes
                .iter()
                .any(|w| w.node == x && w.key == "@removed"),
            "{r:?}"
        );
    }
}

/// Known gap (phase 0.7): a copy that receives already-merged changes does
/// not see an edit hidden by another copy's removal. 1,947 of 2,000
/// scenarios failed on 2026-10-29. Kept as a measurement.
#[test]
#[ignore = "known gap, phase 0.7"]
fn every_copy_warns_about_hidden_edits() {
    for_each_seed(bivar_poc_modelo::testkit::hidden_warned_on_every_copy::<Ent>);
}

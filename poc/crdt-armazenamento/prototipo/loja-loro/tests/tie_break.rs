//! Locks Loro's "last write wins" rule (ADR-0021, owner's condition 1).
//!
//! Moves are the only thing whose winner Loro still decides in the chosen
//! store (parameters and the removal mark are entries, decided by the
//! Bivar): `concurrent_moves_follow_the_same_rule` protects them. The two
//! parameter tests document the register form that was set aside. If a new
//! Loro version picks differently, these tests fail and the update is
//! refused until the detection is reviewed.

use bivar_poc_loja_loro::{RegisterStore as LoroStore, last_write_order};
use bivar_poc_modelo::testkit::sync;
use bivar_poc_modelo::{NodeId, Op, Store, Value};

/// Base: one node with a parameter, forked into peers `pa` and `pb`.
fn pair(pa: u64, pb: u64) -> (LoroStore, LoroStore, NodeId) {
    let mut a = LoroStore::new_document(pa);
    let x = a
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "shape".into(),
        })
        .unwrap()
        .created
        .unwrap();
    a.commit();
    let b = a.fork_as(pb);
    (a, b, x)
}

fn set(s: &mut LoroStore, x: &NodeId, v: &str) {
    s.apply(&Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: Some(Value::Str(v.into())),
    })
    .unwrap();
    s.commit();
}

/// `(Lamport, peer)` of this copy's last color write, read from its history
/// the way the conflict detection reads it.
fn last_color_write(s: &mut LoroStore, x: &NodeId) -> (u64, u64) {
    s.last_param_write(x, "color").expect("a color write")
}

#[test]
fn equal_lamport_higher_peer_wins() {
    for (pa, pb) in [(1, 2), (2, 1), (7, 900)] {
        let (mut a, mut b, x) = pair(pa, pb);
        set(&mut a, &x, "from-a");
        set(&mut b, &x, "from-b");
        let (wa, wb) = (last_color_write(&mut a, &x), last_color_write(&mut b, &x));
        assert_eq!(
            wa.0, wb.0,
            "same history length gives the same Lamport time"
        );
        let expected = if wa > wb { "from-a" } else { "from-b" };
        sync(&mut a, &mut b);
        assert_eq!(
            a.param(&x, "color"),
            Some(Value::Str(expected.into())),
            "peers {pa}/{pb}"
        );
    }
}

#[test]
fn higher_lamport_wins_over_higher_peer() {
    let (mut a, mut b, x) = pair(1, 2);
    // a makes extra changes first, so its write has a later Lamport time.
    for i in 0..5 {
        set(&mut a, &x, &format!("a{i}"));
    }
    set(&mut b, &x, "from-b");
    let ((la, _), (lb, _)) = (last_color_write(&mut a, &x), last_color_write(&mut b, &x));
    assert!(la > lb);
    sync(&mut a, &mut b);
    assert_eq!(
        a.param(&x, "color"),
        Some(Value::Str("a4".into())),
        "Lamport {la} > {lb} wins over peer 2 > 1"
    );
}

#[test]
fn concurrent_moves_follow_the_same_rule() {
    for (pa, pb) in [(1, 2), (2, 1)] {
        let mut a = bivar_poc_loja_loro::EntriesStore::new_document(pa);
        let mk = |s: &mut bivar_poc_loja_loro::EntriesStore| {
            s.apply(&Op::CreateNode {
                parent: None,
                index: None,
                kind: "shape".into(),
            })
            .unwrap()
            .created
            .unwrap()
        };
        let (p1, p2, x) = (mk(&mut a), mk(&mut a), mk(&mut a));
        a.commit();
        let mut b = a.fork_as(pb);
        a.apply(&Op::MoveNode {
            node: x.clone(),
            parent: Some(p1.clone()),
            index: 0,
        })
        .unwrap();
        a.commit();
        b.apply(&Op::MoveNode {
            node: x.clone(),
            parent: Some(p2.clone()),
            index: 0,
        })
        .unwrap();
        b.commit();
        sync(&mut a, &mut b);
        let winner = if last_write_order(0, pa) > last_write_order(0, pb) {
            &p1
        } else {
            &p2
        };
        assert_eq!(a.parent(&x), Some(Some(winner.clone())), "peers {pa}/{pb}");
    }
}

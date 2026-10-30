//! The Loro store behind the library-independent model (ADR-0021).

use bivar_poc_loja_loro::EntriesStore as LoroStore;
use bivar_poc_modelo::testkit::{
    assert_tree_is_sound, for_each_seed, nothing_lost_three_copies, nothing_lost_two_copies, sync,
};
use bivar_poc_modelo::{Document, Edge, NodeId, Op, OpError, Store, Value, clean_copy};

fn create(s: &mut impl Store, parent: Option<&NodeId>, kind: &str) -> NodeId {
    s.apply(&Op::CreateNode {
        parent: parent.cloned(),
        index: None,
        kind: kind.into(),
    })
    .unwrap()
    .created
    .unwrap()
}

fn set(s: &mut impl Store, n: &NodeId, key: &str, v: &str) {
    s.apply(&Op::SetParam {
        node: n.clone(),
        key: key.into(),
        value: Some(Value::Str(v.into())),
    })
    .unwrap();
}

fn str_val(v: &str) -> Option<Value> {
    Some(Value::Str(v.into()))
}

/// Three roots P1, P2, X; X.color = green; forked into peers 1 and 2.
fn base() -> (LoroStore, LoroStore, [NodeId; 3]) {
    let mut a = LoroStore::new_document(1);
    let p1 = create(&mut a, None, "shape");
    let p2 = create(&mut a, None, "shape");
    let x = create(&mut a, None, "shape");
    set(&mut a, &x, "color", "green");
    a.commit();
    let b = a.fork_as(2);
    (a, b, [p1, p2, x])
}

#[test]
fn concurrent_write_is_reported_with_both_values() {
    let (mut a, mut b, [_, _, x]) = base();
    set(&mut a, &x, "color", "red");
    a.commit();
    set(&mut b, &x, "color", "blue");
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    for r in [ra, rb] {
        assert_eq!(r.concurrent_writes.len(), 1, "{r:?}");
        let w = &r.concurrent_writes[0];
        assert_eq!(w.kept, a.param(&x, "color"));
        assert_eq!(w.lost.len(), 1);
        assert_ne!(Some(&w.lost[0]), w.kept.as_ref());
    }
}

#[test]
fn moves_never_duplicate_or_cycle_and_the_loser_is_known() {
    let (mut a, mut b, [p1, p2, x]) = base();
    a.apply(&Op::MoveNode {
        node: x.clone(),
        parent: Some(p1.clone()),
        index: 0,
    })
    .unwrap();
    a.apply(&Op::MoveNode {
        node: p1.clone(),
        parent: Some(p2.clone()),
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
    b.apply(&Op::MoveNode {
        node: p2.clone(),
        parent: Some(p1.clone()),
        index: 0,
    })
    .unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    assert_tree_is_sound(&a.state());
    for r in [ra, rb] {
        assert!(r.lost_moves.iter().any(|m| m.node == x), "{r:?}");
        assert!(
            r.lost_moves.iter().any(|m| m.node == p1 || m.node == p2),
            "{r:?}"
        );
    }
}

#[test]
fn edit_of_a_removed_node_is_reported_and_restorable() {
    let (mut a, mut b, [_, _, x]) = base();
    a.apply(&Op::RemoveNode { node: x.clone() }).unwrap();
    a.commit();
    set(&mut b, &x, "color", "blue");
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    assert_eq!(ra.hidden_by_removal, vec![x.clone()]);
    assert_eq!(rb.hidden_by_removal, vec![x.clone()]);
    // Nothing was discarded: restoring brings the node back with b's edit.
    a.apply(&Op::RestoreNode { node: x.clone() }).unwrap();
    a.commit();
    assert_eq!(a.param(&x, "color"), str_val("blue"));
}

#[test]
fn remove_and_restore_race_is_reported() {
    let (mut a, mut b, [_, _, x]) = base();
    a.apply(&Op::RemoveNode { node: x.clone() }).unwrap();
    a.commit();
    sync(&mut a, &mut b);
    a.apply(&Op::RestoreNode { node: x.clone() }).unwrap();
    a.commit();
    b.apply(&Op::RestoreNode { node: x.clone() }).unwrap();
    b.apply(&Op::RemoveNode { node: x.clone() }).unwrap();
    b.commit();
    let (ra, _) = sync(&mut a, &mut b);
    assert!(
        ra.concurrent_writes
            .iter()
            .any(|w| w.node == x && w.key == "@removed"),
        "{ra:?}"
    );
}

#[test]
fn another_layout_version_is_refused() {
    let mut a = LoroStore::new_document(1);
    let bytes = a.save();
    assert!(LoroStore::load(&bytes, 2).is_ok());
    // A document from a newer layout: refused with a message, never guessed.
    let doc = loro::LoroDoc::from_snapshot(&bytes).unwrap();
    doc.get_map("meta")
        .insert("schema", bivar_poc_loja_loro::SCHEMA_VERSION + 1)
        .unwrap();
    doc.commit();
    let newer = doc.export(loro::ExportMode::Snapshot).unwrap();
    assert!(matches!(
        LoroStore::load(&newer, 3),
        Err(OpError::Invalid(_))
    ));
}

const MARKER: &str = "BIVAR-MARCADOR-7f3a9c2e-UNICO";
const IMAGE_HASH: &str = "blake3:0d1e2f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3e4f5";

#[test]
fn clean_copy_has_no_history_and_nothing_removed() {
    let mut a = LoroStore::new_document(1);
    let text = create(&mut a, None, "text");
    a.apply(&Op::InsertText {
        node: text.clone(),
        pos: 0,
        text: "Olá ".into(),
    })
    .unwrap();
    a.commit();
    a.apply(&Op::InsertText {
        node: text.clone(),
        pos: 4,
        text: MARKER.into(),
    })
    .unwrap();
    a.commit();
    let image = create(&mut a, None, "image");
    set(&mut a, &image, "hash", IMAGE_HASH);
    a.commit();
    a.apply(&Op::DeleteText {
        node: text.clone(),
        pos: 4,
        len: MARKER.chars().count(),
    })
    .unwrap();
    a.apply(&Op::RemoveNode {
        node: image.clone(),
    })
    .unwrap();
    a.commit();
    let dump = a.history_dump();
    assert!(
        dump.contains(MARKER) && dump.contains(IMAGE_HASH),
        "control: the working file keeps them"
    );

    let (mut copy, ids) = clean_copy(&a, 99, false);
    let bytes = copy.save();
    let mut reopened = LoroStore::load(&bytes, 100).unwrap();
    let dump = reopened.history_dump();
    for secret in [MARKER, IMAGE_HASH, "BIVAR-MARCADOR"] {
        assert!(!dump.contains(secret), "{secret} in the copy");
        assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()));
    }
    assert_eq!(reopened.history_peers().len(), 1);
    assert_eq!(reopened.text(&ids[&text]).as_deref(), Some("Olá "));
    assert_eq!(copy.merge_from(&mut a), Err(OpError::DifferentDocument));
}

#[test]
fn undo_removes_only_my_change() {
    let (a, b, [p1, _, x]) = base();
    let (mut a, mut b) = (Document::new(a), Document::new(b));
    a.apply(Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: str_val("red"),
    })
    .unwrap();
    a.commit();
    b.apply(Op::SetParam {
        node: p1.clone(),
        key: "color".into(),
        value: str_val("blue"),
    })
    .unwrap();
    b.commit();
    sync(&mut a.store, &mut b.store);
    let out = a.undo();
    assert!(out.undone && out.skipped.is_empty(), "{out:?}");
    assert_eq!(a.store.param(&x, "color"), str_val("green"));
    assert_eq!(a.store.param(&p1, "color"), str_val("blue"));
}

#[test]
fn undo_never_overwrites_another_copys_edit() {
    let (a, b, [_, _, x]) = base();
    let (mut a, mut b) = (Document::new(a), Document::new(b));
    a.apply(Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: str_val("red"),
    })
    .unwrap();
    a.commit();
    b.store.merge_from(&mut a.store).unwrap();
    b.apply(Op::SetParam {
        node: x.clone(),
        key: "color".into(),
        value: str_val("blue"),
    })
    .unwrap();
    b.commit();
    a.store.merge_from(&mut b.store).unwrap();
    let out = a.undo();
    assert!(!out.undone && out.skipped.len() == 1, "{out:?}");
    assert_eq!(
        a.store.param(&x, "color"),
        str_val("blue"),
        "b's edit stays"
    );
}

#[test]
fn a_transaction_of_500_operations_undoes_in_one_step() {
    let mut d = Document::new(LoroStore::new_document(1));
    let x = d
        .apply(Op::CreateNode {
            parent: None,
            index: None,
            kind: "shape".into(),
        })
        .unwrap()
        .created
        .unwrap();
    d.commit();
    let before = d.store.state();
    for i in 0..250 {
        let n = d.apply(Op::CreateNode {
            parent: Some(x.clone()),
            index: None,
            kind: "effect".into(),
        });
        let n = n.unwrap().created.unwrap();
        d.apply(Op::SetParam {
            node: n,
            key: "i".into(),
            value: Some(Value::Num(i as f64)),
        })
        .unwrap();
    }
    d.commit();
    assert_eq!(d.store.children(Some(&x)).len(), 250);
    let out = d.undo();
    assert!(out.undone && out.skipped.is_empty(), "{out:?}");
    assert_eq!(d.store.state(), before);
}

#[test]
fn undo_of_remove_brings_back_the_same_node_with_its_connections() {
    let mut d = Document::new(LoroStore::new_document(1));
    let mk = |d: &mut Document<LoroStore>| {
        d.apply(Op::CreateNode {
            parent: None,
            index: None,
            kind: "shape".into(),
        })
        .unwrap()
        .created
        .unwrap()
    };
    let (a, b) = (mk(&mut d), mk(&mut d));
    let edge = Edge {
        from: a.clone(),
        from_port: "out".into(),
        to: b.clone(),
        to_port: "in".into(),
    };
    d.apply(Op::Connect { id: None, edge }).unwrap();
    d.commit();
    let before = d.store.state();
    d.apply(Op::RemoveNode { node: b.clone() }).unwrap();
    d.commit();
    assert!(d.store.parent(&b).is_none());
    d.undo();
    assert_eq!(d.store.state(), before);
}

#[test]
fn text_undo_follows_other_copies_edits() {
    let mut a = LoroStore::new_document(1);
    let t = create(&mut a, None, "text");
    a.apply(&Op::InsertText {
        node: t.clone(),
        pos: 0,
        text: "abc".into(),
    })
    .unwrap();
    a.commit();
    let b = a.fork_as(2);
    let (mut a, mut b) = (Document::new(a), Document::new(b));
    a.apply(Op::InsertText {
        node: t.clone(),
        pos: 3,
        text: "XYZ".into(),
    })
    .unwrap();
    a.commit();
    b.apply(Op::InsertText {
        node: t.clone(),
        pos: 0,
        text: "123".into(),
    })
    .unwrap();
    b.commit();
    sync(&mut a.store, &mut b.store);
    assert_eq!(a.store.text(&t).as_deref(), Some("123abcXYZ"));
    a.undo();
    assert_eq!(a.store.text(&t).as_deref(), Some("123abc"));
}

#[test]
fn nothing_is_lost_silently_two_copies() {
    for_each_seed(nothing_lost_two_copies::<LoroStore>);
}

#[test]
fn nothing_is_lost_silently_three_copies() {
    for_each_seed(nothing_lost_three_copies::<LoroStore>);
}

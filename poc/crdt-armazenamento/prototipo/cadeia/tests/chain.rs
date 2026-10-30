//! The Shape -> Stroke -> Fill -> Output chain, on the Loro store.

use bivar_poc_cadeia::{
    Chain, add_effect, broken_links, build_chain, evaluation_order, is_customized, link,
    reorder_effect,
};
use bivar_poc_loja_loro::EntriesStore as LoroStore;
use bivar_poc_modelo::testkit::{assert_tree_is_sound, sync};
use bivar_poc_modelo::{Document, NodeId, Op, OpError, Store};

fn kinds(s: &LoroStore, object: &NodeId) -> Vec<String> {
    evaluation_order(s, object)
        .into_iter()
        .map(|(_, k)| k)
        .collect()
}

fn two_copies() -> (Document<LoroStore>, Document<LoroStore>, Chain) {
    let mut a = Document::new(LoroStore::new_document(1));
    let chain = build_chain(&mut a, None).unwrap();
    let b = Document::new(a.store.fork_as(2));
    (a, b, chain)
}

#[test]
fn the_chain_evaluates_in_stack_order() {
    let mut d = Document::new(LoroStore::new_document(1));
    let c = build_chain(&mut d, None).unwrap();
    assert_eq!(
        kinds(&d.store, &c.object),
        ["shape", "stroke", "fill", "output"]
    );
    assert!(!is_customized(&d.store, &c.object));
    assert_tree_is_sound(&d.store.state());
}

#[test]
fn reordering_stroke_and_fill_changes_the_order_and_undoes() {
    let mut d = Document::new(LoroStore::new_document(1));
    let c = build_chain(&mut d, None).unwrap();
    reorder_effect(&mut d, &c.object, &c.fill, 0).unwrap();
    assert_eq!(
        kinds(&d.store, &c.object),
        ["shape", "fill", "stroke", "output"]
    );
    d.undo();
    assert_eq!(
        kinds(&d.store, &c.object),
        ["shape", "stroke", "fill", "output"]
    );
}

#[test]
fn the_shape_and_the_output_never_move_in_the_stack() {
    let mut d = Document::new(LoroStore::new_document(1));
    let c = build_chain(&mut d, None).unwrap();
    assert!(matches!(
        reorder_effect(&mut d, &c.object, &c.output, 0),
        Err(OpError::Invalid(_))
    ));
    assert!(matches!(
        reorder_effect(&mut d, &c.object, &c.shape, 1),
        Err(OpError::Invalid(_))
    ));
}

#[test]
fn a_reorder_and_a_new_effect_from_two_copies_both_apply() {
    let (mut a, mut b, c) = two_copies();
    reorder_effect(&mut a, &c.object, &c.fill, 0).unwrap();
    let fx = add_effect(&mut b, &c.object, "blur").unwrap();
    let (ra, rb) = sync(&mut a.store, &mut b.store);
    assert!(ra.is_empty() && rb.is_empty(), "{ra:?} {rb:?}");
    let order: Vec<NodeId> = evaluation_order(&a.store, &c.object)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(
        order,
        [
            c.shape.clone(),
            c.fill.clone(),
            c.stroke.clone(),
            fx,
            c.output.clone()
        ]
    );
    assert!(!is_customized(&a.store, &c.object));
}

#[test]
fn reorders_of_the_same_effect_keep_one_and_report_the_other() {
    let (mut a, mut b, c) = two_copies();
    let fx = add_effect(&mut a, &c.object, "blur").unwrap();
    sync(&mut a.store, &mut b.store);
    reorder_effect(&mut a, &c.object, &fx, 0).unwrap();
    reorder_effect(&mut b, &c.object, &fx, 1).unwrap();
    let (ra, rb) = sync(&mut a.store, &mut b.store);
    for r in [ra, rb] {
        assert_eq!(r.lost_moves.len(), 1, "{r:?}");
        assert_eq!(r.lost_moves[0].node, fx);
    }
    let order = kinds(&a.store, &c.object);
    assert_eq!(
        order.iter().filter(|k| *k == "blur").count(),
        1,
        "never duplicated: {order:?}"
    );
    assert!(!is_customized(&a.store, &c.object));
}

#[test]
fn a_link_to_a_removed_object_is_a_broken_reference_on_every_copy() {
    let mut a = Document::new(LoroStore::new_document(1));
    let mask = build_chain(&mut a, None).unwrap();
    let card = build_chain(&mut a, None).unwrap();
    let mut b = Document::new(a.store.fork_as(2));
    // a uses `mask` as the fill mask of `card`; b removes `mask`.
    link(&mut a, &mask.output, &card.fill, "mask").unwrap();
    b.apply(Op::RemoveNode {
        node: mask.object.clone(),
    })
    .unwrap();
    b.commit();
    sync(&mut a.store, &mut b.store);
    assert_eq!(broken_links(&a.store).len(), 1);
    assert_eq!(broken_links(&a.store), broken_links(&b.store));
}

#[test]
fn a_stack_moved_out_of_shape_is_marked_customized_not_changed() {
    let (mut a, mut b, c) = two_copies();
    // a moves the output into another object's stack; b reorders as usual.
    let other = build_chain(&mut a, None).unwrap();
    a.apply(Op::MoveNode {
        node: c.output.clone(),
        parent: Some(other.object.clone()),
        index: 0,
    })
    .unwrap();
    a.commit();
    reorder_effect(&mut b, &c.object, &c.fill, 0).unwrap();
    sync(&mut a.store, &mut b.store);
    assert!(is_customized(&a.store, &c.object));
    assert!(is_customized(&a.store, &other.object));
    assert_tree_is_sound(&a.store.state());
}

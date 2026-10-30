//! The same tests for both libraries (phase 0.6, Loro x Automerge).

use std::collections::BTreeSet;

use bivar_poc_crdt::automerge_doc::AutomergeReplica;
use bivar_poc_crdt::loro_doc::LoroReplica;
use bivar_poc_crdt::model::{
    Edge, MergeReport, NodeId, OpError, REMOVED_MARK, Replica, State, Value, clean_copy,
};
use bivar_poc_crdt::workload::{Intents, Workload};

macro_rules! for_both {
    ($($name:ident),* $(,)?) => {
        mod loro {
            $( #[test] fn $name() { super::$name::<super::LoroReplica>(); } )*
        }
        mod automerge {
            $( #[test] fn $name() { super::$name::<super::AutomergeReplica>(); } )*
        }
    };
}

for_both!(
    save_and_load_keep_the_document,
    concurrent_write_is_detected_with_both_values,
    different_parameters_merge_without_warnings,
    concurrent_moves_of_one_node_keep_it_in_one_place,
    crossed_moves_never_form_a_cycle,
    reorders_of_different_items_both_apply,
    concurrent_reorders_of_one_item_report_the_loser,
    edit_of_a_removed_node_is_reported,
    undo_only_removes_my_change,
    clean_copy_has_no_history_and_no_deleted_content,
    clean_copy_does_not_merge_with_the_original,
    nothing_is_lost_silently,
    nothing_is_lost_silently_with_three_copies,
);

/// Merges both ways and checks the copies end identical.
fn sync<R: Replica>(a: &mut R, b: &mut R) -> (MergeReport, MergeReport) {
    let ra = a.merge_from(b).expect("merge into a");
    let rb = b.merge_from(a).expect("merge into b");
    assert_eq!(
        a.state(),
        b.state(),
        "{}: copies differ after merging",
        R::NAME
    );
    (ra, rb)
}

/// Every node reachable from the roots exactly once, and nothing else.
fn assert_tree_is_sound(st: &State) {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&NodeId> = st.roots.iter().collect();
    while let Some(n) = stack.pop() {
        assert!(seen.insert(n.clone()), "node {n} appears twice");
        stack.extend(st.nodes[n].children.iter());
    }
    assert_eq!(seen.len(), st.nodes.len(), "unreachable nodes");
}

fn num(v: f64) -> Value {
    Value::Num(v)
}

fn save_and_load_keep_the_document<R: Replica>() {
    let mut a = R::new_document(1);
    let shape = a.create_node(None, "shape").unwrap();
    let stroke = a.create_node(Some(&shape), "stroke").unwrap();
    let fill = a.create_node(Some(&shape), "fill").unwrap();
    let out = a.create_node(Some(&shape), "output").unwrap();
    let text = a.create_node(None, "text").unwrap();
    a.set_param(&stroke, "width", num(2.0)).unwrap();
    a.set_param(&fill, "color", Value::Str("#7c3aed".into()))
        .unwrap();
    a.insert_text(&text, 0, "Açaí à brasileira").unwrap();
    a.connect(&Edge {
        from: stroke.clone(),
        from_port: "out".into(),
        to: out.clone(),
        to_port: "in".into(),
    })
    .unwrap();
    a.move_node(&fill, Some(&shape), 0).unwrap();
    a.commit();
    assert_eq!(
        a.children(Some(&shape)),
        vec![fill.clone(), stroke.clone(), out.clone()]
    );
    let bytes = a.save();
    let b = R::load(&bytes, 2).unwrap();
    assert_eq!(a.state(), b.state());
    assert_eq!(b.text(&text).as_deref(), Some("Açaí à brasileira"));
    assert_tree_is_sound(&b.state());
}

/// Base document: three roots P1, P2, X and one parameter on X.
fn base<R: Replica>() -> (R, R, [NodeId; 3]) {
    let mut a = R::new_document(1);
    let p1 = a.create_node(None, "shape").unwrap();
    let p2 = a.create_node(None, "shape").unwrap();
    let x = a.create_node(None, "shape").unwrap();
    a.set_param(&x, "color", Value::Str("green".into()))
        .unwrap();
    a.commit();
    let b = a.fork_as(2);
    (a, b, [p1, p2, x])
}

fn concurrent_write_is_detected_with_both_values<R: Replica>() {
    let (mut a, mut b, [_, _, x]) = base::<R>();
    a.set_param(&x, "color", Value::Str("red".into())).unwrap();
    a.commit();
    b.set_param(&x, "color", Value::Str("blue".into())).unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    let kept = a.param(&x, "color").unwrap();
    for r in [&ra, &rb] {
        assert_eq!(r.concurrent_writes.len(), 1, "{}: {r:?}", R::NAME);
        let w = &r.concurrent_writes[0];
        assert_eq!((w.node.as_str(), w.key.as_str()), (x.as_str(), "color"));
        assert_eq!(w.kept.as_ref(), Some(&kept));
        let mut all = w.lost.clone();
        all.push(kept.clone());
        let mut names: Vec<String> = all
            .iter()
            .map(|v| match v {
                Value::Str(s) => s.clone(),
                Value::Num(n) => n.to_string(),
            })
            .collect();
        names.sort();
        assert_eq!(names, ["blue", "red"]);
    }
}

fn different_parameters_merge_without_warnings<R: Replica>() {
    let (mut a, mut b, [p1, _, x]) = base::<R>();
    a.set_param(&x, "color", Value::Str("red".into())).unwrap();
    a.commit();
    b.set_param(&p1, "width", num(3.0)).unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    assert!(ra.is_empty() && rb.is_empty(), "{ra:?} {rb:?}");
    assert_eq!(a.param(&x, "color"), Some(Value::Str("red".into())));
    assert_eq!(a.param(&p1, "width"), Some(num(3.0)));
}

fn concurrent_moves_of_one_node_keep_it_in_one_place<R: Replica>() {
    let (mut a, mut b, [p1, p2, x]) = base::<R>();
    a.move_node(&x, Some(&p1), 0).unwrap();
    a.commit();
    b.move_node(&x, Some(&p2), 0).unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    assert_tree_is_sound(&a.state());
    let final_parent = a.parent(&x).unwrap();
    assert!(final_parent == Some(p1.clone()) || final_parent == Some(p2.clone()));
    let loser = if final_parent == Some(p1.clone()) {
        &p2
    } else {
        &p1
    };
    for r in [&ra, &rb] {
        assert_eq!(r.lost_moves.len(), 1, "{}: {r:?}", R::NAME);
        assert_eq!(r.lost_moves[0].node, x);
        assert_eq!(r.lost_moves[0].wanted_parent.as_ref(), Some(loser));
    }
}

fn crossed_moves_never_form_a_cycle<R: Replica>() {
    let (mut a, mut b, [p1, p2, _]) = base::<R>();
    a.move_node(&p1, Some(&p2), 0).unwrap();
    a.commit();
    b.move_node(&p2, Some(&p1), 0).unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    let st = a.state();
    assert_tree_is_sound(&st);
    assert!(st.nodes.contains_key(&p1) && st.nodes.contains_key(&p2));
    for r in [&ra, &rb] {
        assert_eq!(r.lost_moves.len(), 1, "{}: {r:?}", R::NAME);
        let lost = &r.lost_moves[0];
        assert!(lost.node == p1 || lost.node == p2);
    }
}

fn reorders_of_different_items_both_apply<R: Replica>() {
    let mut a = R::new_document(1);
    let stack = a.create_node(None, "shape").unwrap();
    let items: Vec<NodeId> = (0..4)
        .map(|_| a.create_node(Some(&stack), "effect").unwrap())
        .collect();
    a.commit();
    let mut b = a.fork_as(2);
    // a: item 3 to the top; b: item 1 to the bottom.
    a.move_node(&items[3], Some(&stack), 0).unwrap();
    a.commit();
    b.move_node(&items[1], Some(&stack), 3).unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    assert!(ra.is_empty() && rb.is_empty(), "{ra:?} {rb:?}");
    let order = a.children(Some(&stack));
    assert_eq!(order.first(), Some(&items[3]), "{order:?}");
    assert_eq!(order.last(), Some(&items[1]), "{order:?}");
}

fn concurrent_reorders_of_one_item_report_the_loser<R: Replica>() {
    let mut a = R::new_document(1);
    let stack = a.create_node(None, "shape").unwrap();
    let items: Vec<NodeId> = (0..4)
        .map(|_| a.create_node(Some(&stack), "effect").unwrap())
        .collect();
    a.commit();
    let mut b = a.fork_as(2);
    a.move_node(&items[2], Some(&stack), 0).unwrap();
    a.commit();
    b.move_node(&items[2], Some(&stack), 3).unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    assert_tree_is_sound(&a.state());
    for r in [&ra, &rb] {
        assert_eq!(r.lost_moves.len(), 1, "{}: {r:?}", R::NAME);
        assert_eq!(r.lost_moves[0].node, items[2]);
        assert_eq!(r.lost_moves[0].wanted_parent.as_ref(), Some(&stack));
    }
}

fn edit_of_a_removed_node_is_reported<R: Replica>() {
    let (mut a, mut b, [_, _, x]) = base::<R>();
    a.remove_node(&x).unwrap();
    a.commit();
    b.set_param(&x, "color", Value::Str("blue".into())).unwrap();
    b.commit();
    let (ra, rb) = sync(&mut a, &mut b);
    for r in [&ra, &rb] {
        assert_eq!(r.hidden_by_removal, vec![x.clone()], "{}: {r:?}", R::NAME);
    }
}

fn undo_only_removes_my_change<R: Replica>() {
    let (mut a, mut b, [p1, _, x]) = base::<R>();
    a.set_param(&x, "color", Value::Str("red".into())).unwrap();
    a.commit();
    b.set_param(&p1, "color", Value::Str("blue".into()))
        .unwrap();
    b.commit();
    sync(&mut a, &mut b);
    let out = a.undo();
    assert!(out.undone, "{}: {out:?}", R::NAME);
    assert_eq!(a.param(&x, "color"), Some(Value::Str("green".into())));
    assert_eq!(a.param(&p1, "color"), Some(Value::Str("blue".into())));
    sync(&mut a, &mut b);
    assert_eq!(b.param(&x, "color"), Some(Value::Str("green".into())));
}

const MARKER: &str = "BIVAR-MARCADOR-7f3a9c2e-UNICO";
const IMAGE_HASH: &str = "blake3:0d1e2f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3e4f5";

fn document_with_deleted_secrets<R: Replica>() -> (R, NodeId) {
    let mut a = R::new_document(1);
    let text = a.create_node(None, "text").unwrap();
    a.insert_text(&text, 0, "Olá ").unwrap();
    a.commit();
    // Inserted at once, so the library stores it as one run.
    a.insert_text(&text, 4, MARKER).unwrap();
    a.commit();
    let image = a.create_node(None, "image").unwrap();
    a.set_param(&image, "hash", Value::Str(IMAGE_HASH.into()))
        .unwrap();
    a.commit();
    a.delete_text(&text, 4, MARKER.chars().count()).unwrap();
    a.commit();
    a.remove_node(&image).unwrap();
    a.commit();
    (a, text)
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

fn clean_copy_has_no_history_and_no_deleted_content<R: Replica>() {
    let (mut a, text) = document_with_deleted_secrets::<R>();
    // Control: the working file does keep them in its history.
    let dump = a.history_dump();
    assert!(
        dump.contains(MARKER),
        "{}: control failed, marker not in history",
        R::NAME
    );
    assert!(
        dump.contains(IMAGE_HASH),
        "{}: control failed, hash not in history",
        R::NAME
    );

    let (mut copy, ids) = clean_copy(&a, 99);
    let bytes = copy.save();
    assert!(!contains(&bytes, MARKER) && !contains(&bytes, IMAGE_HASH));
    // Read back the way the program reads it, history decoded.
    let mut reopened = R::load(&bytes, 100).unwrap();
    let dump = reopened.history_dump();
    assert!(!dump.contains(MARKER), "{}: marker in the copy", R::NAME);
    assert!(
        !dump.contains(IMAGE_HASH),
        "{}: image hash in the copy",
        R::NAME
    );
    assert!(
        !dump.contains("BIVAR-MARCADOR"),
        "{}: part of the marker in the copy",
        R::NAME
    );
    // Only the copy's own peer wrote its history: nothing from before.
    let peers = reopened.history_peers();
    assert_eq!(peers.len(), 1, "{}: {peers:?}", R::NAME);
    assert!(!a.history_peers().iter().any(|p| peers.contains(p)));
    assert_eq!(reopened.text(&ids[&text]).as_deref(), Some("Olá "));
}

fn clean_copy_does_not_merge_with_the_original<R: Replica>() {
    let (mut a, _) = document_with_deleted_secrets::<R>();
    let (mut copy, _) = clean_copy(&a, 99);
    assert_ne!(copy.doc_id(), a.doc_id());
    assert_eq!(copy.merge_from(&mut a), Err(OpError::DifferentDocument));
    assert_eq!(a.merge_from(&mut copy), Err(OpError::DifferentDocument));
}

/// Every intent of a copy is either in the merged document or in the
/// report the program shows (achado A12: converging is not enough).
fn check_intents<R: Replica>(r: &R, side: &Intents, report: &MergeReport, who: &str) {
    let hidden = |n: &NodeId| report.hidden_by_removal.contains(n);
    let lost_move = |n: &NodeId| report.lost_moves.iter().any(|m| &m.node == n);
    for ((node, key), v) in &side.last_param {
        if r.parent(node).is_none() {
            assert!(
                side.removed.contains(node) || hidden(node) || lost_move(node),
                "{} {who}: edit of {node}.{key} vanished with its node",
                R::NAME
            );
            continue;
        }
        let now = r.param(node, key);
        let reported = report
            .concurrent_writes
            .iter()
            .any(|w| &w.node == node && &w.key == key && w.lost.contains(v));
        assert!(
            now.as_ref() == Some(v) || reported,
            "{} {who}: {node}.{key} = {v:?} lost silently (now {now:?})",
            R::NAME
        );
    }
    for (node, wish) in &side.last_place {
        let now = r.parent(node);
        let ok = match &now {
            None => side.removed.contains(node) || hidden(node) || lost_move(node),
            Some(p) => p == wish || lost_move(node),
        };
        assert!(
            ok,
            "{} {who}: move of {node} to {wish:?} lost silently (now {now:?})",
            R::NAME
        );
    }
    for node in &side.created {
        let ok = r.parent(node).is_some() || side.removed.contains(node) || hidden(node);
        assert!(
            ok,
            "{} {who}: created node {node} vanished silently",
            R::NAME
        );
    }
}

/// Sanity check of one report against the state right after that merge.
fn check_lost_removals<R: Replica>(r: &R, report: &MergeReport) {
    for m in &report.lost_moves {
        if m.wanted_parent.as_deref() == Some(REMOVED_MARK) {
            assert!(
                r.parent(&m.node).is_some(),
                "a lost removal leaves the node visible"
            );
        }
    }
}

fn nothing_is_lost_silently<R: Replica>() {
    for_each_seed(|seed| {
        let mut a = R::new_document(1);
        let mut w = Workload::new(seed);
        w.run(&mut a, 300);
        let mut b = a.fork_as(2);
        let mut wa = Workload::with_mirror(seed * 1000 + 1, w.mirror.clone());
        let mut wb = Workload::with_mirror(seed * 1000 + 2, w.mirror.clone());
        wa.run(&mut a, 150);
        wb.run(&mut b, 150);
        let (ra, rb) = sync(&mut a, &mut b);
        assert_tree_is_sound(&a.state());
        check_lost_removals(&a, &ra);
        check_lost_removals(&b, &rb);
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
    });
}

fn merged(reports: &[&MergeReport]) -> MergeReport {
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

/// Three copies: a receives b, then c; then b and c receive a.
///
/// Checked: the copy that did the merges (a) warned about every loss.
/// Not checked, a known gap of the minimal detection (RELATORIO-PARTE2.md):
/// b and c receive a's already-merged changes and do not see conflicts
/// between two other copies; the warning must travel inside the document.
fn nothing_is_lost_silently_with_three_copies<R: Replica>() {
    for_each_seed(|seed| {
        let mut a = R::new_document(1);
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
        let ab = a.merge_from(&mut b).unwrap();
        let ac = a.merge_from(&mut c).unwrap();
        b.merge_from(&mut a).unwrap();
        c.merge_from(&mut a).unwrap();
        assert_eq!(a.state(), b.state());
        assert_eq!(a.state(), c.state());
        assert_tree_is_sound(&a.state());
        let on_a = merged(&[&ab, &ac]);
        for (intents, who) in [(&wa.intents, "a"), (&wb.intents, "b"), (&wc.intents, "c")] {
            check_intents(&a, intents, &on_a, &format!("seed {seed}, {who}'s edits"));
        }
    });
}

/// Runs `f` for seeds BIVAR_SEED_START..=BIVAR_SEEDS (default 1..=6). With
/// BIVAR_COUNT=1, every seed runs and the failing ones are listed at the
/// end, so the report can give a failure rate, not only the first failure.
fn for_each_seed(f: impl Fn(u64) + std::panic::RefUnwindSafe) {
    let env = |name: &str, default: u64| {
        std::env::var(name)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };
    let seeds = env("BIVAR_SEEDS", 6);
    let start = env("BIVAR_SEED_START", 1);
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

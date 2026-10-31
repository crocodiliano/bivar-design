//! What the Bivar assumes about Loro 1.17.1, each locked by a test (audit
//! 2026-10-31, I8, and the owner's investigation of the history cut). A new
//! Loro version is accepted only if all of these still hold (ADR-0021,
//! conditions for updating Loro). A failure here is not necessarily a bug
//! in Loro; it means an assumption of the Bivar must be revisited first.

use loro::{
    ContainerID, ContainerType, ExportMode, Frontiers, JsonOpContent, LoroDoc, LoroValue, TreeID,
    ValueOrContainer, VersionVector,
};

fn sync(a: &LoroDoc, b: &LoroDoc) {
    a.import(&b.export(ExportMode::all_updates()).unwrap())
        .unwrap();
    b.import(&a.export(ExportMode::all_updates()).unwrap())
        .unwrap();
}

/// `n` writes, one commit each; `tag` makes every value new.
fn edit(d: &LoroDoc, n: usize, tag: f64) {
    for i in 0..n {
        d.get_map("m").insert("k", tag + i as f64).unwrap();
        d.commit();
    }
}

/// A one-operation point: a distinct value, or Loro records nothing.
fn point(d: &LoroDoc, v: i64) -> Frontiers {
    d.get_map("cut").insert("p", v).unwrap();
    d.commit();
    d.oplog_frontiers()
}

/// Two copies with a concurrent history.
fn two_copies() -> (LoroDoc, LoroDoc) {
    let a = LoroDoc::new();
    a.set_peer_id(1).unwrap();
    a.get_map("m").insert("x", 0).unwrap();
    a.commit();
    let b = a.fork();
    b.set_peer_id(2).unwrap();
    for r in 0..5 {
        edit(&a, 2000, f64::from(r) * 1e6);
        edit(&b, 2000, f64::from(r) * 1e6 + 5e5);
        sync(&a, &b);
    }
    (a, b)
}

/// The Loro behavior that made re-rooting necessary: cutting at a past
/// point of a history with two copies keeps the whole history (Loro looks
/// for the cut point from the latest heads and does not find it). If this
/// test fails, Loro now cuts there itself, and re-rooting may be dropped.
#[test]
fn a_direct_cut_at_a_past_point_keeps_the_whole_two_copy_history() {
    let (a, b) = two_copies();
    let x = point(&a, 1);
    sync(&a, &b);
    edit(&a, 100, 7e6);
    edit(&b, 100, 8e6);
    sync(&a, &b);
    let cut = a.export(ExportMode::shallow_snapshot(&x)).unwrap();
    let kept = LoroDoc::from_snapshot(&cut).unwrap().len_ops();
    assert_eq!(kept, a.len_ops(), "Loro now cuts a two-copy history");
    // At the latest version, the cut works.
    let now = point(&a, 2);
    let cut = a.export(ExportMode::shallow_snapshot(&now)).unwrap();
    assert_eq!(LoroDoc::from_snapshot(&cut).unwrap().len_ops(), 1);
}

/// Re-rooting, step by step: the state alone at the point, then every
/// later change replayed. Same document, history from the point on, and it
/// still takes changes from the other copy. Also on a document already cut.
#[test]
fn re_rooting_gives_the_same_document_with_a_short_history() {
    let (a, b) = two_copies();
    let x = point(&a, 1);
    sync(&a, &b);
    edit(&a, 100, 7e6);
    edit(&b, 100, 8e6);
    sync(&a, &b);
    let reroot = |d: &LoroDoc, at: &Frontiers| {
        let r =
            LoroDoc::from_snapshot(&d.export(ExportMode::state_only(Some(at))).unwrap()).unwrap();
        let since = d.frontiers_to_vv(at).unwrap();
        r.import(&d.export(ExportMode::updates(&since)).unwrap())
            .unwrap();
        r
    };
    let r = reroot(&a, &x);
    assert_eq!(r.get_deep_value(), a.get_deep_value());
    assert_eq!(r.oplog_vv(), a.oplog_vv());
    assert_eq!(r.shallow_since_frontiers(), x);
    assert_eq!(r.len_ops(), 201, "the point and the 200 later writes");
    // It takes the other copy's later work.
    r.set_peer_id(1).unwrap();
    edit(&b, 50, 9e6);
    r.import(&b.export(ExportMode::updates(&r.oplog_vv())).unwrap())
        .unwrap();
    sync(&a, &b);
    assert_eq!(r.get_deep_value(), a.get_deep_value());
    // A document already cut is re-rooted the same way.
    let y = point(&r, 2);
    b.import(&r.export(ExportMode::updates(&b.oplog_vv())).unwrap())
        .unwrap();
    edit(&b, 30, 1e7);
    r.import(&b.export(ExportMode::updates(&r.oplog_vv())).unwrap())
        .unwrap();
    let r2 = reroot(&r, &y);
    assert_eq!(r2.get_deep_value(), r.get_deep_value());
    assert_eq!(r2.shallow_since_frontiers(), y);
    assert_eq!(r2.len_ops(), 31);
}

/// A change made beside the point (by a copy that had not received it)
/// depends on history before the point: it cannot be replayed onto the
/// re-rooted document. That is why the plan never picks such a point.
#[test]
fn a_change_beside_the_point_cannot_be_replayed_after_it() {
    let (a, b) = two_copies();
    let x = point(&a, 1);
    edit(&b, 1, 9e6); // beside x
    sync(&a, &b);
    let r = LoroDoc::from_snapshot(&a.export(ExportMode::state_only(Some(&x))).unwrap()).unwrap();
    let since = a.frontiers_to_vv(&x).unwrap();
    let replayed = r.import(&a.export(ExportMode::updates(&since)).unwrap());
    let same = replayed.is_ok() && r.get_deep_value() == a.get_deep_value();
    assert!(!same, "Loro now replays a change made beside the cut point");
}

/// The Bivar checks the result before using a re-rooted document; this is
/// what that check compares.
#[test]
fn a_cut_document_reports_where_its_history_starts() {
    let (a, _) = two_copies();
    let x = point(&a, 1);
    let cut = LoroDoc::from_snapshot(&a.export(ExportMode::shallow_snapshot(&x)).unwrap()).unwrap();
    assert!(cut.is_shallow());
    assert_eq!(cut.shallow_since_frontiers(), x);
}

/// A node's metadata map has the node's own identity (peer, counter): the
/// compaction finds each removal by it (I3).
#[test]
fn a_node_metadata_map_has_the_node_identity() {
    let d = LoroDoc::new();
    d.set_peer_id(5).unwrap();
    let tree = d.get_tree("tree");
    let t = tree.create(None).unwrap();
    tree.get_meta(t).unwrap().insert("removed#x", 1).unwrap();
    d.commit();
    let json = d
        .export_json_updates_without_peer_compression(&VersionVector::default(), &d.oplog_vv())
        .unwrap();
    let map_op = json
        .changes
        .iter()
        .flat_map(|c| &c.ops)
        .find(|op| matches!(op.content, JsonOpContent::Map(_)))
        .expect("the map write");
    assert_eq!(
        map_op.container,
        ContainerID::Normal {
            peer: t.peer,
            counter: t.counter,
            container_type: ContainerType::Map,
        }
    );
}

/// The node identity's text form `counter@peer` is stored inside every
/// connection; it must keep reading back the same.
#[test]
fn a_node_identity_reads_back_from_its_text() {
    let t = TreeID::new(0x1234, 56);
    assert_eq!(t.to_string(), "56@4660");
    assert_eq!(TreeID::try_from("56@4660").unwrap(), t);
}

/// Writing the value already there records nothing (the workload counts
/// on it: such a write is not an edit).
#[test]
fn writing_the_value_already_there_records_nothing() {
    let d = LoroDoc::new();
    d.get_map("m").insert("k", 1).unwrap();
    d.commit();
    let before = d.len_ops();
    d.get_map("m").insert("k", 1).unwrap();
    d.commit();
    assert_eq!(d.len_ops(), before);
}

/// A deleted map key stays in the state forever, compaction or not
/// (measured 2026-10-31: why entries are one per copy, not one per write).
#[test]
fn a_deleted_map_key_stays_in_the_state() {
    let d = LoroDoc::new();
    let m = d.get_map("m");
    for i in 0..1000 {
        m.insert(&format!("k{i}"), i).unwrap();
        m.delete(&format!("k{i}")).unwrap();
        d.commit();
    }
    let now = d.oplog_frontiers();
    let cut = d.export(ExportMode::shallow_snapshot(&now)).unwrap();
    assert!(
        cut.len() > 1000 * 4,
        "deleted keys left the state: {}",
        cut.len()
    );
}

/// An old version can be viewed on a cut document (the merge looks at the
/// other copy's view that way), and edits are refused while viewing it.
#[test]
fn an_old_version_can_be_viewed_on_a_cut_document_and_not_edited() {
    let (a, b) = two_copies();
    let x = point(&a, 1);
    sync(&a, &b);
    let r = LoroDoc::from_snapshot(&a.export(ExportMode::shallow_snapshot(&x)).unwrap()).unwrap();
    r.set_peer_id(1).unwrap();
    edit(&r, 10, 3e7);
    r.checkout(&x).unwrap();
    assert!(r.is_detached());
    assert!(
        r.get_map("m").insert("k", -1).is_err(),
        "edit on an old version"
    );
    r.checkout_to_latest();
    assert!(!r.is_detached());
    assert!(matches!(
        r.get_map("m").get("k"),
        Some(ValueOrContainer::Value(LoroValue::Double(v))) if v == 3e7 + 9.0
    ));
}

/// The history in JSON has what the conflict detection reads: map inserts
/// with key and value, each change's dependencies and Lamport time.
#[test]
fn the_history_in_json_has_what_the_detection_reads() {
    let (a, b) = two_copies();
    edit(&b, 1, 4e7);
    let json = b
        .export_json_updates_without_peer_compression(&a.oplog_vv(), &b.oplog_vv())
        .unwrap();
    let change = json.changes.last().expect("one change");
    assert!(!change.deps.is_empty());
    assert!(change.lamport > 0);
    let JsonOpContent::Map(loro::JsonMapOp::Insert { key, value }) = &change.ops[0].content else {
        panic!("not a map insert: {:?}", change.ops[0].content);
    };
    assert_eq!(key, "k");
    assert_eq!(*value, LoroValue::Double(4e7));
}

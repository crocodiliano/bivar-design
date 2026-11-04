//! The copy name (owner's decision 1, 2026-10-31): two sessions never write
//! the same parameter entries without knowing. The name is reused only for
//! a file this computer wrote last, exactly as it is; and when two sessions
//! do share a name, the merge still finds their writes and loses neither.
//! Each case runs as is and after a compaction.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bivar_poc_arquivo::{DocFile, Limits, TrustStore};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::{MergeReport, NodeId, Op, Store, Value};
use bivar_poc_verificador::{How, VerifyLimits, open_checked};

type S = EntriesStore;

const VERIFIER: &str = env!("CARGO_BIN_EXE_bivar-poc-verificador");
const DAY: u64 = 24 * 3600;

fn dir() -> PathBuf {
    let d = std::env::var("BIVAR_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("bivar-poc-copias"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fresh(name: &str) -> PathBuf {
    let p = dir().join(format!("{name}.bivar"));
    let _ = std::fs::remove_file(&p);
    p
}

fn trust(name: &str) -> TrustStore {
    let p = dir().join(format!("trust-{name}.txt"));
    let _ = std::fs::remove_file(&p);
    TrustStore::new(p)
}

thread_local! {
    /// Why each file opened by `open` in this test was read-only, if it
    /// was: a failed save shows it (CI failure of 2026-11-03, dd7d0af).
    static READ_ONLY: RefCell<HashMap<PathBuf, Option<String>>> = RefCell::new(HashMap::new());
}

fn open(path: &Path, t: &TrustStore, peer: u64) -> (DocFile, S, How) {
    let (f, s, report, how) = open_checked::<S>(
        path,
        peer,
        t,
        Path::new(VERIFIER),
        &Limits::default(),
        &VerifyLimits::default(),
    )
    .unwrap();
    READ_ONLY.with(|r| r.borrow_mut().insert(path.to_owned(), report.read_only));
    (f, s, how)
}

/// Saves and notes the file as this computer's, with the session's name.
/// A failed save says whether the file was open for writing and, if not,
/// why it opened read-only.
fn save(f: &mut DocFile, s: &mut S, path: &Path, t: &TrustStore) {
    if let Err(e) = f.save(s, "pc") {
        let why = READ_ONLY.with(|r| r.borrow().get(path).cloned());
        let why = match why {
            Some(Some(reason)) => format!("opened read-only: {reason}"),
            Some(None) => "opened for writing, no read-only reason".to_owned(),
            None => "opened by DocFile::create, which does not report a reason".to_owned(),
        };
        panic!(
            "saving {} failed: {e:?}; writable: {}; {why}",
            path.display(),
            f.is_writable()
        );
    }
    t.remember(path, &f.fingerprint(), s.copy_name()).unwrap();
}

fn set(s: &mut S, n: &NodeId, v: &str) {
    s.apply(&Op::SetParam {
        node: n.clone(),
        key: "name".into(),
        value: Some(Value::Str(v.into())),
    })
    .unwrap();
    s.commit();
}

/// A document written on computer 1 at `path`, with one node.
fn original(tag: &str, t1: &TrustStore) -> (PathBuf, NodeId) {
    let path = fresh(tag);
    let mut s = S::new_document(1);
    s.set_copy_name(0xc0de_0001);
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
    let mut f = DocFile::create(&path, &mut s).unwrap();
    save(&mut f, &mut s, &path, t1);
    (path, n)
}

/// With `compact`, the two sessions meet, confirm in turn and `a` cuts its
/// history first (and the cut is checked to have happened).
fn maybe_compact(a: &mut S, b: &mut S, compact: bool) {
    if !compact {
        return;
    }
    a.merge_from(b).unwrap();
    b.merge_from(a).unwrap();
    // `a` confirms as "pc", the known copy the file was saved as; with any
    // other name, "pc" would hold the history back (and rightly so).
    for at in [1, 2] {
        a.record_seen("pc", at);
        b.merge_from(a).unwrap();
        b.record_seen("b", at);
        a.merge_from(b).unwrap();
    }
    let plan = a.plan_compaction(40 * DAY, 30 * DAY, false);
    assert!(plan.trim_before.is_some(), "{plan:?}");
    let before = a.history_dump().len();
    a.compact(&plan).unwrap();
    assert!(a.history_dump().len() < before, "history not cut");
}

/// Both writes are in the merged document or in the merge report.
fn neither_lost(s: &S, n: &NodeId, report: &MergeReport, values: [&str; 2]) {
    for v in values {
        let v = Value::Str(v.into());
        let shown = s.param(n, "name") == Some(v.clone());
        let reported = report
            .concurrent_writes
            .iter()
            .any(|w| &w.node == n && w.key == "name" && w.lost.contains(&v));
        assert!(shown || reported, "{v:?} lost silently: {report:?}");
    }
    assert!(
        !report.concurrent_writes.is_empty(),
        "no warning: {report:?}"
    );
}

/// Two sessions write the same parameter without seeing each other, then
/// merge both ways.
fn race(a: &mut S, b: &mut S, n: &NodeId, compact: bool) {
    maybe_compact(a, b, compact);
    set(a, n, "from a");
    set(b, n, "from b");
    let ra = a.merge_from(b).unwrap();
    neither_lost(a, n, &ra, ["from a", "from b"]);
    let rb = b.merge_from(a).unwrap();
    assert_eq!(a.state(), b.state());
    assert!(!a.open_conflicts().is_empty() || !rb.concurrent_writes.is_empty());
}

fn copied_to_another_computer(compact: bool) {
    let tag = format!("outro-pc-{compact}");
    let (t1, t2) = (trust(&format!("{tag}-1")), trust(&format!("{tag}-2")));
    let (p1, n) = original(&tag, &t1);
    let p2 = fresh(&format!("{tag}-la"));
    std::fs::copy(&p1, &p2).unwrap();
    let (_f1, mut a, how1) = open(&p1, &t1, 11);
    let (_f2, mut b, how2) = open(&p2, &t2, 12);
    assert_eq!((how1, how2), (How::Trusted, How::Verified));
    assert_eq!(a.copy_name(), 0xc0de_0001);
    assert_ne!(b.copy_name(), a.copy_name());
    race(&mut a, &mut b, &n, compact);
}

fn duplicated_on_this_computer(compact: bool) {
    let tag = format!("duplicado-{compact}");
    let t = trust(&tag);
    let (p, n) = original(&tag, &t);
    let dup = fresh(&format!("{tag}-copia"));
    std::fs::copy(&p, &dup).unwrap();
    let (_f1, mut a, how1) = open(&p, &t, 11);
    let (_f2, mut b, how2) = open(&dup, &t, 12);
    assert_eq!((how1, how2), (How::Trusted, How::Verified));
    assert_eq!(a.copy_name(), 0xc0de_0001, "the original keeps its name");
    assert_ne!(b.copy_name(), a.copy_name());
    race(&mut a, &mut b, &n, compact);
}

fn old_backup_restored(compact: bool) {
    let tag = format!("backup-{compact}");
    let t = trust(&tag);
    let (p, n) = original(&tag, &t);
    let backup = std::fs::read(&p).unwrap();
    // A later session edits and saves; a colleague got that version.
    let (mut f, mut later, _) = open(&p, &t, 11);
    let mut colleague = later.fork_as(30);
    save(&mut f, &mut later, &p, &t);
    drop(f);
    // The old backup goes back in place.
    std::fs::write(&p, &backup).unwrap();
    let (_f, mut restored, how) = open(&p, &t, 12);
    assert_eq!(how, How::Verified, "the restored file is not trusted");
    assert_ne!(restored.copy_name(), later.copy_name());
    // The restored session and the later one's colleague race.
    race(&mut restored, &mut colleague, &n, compact);
}

fn synced_folder_on_two_computers(compact: bool) {
    let tag = format!("pasta-sincronizada-{compact}");
    let (t1, t2) = (trust(&format!("{tag}-1")), trust(&format!("{tag}-2")));
    let (p1, n) = original(&tag, &t1);
    // The sync tool puts the same file on computer 2; both open it at once.
    let p2 = fresh(&format!("{tag}-pc2"));
    std::fs::copy(&p1, &p2).unwrap();
    let (_f1, mut a, _) = open(&p1, &t1, 11);
    let (mut f2, mut b, how) = open(&p2, &t2, 12);
    assert_eq!(how, How::Verified);
    assert_ne!(b.copy_name(), a.copy_name());
    race(&mut a, &mut b, &n, compact);
    // Computer 2 saves and the sync tool brings its file back to computer
    // 1: computer 1 did not write it last, so it gets a new name too.
    save(&mut f2, &mut b, &p2, &t2);
    drop(f2);
    drop(_f1);
    std::fs::copy(&p2, &p1).unwrap();
    let (_f, again, how) = open(&p1, &t1, 13);
    assert_eq!(how, How::Verified);
    assert_ne!(again.copy_name(), a.copy_name());
}

/// The case the list cannot catch: the list itself copied to another
/// computer along with the file (a whole computer restored from an image).
/// Both sessions get the same name; the merge finds it from the history.
fn the_same_name_on_two_sessions(compact: bool) {
    let tag = format!("mesmo-nome-{compact}");
    let (t1, t2) = (trust(&format!("{tag}-1")), trust(&format!("{tag}-2")));
    let (p1, n) = original(&tag, &t1);
    let p2 = fresh(&format!("{tag}-clone"));
    std::fs::copy(&p1, &p2).unwrap();
    let fp = DocFile::open::<S>(&p2, 99).unwrap().0.fingerprint();
    t2.remember(&p2, &fp, 0xc0de_0001).unwrap();
    let (_f1, mut a, how1) = open(&p1, &t1, 11);
    let (_f2, mut b, how2) = open(&p2, &t2, 12);
    assert_eq!((how1, how2), (How::Trusted, How::Trusted));
    assert_eq!(a.copy_name(), b.copy_name(), "test setup: same name");
    assert_ne!(a.peer(), b.peer(), "sessions always differ");
    race(&mut a, &mut b, &n, compact);
}

#[test]
fn a_file_copied_to_another_computer_gets_a_new_name() {
    copied_to_another_computer(false);
    copied_to_another_computer(true);
}

#[test]
fn a_file_duplicated_on_this_computer_gets_a_new_name() {
    duplicated_on_this_computer(false);
    duplicated_on_this_computer(true);
}

#[test]
fn an_old_backup_restored_in_place_gets_a_new_name() {
    old_backup_restored(false);
    old_backup_restored(true);
}

#[test]
fn a_file_in_a_synced_folder_gets_a_name_per_computer() {
    synced_folder_on_two_computers(false);
    synced_folder_on_two_computers(true);
}

#[test]
fn two_sessions_with_the_same_name_still_lose_nothing() {
    the_same_name_on_two_sessions(false);
    the_same_name_on_two_sessions(true);
}

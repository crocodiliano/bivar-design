//! The .bivar file on disk: storage risks of phase 0.6 (achado A6).
//!
//! Files go to BIVAR_TEST_DIR (default: the system temp folder, on C:). To
//! test the hard disk, point it at A:. Kill tests start this same test
//! binary as a child process and kill it mid-write; BIVAR_KILLS sets how
//! many times (default 10).

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use bivar_poc_arquivo::{
    DocFile, FileError, HEADER_LEN, backup_path, migrate, recover_from_backup,
};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::testkit::{Rng, Workload};
use bivar_poc_modelo::{NodeId, Op, OpError, Store, Value};

type S = EntriesStore;

fn dir() -> PathBuf {
    let d = std::env::var("BIVAR_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("bivar-poc-arquivo"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fresh(name: &str) -> PathBuf {
    let p = dir().join(format!("{name}.bivar"));
    for suffix in [
        "",
        ".compact-tmp",
        ".v1.bak",
        ".anterior",
        ".estragado",
        ".parcial",
    ] {
        let mut s = p.as_os_str().to_owned();
        s.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(s));
    }
    p
}

fn kills() -> usize {
    std::env::var("BIVAR_KILLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
}

fn create(s: &mut S, kind: &str) -> NodeId {
    s.apply(&Op::CreateNode {
        parent: None,
        index: None,
        kind: kind.into(),
    })
    .unwrap()
    .created
    .unwrap()
}

fn set(s: &mut S, n: &NodeId, key: &str, v: Value) {
    s.apply(&Op::SetParam {
        node: n.clone(),
        key: key.into(),
        value: Some(v),
    })
    .unwrap();
    s.commit();
}

fn new_file(name: &str) -> (PathBuf, DocFile, S) {
    let path = fresh(name);
    let mut s = S::new_document(1);
    Workload::new(5).run(&mut s, 200);
    let f = DocFile::create(&path, &mut s).unwrap();
    (path, f, s)
}

#[test]
fn save_and_reopen_keep_the_document() {
    let (path, mut f, mut s) = new_file("roundtrip");
    Workload::new(6).run(&mut s, 300);
    f.save(&mut s, "pc-1").unwrap();
    let state = s.state();
    drop(f);
    let (_, back, report) = DocFile::open::<S>(&path, 2).unwrap();
    assert_eq!(back.state(), state);
    assert_eq!(report, Default::default());
}

/// The format's answer to "a trigger or a view injected into the file"
/// (phase 0.6, finding C1): there is nothing to run, and a record of a kind
/// the format does not define, even with a correct digest, is damage. What
/// came before opens read-only, with a message; nothing after it is used.
#[test]
fn an_injected_record_of_an_unknown_kind_is_refused() {
    let (path, mut f, mut s) = new_file("injected");
    f.save(&mut s, "pc-1").unwrap();
    let before = s.state();
    f.append_raw(200, b"SELECT * FROM everything; -- not a Bivar record")
        .unwrap();
    let x = create(&mut s, "shape");
    set(&mut s, &x, "x", Value::Num(1.0));
    f.save(&mut s, "pc-1").unwrap();
    drop(f);
    let (_, back, report) = DocFile::open::<S>(&path, 2).unwrap();
    let damage = report.damaged_at.clone().expect("reported as damage");
    assert!(report.read_only.is_some(), "{report:?}");
    assert_eq!(back.state(), before, "nothing after the injected record");
    println!("message: {damage}");
}

#[test]
fn edits_not_saved_are_back_after_reopening() {
    let (path, mut f, mut s) = new_file("unsaved");
    let x = create(&mut s, "shape");
    for i in 0..25 {
        set(&mut s, &x, "x", Value::Num(i as f64));
        f.append_changes(&mut s).unwrap(); // what autosave does after each edit
    }
    let state = s.state();
    drop(f); // no save, no close
    let (_, back, report) = DocFile::open::<S>(&path, 2).unwrap();
    assert_eq!(back.state(), state);
    assert!(report.edits_after_last_save >= 25, "{report:?}");
}

#[test]
fn a_cut_last_write_is_dropped_and_everything_before_stays() {
    let (path, mut f, mut s) = new_file("torn");
    f.save(&mut s, "pc-1").unwrap();
    let before = s.state();
    let len_before = f.len();
    let x = create(&mut s, "shape");
    set(&mut s, &x, "color", Value::Str("blue".into()));
    f.append_changes(&mut s).unwrap();
    let full = f.len();
    drop(f);
    let original = std::fs::read(&path).unwrap();
    for cut in (len_before + 1)..full {
        std::fs::write(&path, &original[..cut as usize]).unwrap();
        let (f, back, report) = DocFile::open::<S>(&path, 2).unwrap();
        assert_eq!(back.state(), before, "cut at {cut}");
        assert!(report.dropped_tail.is_some(), "cut at {cut}");
        // The cut part was removed from the file, so it can grow again.
        assert_eq!(f.len(), len_before);
        drop(f);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), len_before);
    }
}

#[test]
fn damage_in_the_middle_opens_what_it_can_read_only() {
    let (path, mut f, mut s) = new_file("damaged");
    let x = create(&mut s, "shape");
    for i in 0..10 {
        set(&mut s, &x, "x", Value::Num(i as f64));
        f.append_changes(&mut s).unwrap();
    }
    let first_update_end = f.len();
    drop(f);
    let mut bytes = std::fs::read(&path).unwrap();
    // Flip one byte inside the first records, well before the end.
    let at = (first_update_end as usize) / 2;
    bytes[at] ^= 0x5a;
    std::fs::write(&path, &bytes).unwrap();
    match DocFile::open::<S>(&path, 2) {
        Ok((f, _, report)) => {
            assert!(!f.is_writable());
            assert!(
                report.damaged_at.is_some() || report.dropped_tail.is_some(),
                "{report:?}"
            );
        }
        Err(e) => assert!(
            matches!(e, FileError::Unreadable(_) | FileError::Document(_)),
            "{e:?}"
        ),
    }
}

#[test]
fn random_damage_never_crashes_the_reader() {
    let (path, mut f, mut s) = new_file("random-damage");
    Workload::new(8).run(&mut s, 200);
    f.save(&mut s, "pc-1").unwrap();
    drop(f);
    let original = std::fs::read(&path).unwrap();
    let mut rng = Rng::new(99);
    for _ in 0..300 {
        let mut bytes = original.clone();
        for _ in 0..1 + rng.below(4) {
            let at = rng.below(bytes.len());
            bytes[at] = rng.next_u64() as u8;
        }
        if rng.below(3) == 0 {
            bytes.truncate(rng.below(bytes.len()));
        }
        std::fs::write(&path, &bytes).unwrap();
        // Ok or a clear error; a panic fails the test.
        let _ = DocFile::open::<S>(&path, 2);
    }
}

#[test]
fn a_second_opening_is_read_only_with_a_reason() {
    let (path, f, _) = new_file("twice");
    let (second, _, report) = DocFile::open::<S>(&path, 2).unwrap();
    assert!(f.is_writable());
    assert!(!second.is_writable());
    assert!(report.read_only.is_some());
}

#[test]
fn large_files_are_stored_once_and_a_parameter_change_rewrites_no_piece() {
    let (path, mut f, mut s) = new_file("blob");
    let data: Vec<u8> = (0..10_000_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    let h = f.put_blob(&data[..]).unwrap();
    let again = f.put_blob(&data[..]).unwrap();
    assert_eq!(h, again);
    let img = create(&mut s, "image");
    set(&mut s, &img, "blob", Value::Str(h.clone()));
    f.save(&mut s, "pc-1").unwrap();
    let len = f.len() as usize;
    let before = std::fs::read(&path).unwrap();
    assert!(
        before.len() < data.len() * 11 / 10,
        "stored once: {} bytes",
        before.len()
    );
    set(&mut s, &img, "x", Value::Num(3.0));
    f.save(&mut s, "pc-1").unwrap();
    let after = std::fs::read(&path).unwrap();
    assert_eq!(
        &after[..len],
        &before[..],
        "nothing already written changed"
    );
    let mut out = Vec::new();
    f.read_blob(&h, &mut out).unwrap();
    assert_eq!(out, data);
    drop(f);
    let (f, _, _) = DocFile::open::<S>(&path, 2).unwrap();
    let mut out = Vec::new();
    assert_eq!(f.read_blob(&h, &mut out).unwrap(), data.len() as u64);
    assert_eq!(out, data);
}

#[test]
fn migration_keeps_a_copy_never_harms_the_original_and_newer_files_are_refused() {
    let (path, mut f, mut s) = new_file("migrate");
    f.save(&mut s, "pc-1").unwrap();
    let state = s.state();
    drop(f);
    let original = std::fs::read(&path).unwrap();
    assert!(migrate(&path, 2, true).is_err());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "original untouched"
    );
    let (_, back, _) = DocFile::open::<S>(&path, 2).unwrap();
    assert_eq!(back.state(), state);
    let backup = migrate(&path, 2, false).unwrap();
    assert_eq!(std::fs::read(&backup).unwrap(), original, "copy kept");
    assert!(matches!(
        DocFile::open::<S>(&path, 2),
        Err(FileError::NewerVersion(2))
    ));
    // The header refuses unknown options (the reserved encryption bit).
    let mut bytes = original.clone();
    bytes[12] = 1;
    std::fs::write(&path, &bytes).unwrap();
    assert!(DocFile::open::<S>(&path, 2).is_err());
    let _ = HEADER_LEN;
}

const DAY: u64 = 24 * 3600;
const RETENTION: u64 = 30 * DAY;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn compaction_cleans_only_what_the_rules_allow() {
    let (path, mut f, mut s) = new_file("compact");
    let keep = create(&mut s, "shape");
    let gone = create(&mut s, "image");
    let linked = create(&mut s, "shape");
    let other = create(&mut s, "shape");
    let data = vec![7u8; 9_000_000];
    let h = f.put_blob(&data[..]).unwrap();
    set(&mut s, &gone, "blob", Value::Str(h.clone()));
    s.apply(&Op::Connect {
        id: None,
        edge: bivar_poc_modelo::Edge {
            from: linked.clone(),
            from_port: "out".into(),
            to: other.clone(),
            to_port: "in".into(),
        },
    })
    .unwrap();
    for n in [&gone, &linked] {
        s.apply(&Op::RemoveNode { node: n.clone() }).unwrap();
    }
    s.commit();
    f.save(&mut s, "pc-1").unwrap();
    // Not yet 30 days: nothing to purge, nothing to trim.
    let plan = s.plan_compaction(now(), RETENTION, false);
    assert!(
        plan.purge.is_empty() && plan.trim_before.is_none(),
        "{plan:?}"
    );
    // 31 days later: the image goes; the node with a live connection stays.
    let later = now() + 31 * DAY;
    let plan = s.plan_compaction(later, RETENTION, false);
    // (The workload of the base document removed other nodes too.)
    assert!(plan.purge.contains(&gone), "{plan:?}");
    assert!(!plan.purge.contains(&linked), "{plan:?}");
    assert!(plan.purge.iter().all(|n| s.is_removed(n)));
    assert!(
        plan.trim_before.is_some() && plan.cut_off.is_empty(),
        "{plan:?}"
    );
    let size_before = f.len();
    let f = f.compact(&mut s, &plan).unwrap();
    assert!(f.len() < size_before, "{} < {size_before}", f.len());
    assert!(
        f.read_blob(&h, std::io::sink()).is_err(),
        "the image's pieces are gone"
    );
    assert!(s.parent(&keep).is_some());
    let state = s.state();
    drop(f);
    let (_, back, _) = DocFile::open::<S>(&path, 2).unwrap();
    assert_eq!(back.state(), state);
}

#[test]
fn milestones_and_open_conflicts_are_never_cleaned() {
    let mut a = S::new_document(1);
    let x = create(&mut a, "shape");
    set(&mut a, &x, "color", Value::Str("green".into()));
    let mut b = a.fork_as(2);
    set(&mut a, &x, "color", Value::Str("red".into()));
    set(&mut b, &x, "color", Value::Str("blue".into()));
    a.merge_from(&mut b).unwrap();
    a.apply(&Op::RemoveNode { node: x.clone() }).unwrap();
    a.commit();
    a.record_seen("pc-1", now());
    let plan = a.plan_compaction(now() + 31 * DAY, RETENTION, false);
    assert!(plan.purge.is_empty(), "{plan:?}");
    assert_eq!(plan.kept_for_conflicts, vec![x.clone()]);

    let mut c = S::new_document(3);
    Workload::new(4).run(&mut c, 50);
    c.record_seen("pc-1", now());
    c.add_milestone("Versão enviada ao cliente");
    Workload::new(5).run(&mut c, 50);
    c.record_seen("pc-1", now());
    // The latest checkpoint is after the milestone: history stops there.
    let plan = c.plan_compaction(now() + 31 * DAY, RETENTION, false);
    let trim = plan.trim_before.expect("an older checkpoint qualifies");
    let milestone = c.version_of_milestone("Versão enviada ao cliente");
    assert!(vv_within(&trim, &milestone));
}

#[test]
fn a_copy_offline_longer_than_the_kept_history_is_told_not_merged_silently() {
    let mut a = S::new_document(1);
    Workload::new(4).run(&mut a, 100);
    a.record_seen("pc-1", now());
    let mut laptop = a.fork_as(2);
    laptop.record_seen("laptop", now() - 40 * DAY);
    a.merge_from(&mut laptop).unwrap();
    // The laptop goes offline and keeps working.
    let n = create(&mut laptop, "shape");
    set(
        &mut laptop,
        &n,
        "color",
        Value::Str("from the laptop".into()),
    );
    Workload::new(9).run(&mut a, 100);
    a.record_seen("pc-1", now());
    // Without forgetting the laptop, compaction keeps history for it.
    let plan = a.plan_compaction(now() + 31 * DAY, RETENTION, false);
    assert!(plan.cut_off.is_empty());
    // Forgetting stale copies: the plan says, before doing it, who is cut off.
    let plan = a.plan_compaction(now() + 31 * DAY, RETENTION, true);
    assert_eq!(plan.cut_off, vec!["laptop".to_owned()], "{plan:?}");
    a.compact(&plan).unwrap();
    // The laptop comes back: refused with a clear error, both ways, and its
    // own work is untouched (it can be saved as a separate document).
    assert_eq!(a.merge_from(&mut laptop), Err(OpError::HistoryTrimmed));
    assert_eq!(laptop.merge_from(&mut a), Err(OpError::HistoryTrimmed));
    assert_eq!(
        laptop.param(&n, "color"),
        Some(Value::Str("from the laptop".into()))
    );
}

fn vv_within(a: &[u8], b: &[u8]) -> bool {
    bivar_poc_loja_loro::version_within(a, b)
}

// ---- Kill tests: a child process writes; the parent kills it. ----

/// Runs only when started by a kill test (BIVAR_CHILD set).
#[test]
#[ignore = "child process for the kill tests"]
fn child_process() {
    let Ok(job) = std::env::var("BIVAR_CHILD") else {
        return;
    };
    let path = PathBuf::from(std::env::var("BIVAR_CHILD_PATH").unwrap());
    let (mut f, mut s, _) = DocFile::open::<S>(&path, 7).unwrap();
    // Only the "edits" job adds a node; the others must not change the document.
    let x = if job == "edits" {
        Some(create(&mut s, "shape"))
    } else {
        None
    };
    let mut w = Workload::new(77);
    let mut i = 0u64;
    loop {
        i += 1;
        match job.as_str() {
            "write" => {
                // About what a person editing fast produces: a few edits,
                // autosaved, about 100 times a second.
                w.run(&mut s, 3);
                f.append_changes(&mut s).unwrap();
                f.tick().unwrap();
                std::thread::sleep(Duration::from_millis(10));
            }
            "edits" => {
                let x = x.as_ref().expect("edits job");
                set(&mut s, x, "x", Value::Num(i as f64));
                f.append_changes(&mut s).unwrap();
                if i == 40 {
                    println!("READY {}", x.0);
                }
                if i >= 40 {
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
            "compact" => {
                w.run(&mut s, 20);
                f.save(&mut s, "pc-1").unwrap();
                let plan = s.plan_compaction(now(), 0, false);
                f = f.compact(&mut s, &plan).unwrap();
            }
            "compact-once" => {
                // One compaction; BIVAR_CRASH_AT kills it at a chosen step.
                let plan = s.plan_compaction(now(), 0, false);
                f.compact(&mut s, &plan).unwrap();
                println!("COMPACTED");
                std::thread::sleep(Duration::from_secs(60));
                unreachable!();
            }
            other => panic!("unknown job {other}"),
        }
    }
}

/// What the program does on opening: open; if it offers to recover from the
/// safety copy, accept and open again.
fn open_or_recover(path: &std::path::Path) -> Result<bivar_poc_arquivo::OpenReport, FileError> {
    match DocFile::open::<S>(path, 2) {
        Ok((_, _, r)) => Ok(r),
        Err(FileError::RecoverFromBackup { .. }) => {
            recover_from_backup(path)?;
            DocFile::open::<S>(path, 2).map(|(_, _, r)| r)
        }
        Err(e) => Err(e),
    }
}

fn spawn_with(job: &str, path: &PathBuf, crash_at: Option<&str>) -> Child {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args([
        "--exact",
        "child_process",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ])
    .env("BIVAR_CHILD", job)
    .env("BIVAR_CHILD_PATH", path)
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    if let Some(step) = crash_at {
        c.env("BIVAR_CRASH_AT", step);
    }
    c.spawn().unwrap()
}

fn spawn(job: &str, path: &PathBuf) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_process",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("BIVAR_CHILD", job)
        .env("BIVAR_CHILD_PATH", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn killing_the_program_while_writing_never_breaks_the_file() {
    let (path, f, _) = new_file("kill-write");
    drop(f);
    let mut rng = Rng::new(3);
    for round in 0..kills() {
        let mut child = spawn("write", &path);
        std::thread::sleep(Duration::from_millis(150 + rng.below(600) as u64));
        child.kill().unwrap();
        child.wait().unwrap();
        let (_, _, report) = DocFile::open::<S>(&path, 2)
            .unwrap_or_else(|e| panic!("round {round}: file does not open: {e:?}"));
        assert!(report.damaged_at.is_none(), "round {round}: {report:?}");
    }
}

#[test]
fn edits_since_the_last_save_survive_a_kill() {
    let (path, f, _) = new_file("kill-edits");
    drop(f);
    let mut child = spawn("edits", &path);
    // The test runner prints its own lines first; wait for ours.
    let node = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map_while(Result::ok)
        .find_map(|l| l.split("READY ").nth(1).map(|s| s.trim().to_owned()))
        .map(NodeId)
        .expect("child ready");
    child.kill().unwrap();
    child.wait().unwrap();
    let (_, back, report) = DocFile::open::<S>(&path, 2).unwrap();
    assert_eq!(
        back.param(&node, "x"),
        Some(Value::Num(40.0)),
        "all 40 edits are back"
    );
    assert!(report.edits_after_last_save >= 40, "{report:?}");
}

#[test]
fn a_copy_made_while_writing_always_opens() {
    let (path, f, _) = new_file("copy-open");
    drop(f);
    let mut child = spawn("write", &path);
    std::thread::sleep(Duration::from_millis(300));
    let copy = dir().join("copy-while-writing.bivar");
    let mut rng = Rng::new(5);
    for i in 0..kills() * 2 {
        let _ = std::fs::remove_file(&copy);
        std::fs::copy(&path, &copy).unwrap();
        let (cf, _, report) = DocFile::open::<S>(&copy, 2)
            .unwrap_or_else(|e| panic!("copy {i} does not open: {e:?}"));
        assert!(report.damaged_at.is_none(), "copy {i}: {report:?}");
        drop(cf);
        std::thread::sleep(Duration::from_millis(20 + rng.below(80) as u64));
    }
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn killing_during_compaction_leaves_the_old_file_or_the_new_one() {
    let (path, mut f, mut s) = new_file("kill-compact");
    f.save(&mut s, "pc-1").unwrap();
    drop(f);
    let mut rng = Rng::new(11);
    let started = Instant::now();
    for round in 0..kills() {
        let mut child = spawn("compact", &path);
        std::thread::sleep(Duration::from_millis(100 + rng.below(900) as u64));
        child.kill().unwrap();
        child.wait().unwrap();
        let report = open_or_recover(&path)
            .unwrap_or_else(|e| panic!("round {round}: neither old nor new file: {e:?}"));
        assert!(report.damaged_at.is_none(), "round {round}: {report:?}");
    }
    println!(
        "{} kills during compaction in {:?}",
        kills(),
        started.elapsed()
    );
}

/// The owner's protocol (2026-10-30): the old file becomes a safety copy,
/// the new one takes its place, and the copy is deleted only after the new
/// file opened. The process dies at each step; the document is always back,
/// identical, either directly or by recovering from the safety copy.
#[test]
fn dying_at_each_step_of_the_swap_never_loses_the_document() {
    for (step, expect_recovery) in [
        ("written", false), // only the temporary file was written
        ("backup", true),   // the main file is gone; the safety copy is complete
        ("swapped", false), // the new file is in place; the copy goes on opening
        ("verified", false),
    ] {
        let (path, mut f, mut s) = new_file(&format!("swap-{step}"));
        Workload::new(12).run(&mut s, 300);
        f.save(&mut s, "pc-1").unwrap();
        let expected = s.state();
        drop(f);
        let mut child = spawn_with("compact-once", &path, Some(step));
        let status = child.wait().unwrap();
        assert!(!status.success(), "{step}: the child must have died");
        let first = DocFile::open::<S>(&path, 2);
        let recovered = matches!(first, Err(FileError::RecoverFromBackup { .. }));
        assert_eq!(
            recovered,
            expect_recovery,
            "{step}: {:?}",
            first.as_ref().err()
        );
        drop(first);
        if recovered {
            recover_from_backup(&path).unwrap();
        }
        let (f, back, report) = DocFile::open::<S>(&path, 2).unwrap();
        assert_eq!(back.state(), expected, "{step}: the document is identical");
        assert!(
            report.damaged_at.is_none() && report.backup_available.is_none(),
            "{step}: {report:?}"
        );
        drop(f);
        assert!(
            !backup_path(&path).exists(),
            "{step}: no safety copy left behind"
        );
    }
}

#[test]
fn a_damaged_main_file_with_a_safety_copy_offers_recovery_and_keeps_both() {
    let (path, mut f, mut s) = new_file("damaged-main");
    f.save(&mut s, "pc-1").unwrap();
    let expected = s.state();
    drop(f);
    std::fs::copy(&path, backup_path(&path)).unwrap();
    // The main file's header is damaged (for example by a power cut).
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[3] ^= 0xff;
    std::fs::write(&path, &bytes).unwrap();
    let Err(FileError::RecoverFromBackup { cause, .. }) = DocFile::open::<S>(&path, 2) else {
        panic!("the program must offer recovery");
    };
    assert!(matches!(*cause, FileError::NotABivarFile), "{cause:?}");
    let aside = recover_from_backup(&path)
        .unwrap()
        .expect("damaged file kept aside");
    assert_eq!(std::fs::read(&aside).unwrap(), bytes, "nothing deleted");
    let (_, back, _) = DocFile::open::<S>(&path, 2).unwrap();
    assert_eq!(back.state(), expected);
    let _ = std::fs::remove_file(aside);
}

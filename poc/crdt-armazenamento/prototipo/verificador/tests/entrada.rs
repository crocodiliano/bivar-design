//! External input (ADR-0024): trust, the verifier, and the bombs.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bivar_poc_arquivo::{Access, DocFile, FileError, Limits, TrustStore};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::testkit::Workload;
use bivar_poc_modelo::{NodeId, Op, Store, Value};
use bivar_poc_verificador::{CheckError, How, Refusal, VerifyLimits, open_checked};

type S = EntriesStore;

const VERIFIER: &str = env!("CARGO_BIN_EXE_bivar-poc-verificador");

fn dir() -> PathBuf {
    let d = std::env::var("BIVAR_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("bivar-poc-verificador"));
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

/// A file written by "this computer": saved, then remembered as trusted.
fn saved_file(name: &str, trust: &TrustStore) -> PathBuf {
    let path = fresh(name);
    let mut s = S::new_document(1);
    Workload::new(3).run(&mut s, 300);
    let mut f = DocFile::create(&path, &mut s).unwrap();
    f.save(&mut s, "pc-1").unwrap();
    trust.remember(&path, &f.fingerprint()).unwrap();
    path
}

fn open(path: &Path, trust: &TrustStore) -> Result<How, CheckError> {
    open_with(path, trust, &Limits::default(), &VerifyLimits::default())
}

fn open_with(
    path: &Path,
    trust: &TrustStore,
    l: &Limits,
    v: &VerifyLimits,
) -> Result<How, CheckError> {
    open_checked::<S>(path, 2, trust, Path::new(VERIFIER), l, v).map(|(_, _, _, how)| how)
}

fn refusal(r: Result<How, CheckError>) -> Refusal {
    match r {
        Err(CheckError::Refused(why)) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_file_this_computer_saved_opens_without_the_verifier() {
    let t = trust("own");
    let path = saved_file("own", &t);
    // A verifier that does not exist: if it were called, this would fail.
    let how = open_checked::<S>(
        &path,
        2,
        &t,
        Path::new("no-such-verifier.exe"),
        &Limits::default(),
        &VerifyLimits::default(),
    )
    .map(|(_, _, _, h)| h)
    .unwrap();
    assert_eq!(how, How::Trusted);
}

#[test]
fn a_copy_of_a_trusted_file_from_elsewhere_goes_through_the_verifier() {
    let t = trust("copy");
    let path = saved_file("copy-original", &t);
    let copy = fresh("copy-from-outside");
    std::fs::copy(&path, &copy).unwrap();
    // Same bytes, same fingerprint, but not a file this computer wrote.
    assert_eq!(open(&copy, &t).unwrap(), How::Verified);
}

#[test]
fn a_trusted_file_changed_from_outside_goes_through_the_verifier() {
    let t = trust("changed");
    let path = saved_file("changed", &t);
    // Someone else appends to it (a valid record, correct checksum).
    let (mut f, _, _) = DocFile::open::<S>(&path, 9).unwrap();
    f.append_raw(5, &0u64.to_le_bytes()).unwrap();
    drop(f);
    assert_eq!(open(&path, &t).unwrap(), How::Verified);
}

#[test]
fn trust_lives_only_outside_the_file() {
    let t = trust("outside");
    let path = saved_file("outside", &t);
    // Nothing in the file says "trusted": with an empty list it is verified.
    let empty = trust("empty");
    assert_eq!(open(&path, &empty).unwrap(), How::Verified);
    assert_eq!(open(&path, &t).unwrap(), How::Trusted);
}

#[test]
fn a_file_swapped_after_the_check_is_not_opened() {
    let t = trust("swap");
    let path = saved_file("swap", &t);
    let other = fresh("swap-other");
    let mut s = S::new_document(5);
    let mut f = DocFile::create(&other, &mut s).unwrap();
    f.save(&mut s, "pc-1").unwrap();
    let wrong = f.fingerprint();
    drop(f);
    let r = DocFile::open_with::<S>(&path, 2, &Limits::default(), Access::Write, Some(wrong));
    assert!(matches!(r, Err(FileError::ChangedSinceCheck)));
}

#[test]
fn a_declared_terabyte_record_is_refused_before_anything_is_read() {
    let t = trust("terabyte");
    let path = saved_file("terabyte", &t);
    let mut bytes = std::fs::read(&path).unwrap();
    // A record whose header claims 1 TB.
    bytes.push(2);
    bytes.extend_from_slice(&(1u64 << 40).to_le_bytes());
    bytes.extend_from_slice(&[0u8; 64]);
    let bomb = fresh("terabyte-bomb");
    std::fs::write(&bomb, &bytes).unwrap();
    let started = Instant::now();
    match open(&bomb, &t) {
        Err(CheckError::File(FileError::LimitExceeded { what, limit, found })) => {
            assert_eq!(found, 1 << 40);
            assert_eq!(limit, 256 << 20);
            assert!(what.contains("updates"));
        }
        other => panic!("{other:?}"),
    }
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn too_many_records_are_refused_and_the_limit_can_be_raised_explicitly() {
    let t = trust("records");
    let path = fresh("records");
    let mut s = S::new_document(1);
    let mut f = DocFile::create(&path, &mut s).unwrap();
    let x = s
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "shape".into(),
        })
        .unwrap()
        .created
        .unwrap();
    for i in 0..1_200 {
        s.apply(&Op::SetParam {
            node: x.clone(),
            key: "x".into(),
            value: Some(Value::Num(i as f64)),
        })
        .unwrap();
        s.commit();
        f.append_changes(&mut s).unwrap();
    }
    f.save(&mut s, "pc-1").unwrap();
    drop(f);
    // A lower limit stands for the 50 million (a file that large is slow to
    // build in a test); the rule is the same.
    let low = Limits {
        records: 1_000,
        ..Limits::default()
    };
    let e = open_with(&path, &t, &low, &VerifyLimits::default()).unwrap_err();
    let CheckError::File(e @ FileError::LimitExceeded { .. }) = e else {
        panic!("{e:?}")
    };
    let message = e.to_string();
    assert!(
        message.contains("limit") && message.contains("settings"),
        "{message}"
    );
    // The owner raises the limit, explicitly: the same file opens.
    let raised = Limits {
        records: 10_000,
        ..low
    };
    assert_eq!(
        open_with(&path, &t, &raised, &VerifyLimits::default()).unwrap(),
        How::Verified
    );
}

#[test]
fn document_data_above_the_limit_is_refused_and_large_files_do_not_count() {
    let t = trust("doc-total");
    let path = fresh("doc-total");
    let mut s = S::new_document(1);
    Workload::new(4).run(&mut s, 300); // some tens of kB of document data
    let mut f = DocFile::create(&path, &mut s).unwrap();
    // A 20 MB large file (an image) does not count in the document data.
    let blob: Vec<u8> = (0..20_000_000u32).map(|i| (i % 251) as u8).collect();
    let h = f.put_blob(&blob[..]).unwrap();
    let n = s
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "image".into(),
        })
        .unwrap()
        .created
        .unwrap();
    s.apply(&Op::SetParam {
        node: n,
        key: "blob".into(),
        value: Some(Value::Str(h)),
    })
    .unwrap();
    s.commit();
    f.save(&mut s, "pc-1").unwrap();
    drop(f);
    let small = Limits {
        doc_total: 1 << 20,
        ..Limits::default()
    };
    assert_eq!(
        open_with(&path, &t, &small, &VerifyLimits::default()).unwrap(),
        How::Verified
    );
    // But document data above its own limit is refused.
    let tiny = Limits {
        doc_total: 1_000,
        ..Limits::default()
    };
    let got = open_with(&path, &t, &tiny, &VerifyLimits::default());
    assert!(
        matches!(got, Err(CheckError::File(FileError::LimitExceeded { .. }))),
        "{got:?}"
    );
}

#[test]
fn a_decompression_bomb_is_refused_by_the_memory_cap() {
    let t = trust("lz4");
    let path = fresh("lz4-bomb");
    let mut s = S::new_document(1);
    let n = s
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "text".into(),
        })
        .unwrap()
        .created
        .unwrap();
    // 200 MB of one letter: small once compressed, huge once opened.
    let chunk = "a".repeat(10_000_000);
    for i in 0..20 {
        s.apply(&Op::InsertText {
            node: n.clone(),
            pos: i * chunk.len(),
            text: chunk.clone(),
        })
        .unwrap();
        s.commit();
    }
    let f = DocFile::create(&path, &mut s).unwrap();
    let size = f.len();
    drop(f);
    drop(s);
    assert!(
        size < 20_000_000,
        "the bomb must be small on disk: {size} bytes"
    );
    // A cap of 256 MiB stands for the 4 GiB (the same mechanism).
    let v = VerifyLimits {
        memory: 256 << 20,
        ..VerifyLimits::default()
    };
    let started = Instant::now();
    let why = refusal(open_with(&path, &t, &Limits::default(), &v));
    assert_eq!(why, Refusal::TooMuchMemory, "{}", why.message());
    println!(
        "bomb of {size} bytes on disk refused in {:?}",
        started.elapsed()
    );
    // With the normal 4 GiB the same legitimate (if odd) document opens.
    assert_eq!(open(&path, &t).unwrap(), How::Verified);
}

#[test]
fn a_verifier_that_takes_too_long_is_stopped_and_the_file_refused() {
    let t = trust("slow");
    let path = saved_file("slow", &t);
    let copy = fresh("slow-copy");
    std::fs::copy(&path, &copy).unwrap();
    // The verifier is told (through its own environment only) never to
    // finish. A 3 s limit stands for the 60 s.
    let v = VerifyLimits {
        time: Duration::from_secs(3),
        test_stall: true,
        ..VerifyLimits::default()
    };
    let started = Instant::now();
    let r = open_with(&copy, &t, &Limits::default(), &v);
    assert_eq!(refusal(r), Refusal::TooSlow);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_very_deep_tree_is_opened_or_refused_but_never_takes_the_program_down() {
    let t = trust("deep");
    let path = fresh("deep");
    let mut s = S::new_document(1);
    let mut parent: Option<NodeId> = None;
    let depth: usize = std::env::var("BIVAR_DEEP")
        .ok()
        .and_then(|d| d.parse().ok())
        .unwrap_or(20_000);
    let t0 = Instant::now();
    for _ in 0..depth {
        let n = s
            .apply(&Op::CreateNode {
                parent: parent.clone(),
                index: None,
                kind: "group".into(),
            })
            .unwrap()
            .created
            .unwrap();
        parent = Some(n);
    }
    s.commit();
    let build = t0.elapsed();
    let f = DocFile::create(&path, &mut s).unwrap();
    drop(f);
    let t1 = Instant::now();
    let r = open(&path, &t);
    println!(
        "depth {depth}: {r:?}; build {build:?}; verify and open {:?}",
        t1.elapsed()
    );
    assert!(
        matches!(r, Ok(How::Verified) | Err(CheckError::Refused(_))),
        "{r:?}"
    );
}

#[test]
fn a_large_file_index_pointing_at_missing_or_repeated_pieces_is_harmless() {
    let path = fresh("index");
    let mut s = S::new_document(1);
    let mut f = DocFile::create(&path, &mut s).unwrap();
    let h = f.put_blob(&[7u8; 1000][..]).unwrap();
    // An index that repeats one real piece 262,000 times (1 TB if read) and
    // another that points at a piece that does not exist.
    let real = bivar_poc_arquivo::hex_to_id(&sha_hex(&[7u8; 1000])).unwrap();
    let mut repeated = vec![1u8; 32];
    repeated.extend_from_slice(&(262_000u64 * 1000).to_le_bytes());
    for _ in 0..262_000 {
        repeated.extend_from_slice(&real);
    }
    f.append_raw(4, &repeated).unwrap();
    let mut missing = vec![2u8; 32];
    missing.extend_from_slice(&1000u64.to_le_bytes());
    missing.extend_from_slice(&[9u8; 32]);
    f.append_raw(4, &missing).unwrap();
    drop(f);
    let started = Instant::now();
    let (f, _, report) = DocFile::open::<S>(&path, 2).unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        report.damaged_at.is_some() && !f.is_writable(),
        "{report:?}"
    );
    assert!(
        f.read_blob(&bivar_poc_arquivo::hex(&[2u8; 32]), std::io::sink())
            .is_err()
    );
    assert_eq!(f.read_blob(&h, Vec::new()).unwrap(), 1000);
}

fn sha_hex(data: &[u8]) -> String {
    bivar_poc_arquivo::sha_hex(data)
}

/// What the verification costs, on the 1-million-operation document of the
/// earlier measurements (`resultados/doc-1m-entries.bin`). On demand.
#[test]
#[ignore = "measurement"]
fn verification_cost_on_a_large_document() {
    let snapshot = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resultados/doc-1m-entries.bin"),
    )
    .expect("run the 1M build measurement first");
    let mut s = S::load(&snapshot, 1).unwrap();
    let path = fresh("big");
    let mut f = DocFile::create(&path, &mut s).unwrap();
    f.save(&mut s, "pc-1").unwrap();
    let t = trust("big");
    t.remember(&path, &f.fingerprint()).unwrap();
    drop(f);
    drop(s);
    for (label, list) in [
        ("trusted (this computer saved it)", &t),
        ("from outside", &trust("big-empty")),
    ] {
        let started = Instant::now();
        let how = open(&path, list).unwrap();
        println!(
            "RESULT open {label}: {:?} in {:.2} s",
            how,
            started.elapsed().as_secs_f64()
        );
    }
}

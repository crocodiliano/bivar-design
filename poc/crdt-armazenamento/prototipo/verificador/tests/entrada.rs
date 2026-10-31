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
    trust.remember(&path, &f.fingerprint(), 7).unwrap();
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
    // M2: the file really is swapped between the verification and the
    // opening, the way another program could do it.
    let t = trust("swap");
    let path = saved_file("swap", &t);
    let other = fresh("swap-other");
    let mut s = S::new_document(5);
    let mut f = DocFile::create(&other, &mut s).unwrap();
    f.save(&mut s, "pc-1").unwrap();
    drop(f);
    let checked = bivar_poc_verificador::verify(
        &path,
        Path::new(VERIFIER),
        &Limits::default(),
        &VerifyLimits::default(),
    )
    .unwrap();
    let original = std::fs::read(&path).unwrap();
    std::fs::copy(&other, &path).unwrap(); // swapped
    let r = DocFile::open_with::<S>(&path, 2, &Limits::default(), Access::Write, Some(checked));
    assert!(
        matches!(r, Err(FileError::ChangedSinceCheck)),
        "{:?}",
        r.err()
    );
    // Swapped back, the same check lets it in.
    std::fs::write(&path, &original).unwrap();
    let r = DocFile::open_with::<S>(&path, 2, &Limits::default(), Access::Write, Some(checked));
    assert!(r.is_ok(), "{:?}", r.err());
}

/// A verifier that writes more than a pipe holds (a panic with a full
/// backtrace, for example) is not left waiting for a reader and refused as
/// too slow: the program reads its output while it runs (audit 2026-10-31,
/// suspicion).
#[test]
fn a_verifier_writing_a_lot_is_not_taken_for_a_slow_one() {
    let t = trust("noise");
    let path = saved_file("noise", &t);
    let copy = fresh("noise-copy");
    std::fs::copy(&path, &copy).unwrap();
    let v = VerifyLimits {
        time: Duration::from_secs(20),
        test_noise: Some(4 << 20),
        ..VerifyLimits::default()
    };
    assert_eq!(
        open_with(&copy, &t, &Limits::default(), &v).unwrap(),
        How::Verified
    );
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

/// A chain of `depth` groups, one inside the other; returns them top first.
fn chain(s: &mut S, under: Option<NodeId>, depth: usize) -> Vec<NodeId> {
    let mut parent = under;
    let mut out = Vec::with_capacity(depth);
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
        out.push(n.clone());
        parent = Some(n);
    }
    s.commit();
    out
}

/// The depth limit (10,000 levels; owner, 2026-10-31, I4): a file nested
/// one level deeper is refused with its own message, and one at the limit
/// opens. Building the deep file is what takes time here.
#[test]
fn a_tree_deeper_than_the_limit_is_refused_and_one_at_the_limit_opens() {
    let t = trust("deep");
    for (depth, refused) in [(10_001, true), (10_000, false)] {
        let path = fresh(&format!("deep-{depth}"));
        let mut s = S::new_document(1);
        chain(&mut s, None, depth);
        drop(DocFile::create(&path, &mut s).unwrap());
        let r = open(&path, &t);
        if refused {
            assert_eq!(refusal(r), Refusal::TooDeep);
            assert!(Refusal::TooDeep.message().contains("deeper"));
        } else {
            assert_eq!(r.unwrap(), How::Verified);
        }
    }
}

/// Every operation of the program, not only opening, on a document at the
/// depth limit, run on a thread with a small stack (256 KB, a quarter of
/// Windows' default): nothing that walks the tree may call itself (I4).
#[test]
fn every_operation_runs_on_a_document_at_the_depth_limit_with_a_small_stack() {
    let path = fresh("deep-ops");
    let mut s = S::new_document(1);
    let groups = chain(&mut s, None, bivar_poc_modelo::MAX_DEPTH);
    let f = DocFile::create(&path, &mut s).unwrap();
    drop(f);
    let worker = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || {
            let (mut f, mut s, _) = DocFile::open::<S>(&path, 2).unwrap();
            // The top group hidden: the hidden-layer warning counts 9,999.
            s.apply(&Op::SetParam {
                node: groups[0].clone(),
                key: bivar_poc_modelo::VISIBLE_KEY.into(),
                value: Some(Value::Num(0.0)),
            })
            .unwrap();
            s.commit();
            let hidden = bivar_poc_modelo::hidden_layers(&s.state());
            assert_eq!(hidden[0].inside, bivar_poc_modelo::MAX_DEPTH - 1);
            assert!(bivar_poc_modelo::every_pass(&s, bivar_poc_modelo::MAX_DEPTH).is_ok());
            // Saving, a copy to send (keeping the hidden layers), merging.
            f.save(&mut s, "pc").unwrap();
            let dest = fresh("deep-ops-copy");
            let answers = bivar_poc_arquivo::SendCopyAnswers {
                conflicts_seen: true,
                hidden_layers: Some(bivar_poc_arquivo::HiddenChoice::Keep),
            };
            f.save_send_copy(&s, &dest, answers).unwrap();
            let mut other = s.fork_as(3);
            other
                .apply(&Op::SetParam {
                    node: groups[groups.len() - 1].clone(),
                    key: "name".into(),
                    value: Some(Value::Str("deepest".into())),
                })
                .unwrap();
            s.merge_from(&mut other).unwrap();
            assert_eq!(
                s.param(&groups[groups.len() - 1], "name"),
                Some(Value::Str("deepest".into()))
            );
            // A compaction.
            s.record_seen("pc", 1);
            let plan = s.plan_compaction(40 * 24 * 3600, 30 * 24 * 3600, false);
            f.compact(&mut s, &plan).unwrap();
            assert_eq!(
                bivar_poc_modelo::tree_depth(&s.state()),
                bivar_poc_modelo::MAX_DEPTH
            );
        })
        .unwrap();
    worker
        .join()
        .expect("an operation overflowed a 256 KB stack");
}

/// Two moves, each within the limit, that add up past it on merging: the
/// merge never refuses; it warns, on both copies (owner, 2026-10-31).
#[test]
fn moves_that_add_up_past_the_depth_limit_merge_with_a_warning() {
    let mut a = S::new_document(1);
    let one = chain(&mut a, None, 5_000);
    let two = chain(&mut a, None, 5_000);
    let three = chain(&mut a, None, 4_000);
    let mut b = a.fork_as(2);
    // a: the second chain under the end of the first: 10,000 levels.
    a.apply(&Op::MoveNode {
        node: two[0].clone(),
        parent: Some(one[one.len() - 1].clone()),
        index: 0,
    })
    .unwrap();
    a.commit();
    // b: the first chain under the end of the third: 9,000 levels.
    b.apply(&Op::MoveNode {
        node: one[0].clone(),
        parent: Some(three[three.len() - 1].clone()),
        index: 0,
    })
    .unwrap();
    b.commit();
    let ra = a
        .merge_from(&mut b)
        .expect("a merge never refuses for depth");
    let rb = b
        .merge_from(&mut a)
        .expect("a merge never refuses for depth");
    assert_eq!(a.state(), b.state());
    let depth = bivar_poc_modelo::tree_depth(&a.state());
    assert_eq!(depth, 14_000);
    assert_eq!(ra.too_deep, Some(depth), "{ra:?}");
    assert_eq!(rb.too_deep, Some(depth), "{rb:?}");
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
    t.remember(&path, &f.fingerprint(), 7).unwrap();
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

/// A file whose bomb is where a plain reading of the document never looks
/// (audit 2026-10-31, B1). `hide` puts 200 MB of one letter somewhere and
/// leaves the visible document small. The verifier must refuse it with a
/// 256 MiB cap (standing for the 4 GiB), as it refuses a visible bomb.
fn hidden_bomb(name: &str, hide: impl FnOnce(&mut S)) {
    let t = trust(name);
    let path = fresh(name);
    let mut s = S::new_document(1);
    hide(&mut s);
    let f = DocFile::create(&path, &mut s).unwrap();
    let size = f.len();
    drop(f);
    drop(s);
    assert!(
        size < 20_000_000,
        "the bomb must be small on disk: {size} bytes"
    );
    let v = VerifyLimits {
        memory: 256 << 20,
        ..VerifyLimits::default()
    };
    let why = refusal(open_with(&path, &t, &Limits::default(), &v));
    assert_eq!(why, Refusal::TooMuchMemory, "{}", why.message());
}

fn big_text(s: &mut S) -> NodeId {
    let n = s
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "text".into(),
        })
        .unwrap()
        .created
        .unwrap();
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
    n
}

#[test]
fn a_bomb_in_a_removed_node_is_refused() {
    hidden_bomb("bomb-removed", |s| {
        let n = big_text(s);
        s.apply(&Op::RemoveNode { node: n }).unwrap();
        s.commit();
    });
}

#[test]
fn a_bomb_only_in_the_history_is_refused() {
    hidden_bomb("bomb-history", |s| {
        let n = big_text(s);
        s.apply(&Op::DeleteText {
            node: n,
            pos: 0,
            len: 200_000_000,
        })
        .unwrap();
        s.commit();
    });
}

#[test]
fn a_bomb_in_a_root_map_is_refused() {
    hidden_bomb("bomb-root-map", |s| {
        s.add_milestone(&"a".repeat(200_000_000));
    });
}

/// A file of `n` minimal large-file pieces (4 bytes each, all different)
/// and, with `index`, one index per piece.
fn pieces_file(name: &str, n: u32, index: bool) -> PathBuf {
    let path = fresh(name);
    let mut s = S::new_document(1);
    let snapshot = s.save();
    let mut payloads: Vec<(u8, Vec<u8>)> = Vec::new();
    for i in 0..n {
        let data = i.to_le_bytes();
        let id = bivar_poc_arquivo::hex_to_id(&bivar_poc_arquivo::sha_hex(&data))
            .unwrap()
            .to_vec();
        let mut chunk = id.clone();
        chunk.extend_from_slice(&data);
        payloads.push((3, chunk));
        if index {
            // An index: the large file's id (here the same), size, pieces.
            let mut idx = id.clone();
            idx.extend_from_slice(&4u64.to_le_bytes());
            idx.extend_from_slice(&id);
            payloads.push((4, idx));
        }
    }
    let mut records: Vec<(u8, &[u8])> = vec![(1, &snapshot)];
    records.extend(payloads.iter().map(|(k, p)| (*k, p.as_slice())));
    std::fs::write(&path, bivar_poc_arquivo::build_file_bytes(&records)).unwrap();
    path
}

fn limit_refused(path: &Path, t: &TrustStore, l: &Limits, what: &str) {
    let e = open_with(path, t, l, &VerifyLimits::default()).unwrap_err();
    let CheckError::File(e @ FileError::LimitExceeded { .. }) = e else {
        panic!("{e:?}")
    };
    let message = e.to_string();
    assert!(
        message.contains(what) && message.contains("settings"),
        "{message}"
    );
}

/// I9: too many large-file pieces are refused before anything reaches the
/// document library, and the file opens once the owner raises the limit.
/// A lower limit stands for the 2 million (owner, 2026-10-31).
#[test]
fn too_many_large_file_pieces_are_refused_and_the_limit_can_be_raised() {
    let t = trust("pieces");
    let path = pieces_file("pieces", 150, false);
    let low = Limits {
        pieces: 100,
        ..Limits::default()
    };
    limit_refused(&path, &t, &low, "pieces of large files");
    let raised = Limits { pieces: 200, ..low };
    assert_eq!(
        open_with(&path, &t, &raised, &VerifyLimits::default()).unwrap(),
        How::Verified
    );
}

/// I9: thousands of small indexes, each within its own limit, are refused
/// by the limit on all of them together.
#[test]
fn large_file_indexes_above_their_total_are_refused() {
    let t = trust("indexes");
    let path = pieces_file("indexes", 200, true);
    let low = Limits {
        blob_index_total: 72 * 100, // each index here is 72 bytes
        ..Limits::default()
    };
    limit_refused(&path, &t, &low, "large-file indexes");
    assert_eq!(open(&path, &t).unwrap(), How::Verified);
}

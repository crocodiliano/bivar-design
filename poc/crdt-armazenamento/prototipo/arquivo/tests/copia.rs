//! "Salvar cópia para enviar" and "Salvar como" (ADR-0019, decision 5;
//! phase 0.6, part 6, with the owner's adjustments of 2026-10-30).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bivar_poc_arquivo::{
    DocFile, FileError, HiddenChoice, SendCopyAnswers, discard_partial, sha_hex,
};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::testkit::Workload;
use bivar_poc_modelo::{NodeId, Op, OpError, Store, VISIBLE_KEY, Value};

type S = EntriesStore;

const MARKER: &str = "BIVAR-MARCADOR-c0p1a-3nv14r-UNICO";
/// What the original records about where and how it was made.
const COMPUTER: &str = "PC-DE-FULANO-c0ff33";
const MILESTONE: &str = "Marco-secreto-da-entrega";
const IMAGE_FOLDER: &str = "Pasta-Particular-de-Fulano";
const IMAGE_FILE: &str = "foto-das-ferias-c0ff33.png";

/// Everything the copy goes ahead with: conflicts seen, no hidden layers.
const GO: SendCopyAnswers = SendCopyAnswers {
    conflicts_seen: true,
    hidden_layers: Some(HiddenChoice::Keep),
};

fn dir() -> PathBuf {
    let d = std::env::var("BIVAR_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("bivar-poc-copia"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fresh(name: &str) -> PathBuf {
    let p = dir().join(format!("{name}.bivar"));
    let _ = std::fs::remove_file(&p);
    let _ = discard_partial(&p);
    p
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn create(s: &mut S, parent: Option<NodeId>, kind: &str) -> NodeId {
    s.apply(&Op::CreateNode {
        parent,
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

fn write_text(s: &mut S, n: &NodeId, text: &str) {
    s.apply(&Op::InsertText {
        node: n.clone(),
        pos: 0,
        text: text.into(),
    })
    .unwrap();
    s.commit();
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A file read every way that matters: its bytes as stored, the changes in
/// the library's uncompressed form, the history decoded as text, and the
/// document state. Loro compresses repeated text inside a snapshot, so the
/// stored bytes alone are not enough (`the_search_looks_inside_compressed_data`).
fn views(path: &Path) -> Vec<(&'static str, Vec<u8>)> {
    let raw = std::fs::read(path).unwrap();
    let (_, mut s, _) = DocFile::open::<S>(path, 100).unwrap();
    vec![
        ("stored bytes", raw),
        ("uncompressed changes", s.export_all()),
        ("decoded history", s.history_dump().into_bytes()),
        ("state", format!("{:?}", s.state()).into_bytes()),
    ]
}

/// Which of the views hold `needle`.
fn found_in(views: &[(&'static str, Vec<u8>)], needle: &[u8]) -> Vec<&'static str> {
    views
        .iter()
        .filter(|(_, bytes)| contains(bytes, needle))
        .map(|(label, _)| *label)
        .collect()
}

/// Every way a moment between `from` and `to` (Unix seconds) could be
/// written: decimal, hex text, and the 8 bytes of the file's save mark.
fn dates(from: u64, to: u64) -> Vec<(String, Vec<u8>)> {
    (from..=to)
        .flat_map(|t| {
            [
                (format!("{t}"), t.to_string().into_bytes()),
                (format!("{t:x}"), format!("{t:x}").into_bytes()),
                (format!("{t} as bytes"), t.to_le_bytes().to_vec()),
            ]
        })
        .collect()
}

/// Bytes nobody else would produce: the content of the image that gets
/// deleted, so finding any of it in the copy is meaningful.
fn image(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761).to_le_bytes()[1] ^ seed)
        .collect()
}

struct Original {
    path: PathBuf,
    file: DocFile,
    store: S,
    deleted_image: Vec<u8>,
    deleted_hash: String,
    kept_hash: String,
    /// When the original was written (Unix seconds).
    started: u64,
}

fn original(tag: &str) -> Original {
    let started = unix_now();
    let path = fresh(&format!("original-{tag}"));
    let mut s = S::new_document(1);
    Workload::new(7).run(&mut s, 200);
    let mut f = DocFile::create(&path, &mut s).unwrap();
    let text = create(&mut s, None, "text");
    write_text(&mut s, &text, "Olá, ");
    // The marker, inserted at once (one run, not letter by letter).
    s.apply(&Op::InsertText {
        node: text.clone(),
        pos: 5,
        text: MARKER.into(),
    })
    .unwrap();
    s.commit();
    let deleted_image = image(0x5a, 9_000_000);
    let deleted_hash = f.put_blob(&deleted_image[..]).unwrap();
    let gone = create(&mut s, None, "image");
    set(&mut s, &gone, "blob", Value::Str(deleted_hash.clone()));
    // The image that stays is imported from a file in a personal folder.
    let folder = dir().join(IMAGE_FOLDER);
    std::fs::create_dir_all(&folder).unwrap();
    let image_path = folder.join(IMAGE_FILE);
    std::fs::write(&image_path, image(0x33, 500_000)).unwrap();
    let kept_hash = f
        .put_blob(std::fs::File::open(&image_path).unwrap())
        .unwrap();
    let kept = create(&mut s, None, "image");
    set(&mut s, &kept, "blob", Value::Str(kept_hash.clone()));
    s.add_milestone(MILESTONE);
    f.save(&mut s, COMPUTER).unwrap();
    // Now the secrets are deleted before sending.
    s.apply(&Op::DeleteText {
        node: text.clone(),
        pos: 5,
        len: MARKER.chars().count(),
    })
    .unwrap();
    s.apply(&Op::RemoveNode { node: gone }).unwrap();
    s.commit();
    f.save(&mut s, COMPUTER).unwrap();
    Original {
        path,
        file: f,
        store: s,
        deleted_image,
        deleted_hash,
        kept_hash,
        started,
    }
}

#[test]
fn the_copy_carries_nothing_deleted_and_no_history() {
    let mut o = original("nothing");
    let working = views(&o.path);
    // Controls: the working file does keep them, so the search works.
    assert!(contains(&working[0].1, &o.deleted_image[1_000..1_064]));
    for secret in [MARKER, &o.deleted_hash] {
        assert!(
            !found_in(&working, secret.as_bytes()).is_empty(),
            "control: {secret} in the working file"
        );
    }

    let dest = fresh("copy");
    let report = o
        .file
        .save_send_copy(&o.store, &dest, SendCopyAnswers::default())
        .unwrap();
    assert_eq!(
        report.large_files, 1,
        "only the image still in use goes along"
    );
    let copy_views = views(&dest);

    // Nothing deleted, in any form.
    let piece_id =
        bivar_poc_arquivo::hex_to_id(&sha_hex(&o.deleted_image[..4 * 1024 * 1024])).unwrap();
    let hash_bytes = bivar_poc_arquivo::hex_to_id(&o.deleted_hash).unwrap();
    for (what, needle) in [
        ("the marker", MARKER.as_bytes()),
        ("part of the marker", b"BIVAR-MARCADOR".as_slice()),
        ("the deleted image's hash", o.deleted_hash.as_bytes()),
        ("the deleted image's hash, as bytes", &hash_bytes),
        ("a piece id of the deleted image", &piece_id),
        ("bytes of the deleted image", &o.deleted_image[1_000..1_064]),
    ] {
        assert_eq!(
            found_in(&copy_views, needle),
            Vec::<&str>::new(),
            "{what} is in the copy"
        );
    }
    let (copy_file, mut copy, _) = DocFile::open::<S>(&dest, 100).unwrap();
    assert!(
        copy_file
            .read_blob(&o.deleted_hash, std::io::sink())
            .is_err()
    );
    assert!(copy_file.read_blob(&o.kept_hash, std::io::sink()).is_ok());
    assert!(
        copy.state()
            .nodes
            .values()
            .any(|n| n.text.as_deref() == Some("Olá, ")),
        "the visible text went along"
    );

    // A new history, written by one new author.
    let peers = copy.history_peers();
    assert_eq!(peers.len(), 1, "{peers:?}");
    assert!(
        peers.is_disjoint(&o.store.history_peers()),
        "an author of the original is in the copy: {peers:?}"
    );
    assert_ne!(copy.doc_id(), o.store.doc_id());

    // It never merges with the original.
    assert_eq!(
        copy.merge_from(&mut o.store),
        Err(OpError::DifferentDocument)
    );
    assert_eq!(
        o.store.merge_from(&mut copy),
        Err(OpError::DifferentDocument)
    );
}

/// The owner's privacy list (2026-10-30): beyond what is visible, nothing
/// about who edited, on which computer, from which folder, or when.
#[test]
fn the_copy_says_nothing_about_who_where_or_when() {
    let mut o = original("privacy");
    let working = views(&o.path);
    let dest = fresh("copy-privacy");
    o.file
        .save_send_copy(&o.store, &dest, SendCopyAnswers::default())
        .unwrap();
    let ended = unix_now();
    let copy_views = views(&dest);
    let original_peers: Vec<String> = o.store.history_peers().into_iter().collect();
    let original_id = o.store.doc_id();

    // Recorded in the original (the controls), and not in the copy.
    let mut recorded = vec![
        ("the computer's name", COMPUTER.to_owned()),
        ("the milestone's name", MILESTONE.to_owned()),
        ("the original's identity", original_id.clone()),
    ];
    for p in &original_peers {
        // Entry keys carry their author as 16 hex digits.
        recorded.push(("an author of the original", format!("{p:0>16}")));
    }
    for (what, needle) in &recorded {
        assert!(
            !found_in(&working, needle.as_bytes()).is_empty(),
            "control: {what} ({needle}) is in the original"
        );
        assert_eq!(
            found_in(&copy_views, needle.as_bytes()),
            Vec::<&str>::new(),
            "{what} ({needle}) is in the copy"
        );
    }
    // The dates the original holds (save marks, when each copy was seen,
    // when a node was removed): none of the moments the test ran.
    let moments = dates(o.started.saturating_sub(2), ended + 2);
    assert!(
        moments
            .iter()
            .any(|(_, d)| !found_in(&working, d).is_empty()),
        "control: the original holds dates"
    );
    for (what, needle) in &moments {
        assert_eq!(
            found_in(&copy_views, needle),
            Vec::<&str>::new(),
            "the date {what} is in the copy"
        );
    }
    let (_, mut copy, _) = DocFile::open::<S>(&dest, 100).unwrap();
    assert_eq!(
        copy.history_times(),
        [0].into_iter().collect(),
        "the library recorded the time of a change"
    );

    // Never stored at all, not even in the original: a large file is kept
    // by content (its hash), never by its folder or file name.
    for (what, needle) in [
        ("the image's folder", IMAGE_FOLDER),
        ("the image's file name", IMAGE_FILE),
    ] {
        for (file, v) in [("original", &working), ("copy", &copy_views)] {
            assert_eq!(
                found_in(v, needle.as_bytes()),
                Vec::<&str>::new(),
                "{what} is in the {file}"
            );
        }
    }
}

/// The byte search alone can miss text inside a compressed snapshot (owner's
/// question, 2026-10-30). Proof, and why `views` also decompresses.
#[test]
fn the_search_looks_inside_compressed_data() {
    let mut o = original("compressed");
    let text = create(&mut o.store, None, "text");
    let repeated = MARKER.repeat(50);
    write_text(&mut o.store, &text, &repeated);
    let dest = fresh("copy-compressed");
    o.file
        .save_send_copy(&o.store, &dest, SendCopyAnswers::default())
        .unwrap();
    let found = found_in(&views(&dest), repeated.as_bytes());
    assert!(
        !found.contains(&"stored bytes"),
        "Loro no longer compresses this: the stored bytes alone would do"
    );
    assert_eq!(
        found,
        ["uncompressed changes", "decoded history", "state"],
        "the visible text must be found once decompressed"
    );
}

#[test]
fn open_conflicts_are_announced_before_writing_the_copy() {
    let mut o = original("pending");
    let x = create(&mut o.store, None, "shape");
    set(&mut o.store, &x, "color", Value::Str("green".into()));
    let mut colleague = o.store.fork_as(2);
    set(&mut o.store, &x, "color", Value::Str("red".into()));
    set(&mut colleague, &x, "color", Value::Str("blue".into()));
    o.store.merge_from(&mut colleague).unwrap();
    let pending = o.store.open_conflicts();
    assert_eq!(pending.len(), 1, "{pending:?}");

    let dest = fresh("copy-pending");
    match o
        .file
        .save_send_copy(&o.store, &dest, SendCopyAnswers::default())
    {
        Err(FileError::PendingConflicts(list)) => assert_eq!(list, pending),
        other => panic!("the copy must warn first: {other:?}"),
    }
    assert!(!dest.exists(), "nothing written before the owner confirms");
    // The owner confirms: the copy keeps only the winning value.
    let report = o.file.save_send_copy(&o.store, &dest, GO).unwrap();
    assert_eq!(report.conflicts_left_behind, pending);
    let (_, copy, _) = DocFile::open::<S>(&dest, 100).unwrap();
    assert!(copy.open_conflicts().is_empty());
}

/// Hidden layers go along only if the owner says so (2026-10-30): the copy
/// asks first, lists them, and can leave them out.
#[test]
fn hidden_layers_are_listed_and_can_be_left_out() {
    let mut o = original("hidden");
    let secret = "TEXTO-DA-CAMADA-OCULTA-c0ff33";
    let group = create(&mut o.store, None, "group");
    set(&mut o.store, &group, "name", Value::Str("Rascunho".into()));
    set(&mut o.store, &group, VISIBLE_KEY, Value::Num(0.0));
    let inside = create(&mut o.store, Some(group.clone()), "text");
    write_text(&mut o.store, &inside, secret);
    let shown = create(&mut o.store, None, "group");
    set(&mut o.store, &shown, VISIBLE_KEY, Value::Num(1.0));
    let nested = create(&mut o.store, Some(shown.clone()), "shape");
    set(&mut o.store, &nested, VISIBLE_KEY, Value::Num(0.0));

    // Asked first: nothing written, both hidden layers listed.
    let dest = fresh("copy-hidden");
    let list = match o
        .file
        .save_send_copy(&o.store, &dest, SendCopyAnswers::default())
    {
        Err(e @ FileError::HiddenLayers(_)) => {
            assert!(e.to_string().contains("contains 2 hidden layers"), "{e}");
            let FileError::HiddenLayers(list) = e else {
                unreachable!()
            };
            list
        }
        other => panic!("the copy must ask about hidden layers: {other:?}"),
    };
    assert!(!dest.exists());
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].node, group);
    assert_eq!(list[0].name.as_deref(), Some("Rascunho"));
    assert_eq!(list[0].inside, 1, "the text inside goes or stays with it");
    assert_eq!((list[1].node.clone(), list[1].inside), (nested.clone(), 0));

    // Kept: they go along, still hidden.
    let keep = SendCopyAnswers {
        conflicts_seen: false,
        hidden_layers: Some(HiddenChoice::Keep),
    };
    let report = o.file.save_send_copy(&o.store, &dest, keep).unwrap();
    assert_eq!(report.hidden_layers_left_out, 0);
    let kept = views(&dest);
    assert!(found_in(&kept, secret.as_bytes()).contains(&"state"));
    let (_, copy, _) = DocFile::open::<S>(&dest, 100).unwrap();
    assert_eq!(bivar_poc_modelo::hidden_layers(&copy.state()).len(), 2);

    // Left out: gone with what is inside them, in every form.
    let dest = fresh("copy-hidden-out");
    let out = SendCopyAnswers {
        conflicts_seen: false,
        hidden_layers: Some(HiddenChoice::Remove),
    };
    let report = o.file.save_send_copy(&o.store, &dest, out).unwrap();
    assert_eq!(report.hidden_layers_left_out, 2);
    assert_eq!(
        found_in(&views(&dest), secret.as_bytes()),
        Vec::<&str>::new()
    );
    let (_, copy, _) = DocFile::open::<S>(&dest, 100).unwrap();
    let state = copy.state();
    assert!(bivar_poc_modelo::hidden_layers(&state).is_empty());
    assert_eq!(
        state.nodes.len(),
        o.store.state().nodes.len() - 3,
        "only the two hidden layers and the text inside one left out"
    );
}

/// A half-written copy from a crash is reported, never deleted without
/// asking (owner's rule, 2026-10-30).
#[test]
fn a_leftover_partial_copy_is_reported_and_kept() {
    let mut o = original("partial");
    let dest = fresh("copy-partial");
    let mut partial = dest.as_os_str().to_owned();
    partial.push(".parcial");
    let partial = PathBuf::from(partial);
    let half = b"BIVARDOC half of a copy".to_vec();
    std::fs::write(&partial, &half).unwrap();

    for result in [
        o.file.save_send_copy(&o.store, &dest, GO).map(drop),
        o.file.save_as(&mut o.store, &dest).map(drop),
    ] {
        match result {
            Err(e @ FileError::PartialLeftover(_)) => {
                assert!(e.to_string().contains("was not deleted"), "{e}");
                let FileError::PartialLeftover(p) = e else {
                    unreachable!()
                };
                assert_eq!(p, partial);
            }
            other => panic!("the leftover must be reported: {other:?}"),
        }
        assert_eq!(std::fs::read(&partial).unwrap(), half, "left untouched");
        assert!(!dest.exists());
    }

    // Opening a document with a leftover next to it tells about it too.
    let mut s = S::new_document(5);
    drop(DocFile::create(&dest, &mut s).unwrap());
    let (_, _, report) = DocFile::open::<S>(&dest, 5).unwrap();
    assert_eq!(report.partial_left.as_deref(), Some(partial.as_path()));
    assert_eq!(std::fs::read(&partial).unwrap(), half, "still untouched");

    // The owner says to delete it: then the copy goes ahead.
    std::fs::remove_file(&dest).unwrap();
    discard_partial(&dest).unwrap();
    o.file.save_send_copy(&o.store, &dest, GO).unwrap();
    assert!(!partial.exists());
    assert!(DocFile::open::<S>(&dest, 100).is_ok());
    assert!(
        matches!(
            o.file.save_send_copy(&o.store, &dest, GO),
            Err(FileError::AlreadyExists)
        ),
        "never overwrites"
    );
}

#[test]
fn save_as_keeps_the_history_and_suggests_the_copy_for_sending() {
    let mut o = original("save-as");
    let dest = fresh("save-as");
    let report = o.file.save_as(&mut o.store, &dest).unwrap();
    assert!(report.suggest_send_copy);
    // Why the hint matters: "Salvar como" carries everything, deleted
    // content included, in its history.
    let (_, mut same, _) = DocFile::open::<S>(&dest, 100).unwrap();
    assert!(same.history_dump().contains(MARKER));
    assert_eq!(
        same.doc_id(),
        o.store.doc_id(),
        "the same document: it can merge back"
    );
    assert!(
        matches!(
            o.file.save_as(&mut o.store, &dest),
            Err(FileError::AlreadyExists)
        ),
        "never overwrites"
    );
}

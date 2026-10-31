//! The collection of `.bivar` files kept forever (owner, 2026-10-31; audit
//! I8 and DeepSeek 2.5): files written by this version, in `tests/formato/`
//! at the root of the repository, each with the result it must open to.
//! Every future version of Loro and of the Bivar must open every one of them
//! with the same result. Files are only ever added: a check refuses any
//! change or deletion of a file already there (`.githooks/commit-msg` and
//! CI). A new format version comes in as new files; the old ones stay.
//!
//! `write_collection` (ignored) writes the files of this version, only those
//! not there yet. The test reads all of them.

use std::path::{Path, PathBuf};

use bivar_poc_arquivo::{Access, DocFile, Limits};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::testkit::{Workload, sync};
use bivar_poc_modelo::{Edge, NodeId, Op, Store, Value};

type S = EntriesStore;

fn collection() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../tests/formato")
}

/// What a file opens to, as text: everything a reader must get right.
fn describe(path: &Path) -> String {
    let (f, s, report) =
        DocFile::open_with::<S>(path, 9, &Limits::default(), Access::ReadOnly, None).unwrap();
    let st = s.state();
    let mut out = format!(
        "doc_id: {}\nnodes: {}\nroots: {:?}\nedges: {:?}\nconflicts: {:?}\n",
        st.doc_id,
        st.nodes.len(),
        st.roots,
        st.edges,
        s.open_conflicts()
    );
    for (id, n) in &st.nodes {
        out.push_str(&format!(
            "node {id}: kind={} parent={:?} children={:?} params={:?} text={:?}\n",
            n.kind, n.parent, n.children, n.params, n.text
        ));
    }
    for h in s.blob_hashes() {
        let mut data = Vec::new();
        let size = f.read_blob(&h, &mut data).map_err(|e| format!("{e:?}"));
        out.push_str(&format!("large file {h}: {size:?}\n"));
    }
    out.push_str(&format!(
        "edits after the last save: {}\ndamaged: {:?}\n",
        report.edits_after_last_save, report.damaged_at
    ));
    out
}

#[test]
fn every_file_of_the_collection_opens_to_its_recorded_result() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(collection())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bivar"))
        .collect();
    files.sort();
    assert!(files.len() >= 5, "the collection is missing: {files:?}");
    for path in files {
        // Opened from a copy: the collection is never written, not even by
        // opening (which may drop a cut tail).
        let tmp = std::env::temp_dir().join("bivar-poc-formato");
        std::fs::create_dir_all(&tmp).unwrap();
        let copy = tmp.join(path.file_name().unwrap());
        std::fs::copy(&path, &copy).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("esperado.txt"))
            .unwrap_or_else(|_| panic!("no recorded result for {}", path.display()))
            .replace("\r\n", "\n");
        assert_eq!(describe(&copy), expected, "{}", path.display());
    }
}

fn node(s: &mut S, parent: Option<NodeId>, kind: &str) -> NodeId {
    let n = s
        .apply(&Op::CreateNode {
            parent,
            index: None,
            kind: kind.into(),
        })
        .unwrap()
        .created
        .unwrap();
    s.commit();
    n
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

/// Writes `name` (and its recorded result) if it is not there yet.
fn keep(name: &str, write: impl FnOnce(&Path)) {
    let path = collection().join(format!("{name}.bivar"));
    if path.exists() {
        return; // never rewritten
    }
    let tmp = std::env::temp_dir().join("bivar-poc-formato-new");
    std::fs::create_dir_all(&tmp).unwrap();
    let work = tmp.join(format!("{name}.bivar"));
    let _ = std::fs::remove_file(&work);
    write(&work);
    std::fs::copy(&work, &path).unwrap();
    std::fs::write(path.with_extension("esperado.txt"), describe(&work)).unwrap();
}

/// Format 1, layout 3, Loro 1.17.1 (phase 0.6, 2026-10-31).
#[test]
#[ignore = "writes the collection; run once per format or layout version"]
fn write_collection() {
    std::fs::create_dir_all(collection()).unwrap();
    // Parameters, text, nesting and connections, saved once.
    keep("v1-camada3-basico", |p| {
        let mut s = S::new_document(1);
        let g = node(&mut s, None, "group");
        let a = node(&mut s, Some(g.clone()), "shape");
        let b = node(&mut s, Some(g), "shape");
        let t = node(&mut s, None, "text");
        set(&mut s, &a, "x", Value::Num(1.5));
        set(
            &mut s,
            &a,
            "name",
            Value::Str("Açaí à brasileira — ótimo".into()),
        );
        s.apply(&Op::InsertText {
            node: t,
            pos: 0,
            text: "Olá, mundo".into(),
        })
        .unwrap();
        s.apply(&Op::Connect {
            id: None,
            edge: Edge {
                from: a,
                from_port: "out".into(),
                to: b,
                to_port: "in".into(),
            },
        })
        .unwrap();
        s.commit();
        let mut f = DocFile::create(p, &mut s).unwrap();
        f.save(&mut s, "pc").unwrap();
    });
    // Edits appended after the last save, as autosave leaves them.
    keep("v1-camada3-edicoes-sem-salvar", |p| {
        let mut s = S::new_document(1);
        Workload::new(21).run(&mut s, 300);
        let mut f = DocFile::create(p, &mut s).unwrap();
        f.save(&mut s, "pc").unwrap();
        Workload::new(22).run(&mut s, 50);
        f.append_changes(&mut s).unwrap();
        f.sync().unwrap();
    });
    // Two copies that wrote the same parameter: an open conflict.
    keep("v1-camada3-conflito", |p| {
        let mut a = S::new_document(1);
        let n = node(&mut a, None, "shape");
        let mut b = a.fork_as(2);
        set(&mut a, &n, "color", Value::Str("red".into()));
        set(&mut b, &n, "color", Value::Str("blue".into()));
        sync(&mut a, &mut b);
        let mut f = DocFile::create(p, &mut a).unwrap();
        f.save(&mut a, "pc").unwrap();
    });
    // A history cut by re-rooting, with a node removed.
    keep("v1-camada3-compactado", |p| {
        let mut a = S::new_document(1);
        Workload::new(23).run(&mut a, 300);
        let mut b = a.fork_as(2);
        Workload::new(24).run(&mut b, 0);
        let gone = node(&mut a, None, "shape");
        a.apply(&Op::RemoveNode { node: gone }).unwrap();
        a.commit();
        sync(&mut a, &mut b);
        for at in [1, 2] {
            a.record_seen("pc", at);
            b.merge_from(&mut a).unwrap();
            b.record_seen("laptop", at);
            a.merge_from(&mut b).unwrap();
        }
        let mut f = DocFile::create(p, &mut a).unwrap();
        let plan = a.plan_compaction(40 * 24 * 3600, 30 * 24 * 3600, false);
        assert!(plan.trim_before.is_some(), "{plan:?}");
        f.compact(&mut a, &plan).unwrap();
    });
    // A large file in two pieces, and a milestone.
    keep("v1-camada3-arquivo-grande", |p| {
        let mut s = S::new_document(1);
        let mut f = DocFile::create(p, &mut s).unwrap();
        let data: Vec<u8> = (0..(bivar_poc_arquivo::CHUNK_SIZE + 1000))
            .map(|i| (i as u32).wrapping_mul(2_654_435_761).to_le_bytes()[2])
            .collect();
        let h = f.put_blob(&data[..]).unwrap();
        let img = node(&mut s, None, "image");
        set(&mut s, &img, "blob", Value::Str(h));
        s.add_milestone("Primeira versão");
        f.save(&mut s, "pc").unwrap();
    });
}

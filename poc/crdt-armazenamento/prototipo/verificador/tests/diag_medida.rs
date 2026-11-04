//! TEMPORARY (branch diag/fuzz-fork, never merged): the largest single
//! memory request of the verifier, on legitimate documents and on the bombs,
//! and what the proposed "absurd request" rule would say about each.
//!
//! Measured from outside, without changing the verifier: the verifier runs
//! under `strace`, with glibc told to serve every request of 128 KiB or more
//! with its own `mmap` (and to grow it with `mremap`), so each large request
//! shows up as one system call of its size, rounded up to the page (at most
//! 4 KiB + 16 bytes more than asked). A request the memory cap refuses never
//! reaches glibc: its size comes from the verifier's own error text.
//!
//! Rule under test (owner, 2026-11-04): a refusal by memory is "absurd" when
//! the refused single request is above the memory limit AND above 256 times
//! the document data (snapshot and updates records; large files not counted).
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

use bivar_poc_arquivo::{DocFile, Limits};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::testkit::Workload;
use bivar_poc_modelo::{NodeId, Op, Store};

type S = EntriesStore;

const VERIFIER: &str = env!("CARGO_BIN_EXE_bivar-poc-verificador");
const MIB: u64 = 1 << 20;

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join("bivar-diag-medida");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// (file bytes, document data bytes, large-file bytes), read from the
/// records: kind (1), length (8), payload, digest (32), after the header.
fn sizes(path: &Path) -> (u64, u64, u64) {
    let b = std::fs::read(path).unwrap();
    let (mut at, mut data, mut large) = (bivar_poc_arquivo::HEADER_LEN, 0u64, 0u64);
    while at + 9 <= b.len() {
        let kind = b[at];
        let len = u64::from_le_bytes(b[at + 1..at + 9].try_into().unwrap());
        match kind {
            1 | 2 => data += len,
            3 | 4 => large += len,
            _ => {}
        }
        at += 9 + len as usize + 32;
    }
    assert_eq!(at, b.len(), "record walk must end at the end of the file");
    (b.len() as u64, data, large)
}

struct Run {
    answer: String,
    refused_request: Option<u64>,
    largest_mmap: u64,
    large_calls: usize,
}

fn run_verifier(path: &Path, memory: u64) -> Run {
    let trace = dir().join("trace.txt");
    let _ = std::fs::remove_file(&trace);
    let l = Limits::default();
    let out = Command::new("strace")
        .args(["-f", "--seccomp-bpf", "-e", "trace=mmap,mremap", "-o"])
        .arg(&trace)
        .arg(VERIFIER)
        .arg(path)
        .arg(memory.to_string())
        .args(
            [
                l.doc_record,
                l.doc_total,
                l.records,
                l.blob_index,
                l.blob_index_total,
                l.depth,
                l.pieces,
            ]
            .map(|v| v.to_string()),
        )
        .env("GLIBC_TUNABLES", "glibc.malloc.mmap_threshold=131072")
        .output()
        .expect("strace must be installed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let answer = stdout
        .lines()
        .last()
        .map(|l| l.chars().take(60).collect())
        .unwrap_or_else(|| format!("no answer ({})", out.status));
    let refused_request = stderr.lines().find_map(|l| {
        l.split("memory allocation of ")
            .nth(1)?
            .split(' ')
            .next()?
            .parse()
            .ok()
    });
    let text = std::fs::read_to_string(&trace).unwrap();
    let (mut largest, mut calls) = (0u64, 0usize);
    for line in text.lines() {
        let size = if let Some(args) = line.split("mremap(").nth(1) {
            args.split(", ").nth(2).and_then(|s| s.parse::<u64>().ok())
        } else if let Some(args) = line.split("mmap(").nth(1) {
            let anonymous = args.contains("MAP_ANONYMOUS")
                && !args.contains("PROT_NONE")
                && !args.contains("MAP_STACK")
                && !args.contains("MAP_NORESERVE");
            if anonymous {
                args.split(", ").nth(1).and_then(|s| s.parse::<u64>().ok())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(s) = size {
            calls += 1;
            largest = largest.max(s);
        }
    }
    Run {
        answer,
        refused_request,
        largest_mmap: largest,
        large_calls: calls,
    }
}

fn report(case: &str, path: &Path, caps: &[u64]) {
    let (file, data, large) = sizes(path);
    for &cap in caps {
        let r = run_verifier(path, cap);
        let ratio = r.largest_mmap as f64 / data.max(1) as f64;
        let rule = match r.refused_request {
            Some(n) => {
                if n > cap && n > 256 * data {
                    "ABSURD"
                } else {
                    "normal"
                }
            }
            None => "-",
        };
        println!(
            "RESULT case={case} cap_mib={} file_bytes={file} doc_data_bytes={data} \
             large_file_bytes={large} largest_single_mmap_bytes={} largest_over_data={ratio:.2} \
             large_calls={} refused_request_bytes={} refused_over_data={} rule={rule} answer=[{}]",
            cap / MIB,
            r.largest_mmap,
            r.large_calls,
            r.refused_request.map_or("-".into(), |n| n.to_string()),
            r.refused_request.map_or("-".into(), |n| format!(
                "{:.1}",
                n as f64 / data.max(1) as f64
            )),
            r.answer
        );
    }
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

fn bomb(name: &str, hide: impl FnOnce(&mut S)) -> PathBuf {
    let path = dir().join(format!("{name}.bivar"));
    let _ = std::fs::remove_file(&path);
    let mut s = S::new_document(1);
    hide(&mut s);
    drop(DocFile::create(&path, &mut s).unwrap());
    path
}

#[test]
#[ignore = "temporary measurement, run on demand"]
fn largest_single_request() {
    // Bombs: the same four as tests/entrada.rs. The test cap is 256 MiB;
    // also run at the normal 4 GiB, where they open.
    let bombs = [
        bomb("bomb-visible", |s| {
            big_text(s);
        }),
        bomb("bomb-removed", |s| {
            let n = big_text(s);
            s.apply(&Op::RemoveNode { node: n }).unwrap();
            s.commit();
        }),
        bomb("bomb-history", |s| {
            let n = big_text(s);
            s.apply(&Op::DeleteText {
                node: n,
                pos: 0,
                len: 200_000_000,
            })
            .unwrap();
            s.commit();
        }),
        bomb("bomb-root-map", |s| {
            s.add_milestone(&"a".repeat(200_000_000));
        }),
    ];
    for p in &bombs {
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        report(&name, p, &[256 * MIB, 4096 * MIB]);
    }
    // The fuzzing input, as a whole file (the same cutting as fuzz_records).
    let data = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../fuzz/diag/crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61"),
    )
    .unwrap();
    let mut records: Vec<(u8, &[u8])> = Vec::new();
    let mut rest = &data[..];
    while rest.len() >= 3 && records.len() < 64 {
        let kind = rest[0];
        let len = usize::from(u16::from_le_bytes([rest[1], rest[2]])).min(rest.len() - 3);
        records.push((kind, &rest[3..3 + len]));
        rest = &rest[3 + len..];
    }
    let crash = dir().join("fuzz-crash.bivar");
    std::fs::write(&crash, bivar_poc_arquivo::build_file_bytes(&records)).unwrap();
    report("fuzz-crash", &crash, &[4096 * MIB]);
    // Legitimate documents: the sizes of the battery, at the normal cap
    // and at lower caps (a legitimate document refused by memory must not
    // be called absurd).
    for ops in [250_000usize, 500_000, 1_000_000, 2_000_000] {
        let path = dir().join(format!("legit-{ops}.bivar"));
        let _ = std::fs::remove_file(&path);
        let mut s = S::new_document(1);
        Workload::new(ops as u64).run(&mut s, ops);
        drop(DocFile::create(&path, &mut s).unwrap());
        drop(s);
        report(
            &format!("legit-{ops}"),
            &path,
            &[
                4096 * MIB,
                1024 * MIB,
                512 * MIB,
                256 * MIB,
                128 * MIB,
                64 * MIB,
            ],
        );
        let _ = std::fs::remove_file(&path);
    }
}

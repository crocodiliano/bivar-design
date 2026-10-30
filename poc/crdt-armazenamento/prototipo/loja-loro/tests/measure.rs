//! Measurements comparing the two ways of storing parameters (owner's
//! request, 2026-10-29). On demand, one test per process, on the reference
//! machine:
//! `cargo test --release -p bivar-poc-loja-loro --test measure <name> -- --ignored --nocapture --test-threads=1`
//! Generate the 1M documents (`build_1m_*`) before opening them (`open_1m_*`).

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use bivar_poc_loja_loro::{Entries, LoroStore, Register};
use bivar_poc_modelo::Store;
use bivar_poc_modelo::testkit::Workload;

/// Peak memory of this process so far (Windows or Linux), no `unsafe`.
fn peak_memory() -> u64 {
    if cfg!(windows) {
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {}).PeakWorkingSet64", std::process::id()),
            ])
            .output()
            .expect("powershell runs");
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    } else {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        status
            .lines()
            .find_map(|l| l.strip_prefix("VmHWM:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map_or(0, |kb| kb * 1024)
    }
}

fn file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resultados")
        .join(name)
}

/// 100,000 operations of the approved mix, one commit each (limit: 5 s).
fn ops<S: Store>() {
    let mut times = Vec::new();
    for run in 0..3 {
        let mut d = S::new_document(1);
        let t = Instant::now();
        Workload::new(100 + run).run(&mut d, 100_000);
        times.push(t.elapsed().as_secs_f64());
    }
    times.sort_by(f64::total_cmp);
    println!(
        "RESULT {} ops_100k_median_seconds={:.3} all={times:?}",
        S::NAME,
        times[1]
    );
}

fn merge<S: Store>() {
    let mut times = Vec::new();
    for run in 0..3 {
        let mut a = S::new_document(1);
        let mut w = Workload::new(7 + run);
        w.run(&mut a, 2_000);
        let mut b = a.fork_as(2);
        Workload::with_mirror(71, w.mirror.clone()).run(&mut a, 10_000);
        Workload::with_mirror(72, w.mirror.clone()).run(&mut b, 10_000);
        let t = Instant::now();
        a.merge_from(&mut b).unwrap();
        b.merge_from(&mut a).unwrap();
        times.push(t.elapsed().as_secs_f64());
        assert_eq!(a.state(), b.state());
        if run == 0 {
            println!("RESULT {} merged_file_bytes={}", S::NAME, a.save().len());
        }
    }
    times.sort_by(f64::total_cmp);
    println!(
        "RESULT {} merge_10k_plus_10k_median_seconds={:.3} all={times:?}",
        S::NAME,
        times[1]
    );
}

fn build<S: Store>(name: &str) {
    let mut d = S::new_document(1);
    let t = Instant::now();
    Workload::new(1_000_000).run(&mut d, 1_000_000);
    let gen_s = t.elapsed().as_secs_f64();
    let bytes = d.save();
    std::fs::write(file(name), &bytes).unwrap();
    println!(
        "RESULT {} build_1m_seconds={gen_s:.1} file_bytes={}",
        S::NAME,
        bytes.len()
    );
}

fn open<S: Store>(name: &str) {
    let bytes = std::fs::read(file(name)).unwrap();
    let before = peak_memory();
    let t = Instant::now();
    let d = S::load(&bytes, 9).unwrap();
    let load_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let nodes = d.state().nodes.len();
    let read_s = t.elapsed().as_secs_f64();
    let extra = peak_memory().saturating_sub(before);
    println!(
        "RESULT {} open_1m_load_seconds={load_s:.3} first_full_read_seconds={read_s:.2} total={:.2} extra_memory_mb={:.0} visible_nodes={nodes}",
        S::NAME,
        load_s + read_s,
        extra as f64 / 1_048_576.0
    );
}

#[test]
#[ignore = "measurement"]
fn ops_100k_entries() {
    ops::<LoroStore<Entries>>();
}

#[test]
#[ignore = "measurement"]
fn merge_register() {
    merge::<LoroStore<Register>>();
}

#[test]
#[ignore = "measurement"]
fn merge_entries() {
    merge::<LoroStore<Entries>>();
}

#[test]
#[ignore = "measurement"]
fn build_1m_register() {
    build::<LoroStore<Register>>("doc-1m-register.bin");
}

#[test]
#[ignore = "measurement"]
fn build_1m_entries() {
    build::<LoroStore<Entries>>("doc-1m-entries.bin");
}

#[test]
#[ignore = "measurement"]
fn open_1m_register() {
    open::<LoroStore<Register>>("doc-1m-register.bin");
}

#[test]
#[ignore = "measurement"]
fn open_1m_entries() {
    open::<LoroStore<Entries>>("doc-1m-entries.bin");
}

/// The owner's request (2026-10-29): reading one node should not walk up to
/// the root. 10,000 groups, one inside the other; read a parameter of the
/// deepest one 10,000 times, with and without the hidden-node index.
#[test]
#[ignore = "measurement"]
fn hidden_index_deep() {
    use bivar_poc_modelo::{Op, Value};
    let mut s = LoroStore::<Entries>::new_document(1);
    let mut parent = None;
    for _ in 0..10_000 {
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
    let deepest = parent.unwrap();
    s.apply(&Op::SetParam {
        node: deepest.clone(),
        key: "x".into(),
        value: Some(Value::Num(1.0)),
    })
    .unwrap();
    s.commit();
    // Per-read time. Without the index one read walks 10,000 levels, so
    // only 100 reads are timed (10,000 took over an hour).
    let mut result = Vec::new();
    for (on, reads) in [(false, 100u32), (true, 10_000)] {
        s.set_hidden_index(on);
        let t = Instant::now();
        for _ in 0..reads {
            assert!(s.param(&deepest, "x").is_some());
        }
        result.push(t.elapsed().as_secs_f64() / f64::from(reads));
    }
    // Rebuilding the index (after opening or merging) on this document.
    let t = Instant::now();
    s.set_hidden_index(true);
    let rebuild_ms = t.elapsed().as_secs_f64() * 1000.0;
    // A normal document: 100,000 operations of the approved mix.
    let mut n = LoroStore::<Entries>::new_document(2);
    Workload::new(1).run(&mut n, 100_000);
    let t = Instant::now();
    n.set_hidden_index(true);
    let rebuild_normal_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "RESULT hidden_index depth=10000 per_read_without_index_ms={:.1} per_read_with_index_us={:.1} rebuild_deep_ms={rebuild_ms:.1} rebuild_100k_ops_doc_ms={rebuild_normal_ms:.1}",
        result[0] * 1000.0,
        result[1] * 1_000_000.0
    );
}

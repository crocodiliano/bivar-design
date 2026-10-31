//! Does every parameter write leave a permanent dead key in the node's Loro
//! map? (Audit 2026-10-31, item 3; Gemini 1.1, DeepSeek 1.2.) On demand:
//! `cargo test --release -p bivar-poc-loja-loro --test growth growth_all -- --ignored --nocapture --test-threads=1`
//!
//! Three scenarios, in both parameter modes (entries, the chosen one, and
//! register, as the baseline without entry keys):
//! - one parameter written 100,000 times, half on each of two copies, merged
//!   once at the end;
//! - the same, merged both ways every 100 writes (real collaboration);
//! - 1,000 parameters written 100 times each, merged every 100 writes.
//!
//! For each: file size, opening time and opening memory, before and after a
//! compaction that drops all history both copies have seen. Opening runs in
//! a child process so its peak memory is its own.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use bivar_poc_loja_loro::{Entries, LoroStore, ParamMode, Register};
use bivar_poc_modelo::{NodeId, Op, Store, Value};

const WRITES: usize = 100_000;

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
        .join(format!("crescimento-{name}"))
}

fn set(s: &mut impl Store, node: &NodeId, key: &str, v: f64) {
    s.apply(&Op::SetParam {
        node: node.clone(),
        key: key.into(),
        value: Some(Value::Num(v)),
    })
    .unwrap();
    s.commit();
}

/// Writes `WRITES` times over `params` parameters, alternating copies in
/// blocks of `block` writes; merges both ways after every pair of blocks
/// (`merge_each`), or only at the end.
fn scenario<M: ParamMode>(name: &str, params: usize, block: usize, merge_each: bool) {
    let mut a = LoroStore::<M>::new_document(1);
    let node = a
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "shape".into(),
        })
        .unwrap()
        .created
        .unwrap();
    a.commit();
    let mut b = a.fork_as(2);
    let keys: Vec<String> = (0..params).map(|i| format!("k{i}")).collect();

    let t = Instant::now();
    let mut merge_s = 0.0;
    let mut last_merge_s = 0.0;
    let mut written = 0;
    while written < WRITES {
        for copy in [&mut a, &mut b] {
            for _ in 0..block {
                set(copy, &node, &keys[written % params], written as f64);
                written += 1;
            }
        }
        if merge_each {
            let m = Instant::now();
            a.merge_from(&mut b).unwrap();
            b.merge_from(&mut a).unwrap();
            last_merge_s = m.elapsed().as_secs_f64();
            merge_s += last_merge_s;
        }
    }
    a.merge_from(&mut b).unwrap();
    b.merge_from(&mut a).unwrap();
    assert_eq!(a.state(), b.state());
    let build_s = t.elapsed().as_secs_f64();

    let before = a.save();
    std::fs::write(file(&format!("{name}-antes.bin")), &before).unwrap();

    // Each copy confirms what it has after receiving the other's
    // confirmation; the compaction cuts at the latest such point that comes
    // before everything kept (re-rooting, ADR-0023).
    for t in [1, 2] {
        a.record_seen("a", t);
        b.merge_from(&mut a).unwrap();
        b.record_seen("b", t);
        a.merge_from(&mut b).unwrap();
    }
    let plan = a.plan_compaction(40 * 24 * 3600, 30 * 24 * 3600, false);
    assert!(
        plan.trim_before.is_some(),
        "{name}: nothing to trim: {plan:?}"
    );
    let state = a.state();
    let t = Instant::now();
    let after = a.compact(&plan).unwrap();
    let compact_s = t.elapsed().as_secs_f64();
    assert_eq!(a.state(), state);
    std::fs::write(file(&format!("{name}-depois.bin")), &after).unwrap();

    println!(
        "RESULT {name} mode={} build_s={build_s:.1} merges_s={merge_s:.1} last_merge_ms={:.1} \
         bytes_before={} bytes_after={} compact_s={compact_s:.2}",
        M::NAME,
        last_merge_s * 1000.0,
        before.len(),
        after.len()
    );
    for stage in ["antes", "depois"] {
        open_in_child(&format!("{name}-{stage}.bin"), M::ENTRIES);
    }
}

/// Opens one file in a fresh process (the `growth_open_child` test).
fn open_in_child(name: &str, entries: bool) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "growth_open_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("BIVAR_GROWTH_FILE", file(name))
        .env("BIVAR_GROWTH_ENTRIES", if entries { "1" } else { "0" })
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find_map(|l| l.find("RESULT").map(|i| &l[i..]))
        .unwrap_or_else(|| {
            panic!(
                "child failed: {text}{}",
                String::from_utf8_lossy(&out.stderr)
            )
        });
    println!("{line}");
}

fn open<S: Store>(name: &str) {
    let bytes = std::fs::read(name).unwrap();
    let base = peak_memory();
    let t = Instant::now();
    let d = S::load(&bytes, 9).unwrap();
    let load_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let nodes = d.state().nodes.len();
    let read_s = t.elapsed().as_secs_f64();
    let extra = peak_memory().saturating_sub(base);
    println!(
        "RESULT open {} load_ms={:.1} first_read_ms={:.1} extra_memory_mb={:.1} nodes={nodes}",
        PathBuf::from(name).file_name().unwrap().to_string_lossy(),
        load_s * 1000.0,
        read_s * 1000.0,
        extra as f64 / 1_048_576.0
    );
}

#[test]
#[ignore = "child process of the growth measurement"]
fn growth_open_child() {
    let name = std::env::var("BIVAR_GROWTH_FILE").unwrap();
    if std::env::var("BIVAR_GROWTH_ENTRIES").as_deref() == Ok("1") {
        open::<LoroStore<Entries>>(&name);
    } else {
        open::<LoroStore<Register>>(&name);
    }
}

#[test]
#[ignore = "measurement"]
fn growth_all() {
    for (name, params, block, merge_each) in [
        ("um-param-junta-no-fim", 1, WRITES / 2, false),
        ("um-param-junta-a-cada-100", 1, 50, true),
        ("mil-params-junta-a-cada-100", 1_000, 50, true),
    ] {
        scenario::<Entries>(&format!("{name}-entradas"), params, block, merge_each);
        scenario::<Register>(&format!("{name}-registro"), params, block, merge_each);
    }
}

/// A small merge (100 + 100 writes) in a document with a long history
/// (owner's condition 2 of decision 1, 2026-10-31), with the chosen form:
/// one entry per copy. The history is built in blocks of 25,000 writes per
/// copy.
#[test]
#[ignore = "measurement"]
fn small_merge_vs_history() {
    for total in [100_000usize, 300_000, 1_000_000] {
        let mut a = LoroStore::<Entries>::new_document(1);
        let node = a
            .apply(&Op::CreateNode {
                parent: None,
                index: None,
                kind: "shape".into(),
            })
            .unwrap()
            .created
            .unwrap();
        a.commit();
        let mut b = a.fork_as(2);
        let keys: Vec<String> = (0..1_000).map(|i| format!("k{i}")).collect();
        let mut written = 0;
        while written < total {
            for copy in [&mut a, &mut b] {
                for _ in 0..25_000 {
                    set(copy, &node, &keys[written % keys.len()], written as f64);
                    written += 1;
                }
            }
            a.merge_from(&mut b).unwrap();
            b.merge_from(&mut a).unwrap();
        }
        let mut times = Vec::new();
        for round in 0..3 {
            for copy in [&mut a, &mut b] {
                for i in 0..100 {
                    set(
                        copy,
                        &node,
                        &keys[i * 7 % keys.len()],
                        (round * 1000 + i) as f64,
                    );
                }
            }
            let t = Instant::now();
            a.merge_from(&mut b).unwrap();
            let one = t.elapsed().as_secs_f64();
            b.merge_from(&mut a).unwrap();
            times.push(one * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "RESULT small_merge history_writes={total} merge_100_plus_100_ms_median={:.1} all={times:?} file_bytes={}",
            times[1],
            a.save().len()
        );
    }
}

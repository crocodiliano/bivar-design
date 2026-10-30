//! The fuzzing collection, replayed by the normal `cargo test` on stable
//! (ADR-0024): every file in `fuzz/corpus/` and `fuzz/regressions/` goes
//! through the same functions the fuzzing targets call. A case that once
//! broke a reader stays a test forever.

use std::path::PathBuf;

use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::Store;
use bivar_poc_modelo::testkit::Workload;
use bivar_poc_verificador::{fuzz_container, fuzz_records};

fn fuzz_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../fuzz")
}

fn files(target: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for kind in ["corpus", "regressions"] {
        let dir = fuzz_dir().join(kind).join(target);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() && p.file_name().is_some_and(|n| n != ".gitkeep") {
                    out.push(p);
                }
            }
        }
    }
    out
}

#[test]
fn the_fuzzing_collection_never_crashes_the_reader() {
    let container = files("bivar_container");
    let records = files("bivar_records");
    assert!(
        !container.is_empty(),
        "seed files are missing in fuzz/corpus"
    );
    for p in &container {
        fuzz_container(&std::fs::read(p).unwrap());
    }
    for p in &records {
        fuzz_records(&std::fs::read(p).unwrap());
    }
    println!("{} + {} files replayed", container.len(), records.len());
}

/// Writes the seed files (run once, on demand): small real documents for
/// the container target, and the same as records for the other target.
#[test]
#[ignore = "writes fuzz/corpus seeds"]
fn write_fuzz_seeds() {
    for (i, ops) in [0usize, 20, 200].into_iter().enumerate() {
        let mut s = EntriesStore::new_document(1);
        Workload::new(i as u64 + 1).run(&mut s, ops);
        let snapshot = s.save();
        let file = bivar_poc_arquivo::build_file_bytes(&[(1, &snapshot), (5, &0u64.to_le_bytes())]);
        std::fs::write(
            fuzz_dir().join(format!("corpus/bivar_container/seed-{i}.bivar")),
            &file,
        )
        .unwrap();
        // For bivar_records: kind, 2-byte length, payload.
        if snapshot.len() <= u16::MAX as usize {
            let mut rec = vec![1u8];
            rec.extend_from_slice(&(snapshot.len() as u16).to_le_bytes());
            rec.extend_from_slice(&snapshot);
            rec.extend_from_slice(&[5, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            std::fs::write(
                fuzz_dir().join(format!("corpus/bivar_records/seed-{i}.bin")),
                &rec,
            )
            .unwrap();
        }
    }
}

/// A quick local probe before the real fuzzing (which runs on Linux): random
/// mutations of the seeds, counting panics. On demand.
#[test]
#[ignore = "local probe"]
fn local_mutation_probe() {
    let seeds: Vec<Vec<u8>> = files("bivar_records")
        .iter()
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    let mut rng = bivar_poc_modelo::testkit::Rng::new(1);
    let runs: usize = std::env::var("BIVAR_PROBE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5_000);
    let mut panics: Vec<String> = Vec::new();
    std::panic::set_hook(Box::new(|_| {}));
    for i in 0..runs {
        let mut d = seeds[rng.below(seeds.len())].clone();
        for _ in 0..1 + rng.below(8) {
            let at = 3 + rng.below(d.len() - 3);
            d[at] = rng.next_u64() as u8;
        }
        if std::panic::catch_unwind(|| fuzz_records(&d)).is_err() {
            let name = fuzz_dir().join(format!("regressions/bivar_records/probe-{i}.bin"));
            std::fs::write(&name, &d).unwrap();
            panics.push(name.display().to_string());
        }
    }
    let _ = std::panic::take_hook();
    println!("{runs} mutations, {} panics", panics.len());
    for p in panics.iter().take(5) {
        println!("  {p}");
    }
}

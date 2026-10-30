//! Storage measurements on the reference machine (phase 0.6, part 4). On
//! demand, one at a time, with BIVAR_TEST_DIR set to a folder on the disk
//! to measure (A: hard disk, C: SSD):
//! `cargo test --release -p bivar-poc-arquivo --test medidas <name> -- --ignored --nocapture --test-threads=1`

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bivar_poc_arquivo::DocFile;
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::testkit::{Rng, Workload};
use bivar_poc_modelo::{Op, Store, Value};
use sha2::{Digest, Sha256};

type S = EntriesStore;

fn path(name: &str) -> PathBuf {
    let d = std::env::var("BIVAR_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("bivar-poc-arquivo"));
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join(format!("{name}.bivar"));
    let _ = std::fs::remove_file(&p);
    p
}

fn disk() -> String {
    std::env::var("BIVAR_TEST_DIR").unwrap_or_else(|_| "temp (C:)".into())
}

/// Endless pseudo-random bytes, so pieces never repeat.
struct Noise {
    rng: Rng,
    left: u64,
}

impl Read for Noise {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = (buf.len() as u64).min(self.left) as usize;
        for chunk in buf[..n].chunks_mut(8) {
            let v = self.rng.next_u64().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
        self.left -= n as u64;
        Ok(n)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
#[ignore = "measurement"]
fn blob_2gb() {
    const SIZE: u64 = 2 * 1024 * 1024 * 1024 + 12_345;
    let p = path("blob-2gb");
    let mut s = S::new_document(1);
    let mut f = DocFile::create(&p, &mut s).unwrap();
    let mut source_hash = Sha256::new();
    let t = Instant::now();
    let h = {
        struct Tee<'a, R>(R, &'a mut Sha256);
        impl<R: Read> Read for Tee<'_, R> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.read(buf)?;
                self.1.update(&buf[..n]);
                Ok(n)
            }
        }
        f.put_blob(Tee(
            Noise {
                rng: Rng::new(1),
                left: SIZE,
            },
            &mut source_hash,
        ))
        .unwrap()
    };
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
        node: n.clone(),
        key: "blob".into(),
        value: Some(Value::Str(h.clone())),
    })
    .unwrap();
    s.commit();
    f.save(&mut s, "pc-1").unwrap();
    let write_s = t.elapsed().as_secs_f64();
    let source: [u8; 32] = source_hash.finalize().into();
    assert_eq!(
        h,
        bivar_poc_arquivo::hex(&source),
        "the file's own id is its hash"
    );
    let len_before = f.len();
    // A parameter change afterwards appends only a small record.
    let t = Instant::now();
    s.apply(&Op::SetParam {
        node: n.clone(),
        key: "x".into(),
        value: Some(Value::Num(1.0)),
    })
    .unwrap();
    s.commit();
    f.save(&mut s, "pc-1").unwrap();
    let small_save_ms = t.elapsed().as_secs_f64() * 1000.0;
    let growth = f.len() - len_before;
    drop(f);
    let t = Instant::now();
    let (f, _, _) = DocFile::open::<S>(&p, 2).unwrap();
    let open_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut back = Sha256::new();
    struct Hasher<'a>(&'a mut Sha256);
    impl std::io::Write for Hasher<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let size = f.read_blob(&h, Hasher(&mut back)).unwrap();
    let read_s = t.elapsed().as_secs_f64();
    let back: [u8; 32] = back.finalize().into();
    assert_eq!(size, SIZE);
    assert_eq!(back, source, "identical after reopening");
    println!(
        "RESULT {} blob_2gb write_and_save_s={write_s:.1} open_s={open_s:.2} read_and_check_s={read_s:.1} \
         param_change_save_ms={small_save_ms:.1} param_change_growth_bytes={growth}",
        disk()
    );
    drop(f);
    std::fs::remove_file(&p).unwrap();
}

#[test]
#[ignore = "measurement"]
fn sync_cost() {
    let p = path("sync-cost");
    let mut s = S::new_document(1);
    let mut f = DocFile::create(&p, &mut s).unwrap();
    let x = s
        .apply(&Op::CreateNode {
            parent: None,
            index: None,
            kind: "shape".into(),
        })
        .unwrap()
        .created
        .unwrap();
    let mut times = Vec::new();
    for i in 0..100 {
        s.apply(&Op::SetParam {
            node: x.clone(),
            key: "x".into(),
            value: Some(Value::Num(i as f64)),
        })
        .unwrap();
        s.commit();
        f.append_changes(&mut s).unwrap();
        let t = Instant::now();
        f.sync().unwrap();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "RESULT {} sync_ms median={:.2} p95={:.2} max={:.2}",
        disk(),
        times[50],
        times[95],
        times[99]
    );
    drop(f);
    let _ = std::fs::remove_file(&p);
}

/// The interface edits at 60 frames per second; once per second, autosave.
/// The interface thread only exports the changes (and hands them over);
/// writing and forcing to disk happen on a writer thread. Limit approved
/// by the owner: no pause above 16 ms (one frame).
#[test]
#[ignore = "measurement"]
fn autosave_pause() {
    let p = path("autosave");
    let mut s = S::new_document(1);
    let mut w = Workload::new(3);
    w.run(&mut s, 100_000);
    let f = DocFile::create(&p, &mut s).unwrap();
    let (tx, rx) = mpsc::channel::<(Vec<u8>, Vec<u8>)>();
    let writer = std::thread::spawn(move || {
        let mut f = f;
        let mut worst_sync = 0f64;
        for (bytes, version) in rx {
            f.append_exported(&bytes, version).unwrap();
            let t = Instant::now();
            f.sync().unwrap();
            worst_sync = worst_sync.max(t.elapsed().as_secs_f64() * 1000.0);
        }
        worst_sync
    });
    let mut written = s.version();
    let mut ui_ms = Vec::new();
    let mut frame_work = Vec::new();
    for frame in 0..600 {
        let t = Instant::now();
        w.run(&mut s, 20); // a busy frame: 20 edits
        frame_work.push(t.elapsed().as_secs_f64() * 1000.0);
        if frame % 60 == 59 {
            let t = Instant::now();
            let bytes = s.updates_since(&written).unwrap();
            written = s.version();
            tx.send((bytes, written.clone())).unwrap();
            ui_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        std::thread::sleep(Duration::from_millis(16).saturating_sub(t.elapsed()));
    }
    drop(tx);
    let worst_sync = writer.join().unwrap();
    ui_ms.sort_by(f64::total_cmp);
    frame_work.sort_by(f64::total_cmp);
    println!(
        "RESULT {} autosave_on_interface_thread_ms max={:.2} median={:.2} (limit 16) \
         writer_thread_worst_sync_ms={worst_sync:.1} edit_frame_ms_median={:.2}",
        disk(),
        ui_ms[ui_ms.len() - 1],
        ui_ms[ui_ms.len() / 2],
        frame_work[frame_work.len() / 2]
    );
    let _ = std::fs::remove_file(&p);
}

/// Long use (autosave about every 100 edits), then compaction 31 days later.
#[test]
#[ignore = "measurement"]
fn growth_and_compaction() {
    let p = path("growth");
    let mut s = S::new_document(1);
    let mut f = DocFile::create(&p, &mut s).unwrap();
    let mut w = Workload::new(17);
    for i in 0..2_000 {
        w.run(&mut s, 100);
        f.append_changes(&mut s).unwrap();
        if i % 100 == 99 {
            f.save(&mut s, "pc-1").unwrap();
        }
    }
    f.save(&mut s, "pc-1").unwrap();
    let grown = f.len();
    let state_nodes = s.state().nodes.len();
    let removed = w
        .counts
        .get(&bivar_poc_modelo::testkit::OpKind::Remove)
        .copied()
        .unwrap_or(0);
    let plan = s.plan_compaction(now() + 31 * 24 * 3600, 30 * 24 * 3600, false);
    let t = Instant::now();
    let f = f.compact(&mut s, &plan).unwrap();
    let compact_s = t.elapsed().as_secs_f64();
    let after = f.len();
    drop(f);
    let t = Instant::now();
    let (_, back, _) = DocFile::open::<S>(&p, 2).unwrap();
    let open_s = t.elapsed().as_secs_f64();
    assert_eq!(back.state().nodes.len(), state_nodes);
    println!(
        "RESULT {} growth ops=200000 removals={removed} visible_nodes={state_nodes} \
         file_before_mb={:.1} purged={} history_trimmed={} file_after_mb={:.1} \
         compact_s={compact_s:.2} open_after_s={open_s:.2}",
        disk(),
        grown as f64 / 1_048_576.0,
        plan.purge.len(),
        plan.trim_before.is_some(),
        after as f64 / 1_048_576.0
    );
    let _ = std::fs::remove_file(&p);
}

//! Temporary (branch diag/trava-linux, never merged). Minimal reproduction
//! of the refused lock seen in CI (2026-11-03), without any Bivar code.
//!
//! One thread opens a file for reading and writing, takes the same lock as
//! the prototype (`File::try_lock`), closes it and opens it again, counting
//! the refusals. With `spawn`, other threads launch a simple process
//! (`/bin/true`, stdin null, stdout and stderr piped, as the prototype
//! launches its verifier) in a loop.
//!
//! Usage: bivar-diag-trava-repro <spawn|nospawn> <seconds> <spawner threads> <file>
//! Prints one line: mode, seconds, attempts, refusals, launches.

use std::fs::OpenOptions;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!(
            "usage: {} <spawn|nospawn> <seconds> <spawner threads> <file>",
            args[0]
        );
        std::process::exit(2);
    }
    let spawn = match args[1].as_str() {
        "spawn" => true,
        "nospawn" => false,
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    };
    let seconds: u64 = args[2].parse().expect("seconds");
    let spawners: usize = args[3].parse().expect("spawner threads");
    let path = std::path::PathBuf::from(&args[4]);
    std::fs::write(&path, b"x").expect("create the file");

    let stop = Arc::new(AtomicBool::new(false));
    let launches = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    if spawn {
        for _ in 0..spawners {
            let stop = stop.clone();
            let launches = launches.clone();
            handles.push(std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let out = Command::new("/bin/true")
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output();
                    if out.is_ok() {
                        launches.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }
    }

    let end = Instant::now() + Duration::from_secs(seconds);
    let (mut attempts, mut refusals) = (0u64, 0u64);
    while Instant::now() < end {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open for writing");
        attempts += 1;
        if f.try_lock().is_err() {
            refusals += 1;
        }
        // The file (and its lock, if taken) is closed here; the next turn
        // opens it again.
        drop(f);
    }
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }
    println!(
        "mode={} seconds={seconds} spawner_threads={} attempts={attempts} refusals={refusals} launches={}",
        args[1],
        if spawn { spawners } else { 0 },
        launches.load(Ordering::Relaxed)
    );
}

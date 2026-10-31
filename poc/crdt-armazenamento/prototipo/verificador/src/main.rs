//! The verifier process (ADR-0024): opens one file the way the program
//! would, with its memory capped, and says whether it is acceptable. It
//! never writes. If the file needs more memory than allowed, an allocation
//! fails and this process stops; the program that started it survives and
//! refuses the file.
//!
//! Arguments: path, memory limit (bytes), then the file limits: document
//! record, document total, records, large-file index, all large-file
//! indexes, depth. A tree deeper than the depth limit is answered
//! `TOO_DEEP <depth>`: our own message, not the text of a crash (audit
//! 2026-10-31, M3).

use std::path::PathBuf;
use std::time::Duration;

use bivar_poc_arquivo::{Access, DocFile, Limits, hex};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::every_pass;

#[global_allocator]
static ALLOC: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

fn arg(args: &[String], i: usize) -> u64 {
    args.get(i).and_then(|a| a.parse().ok()).unwrap_or_else(|| {
        println!("REFUSED bad verifier arguments");
        std::process::exit(3)
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = PathBuf::from(args.get(1).cloned().unwrap_or_default());
    let memory = arg(&args, 2);
    let limits = Limits {
        doc_record: arg(&args, 3),
        doc_total: arg(&args, 4),
        records: arg(&args, 5),
        blob_index: arg(&args, 6),
        blob_index_total: arg(&args, 7),
        depth: arg(&args, 8),
    };
    // Test hook: noise on the error output, more than a pipe holds, to
    // prove the program reads it while the verifier runs.
    if let Some(n) = std::env::var("BIVAR_VERIFIER_NOISE")
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
    {
        eprint!("{}", "x".repeat(n));
    }
    // Test hook: a verifier that never finishes, to prove the time limit.
    if std::env::var("BIVAR_VERIFIER_STALL").is_ok() {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    let _ = ALLOC.set_limit(usize::try_from(memory).unwrap_or(usize::MAX));
    match DocFile::open_with::<EntriesStore>(&path, 1, &limits, Access::ReadOnly, None) {
        Ok((f, store, report)) => {
            // Every pass the program makes over the whole document, after
            // decoding all of it: Loro decodes parts lazily, and a bomb in a
            // part nobody read, or a structure one of these passes chokes
            // on, would otherwise go off later, in the program (B1).
            let _ = report;
            let max = usize::try_from(limits.depth).unwrap_or(usize::MAX);
            if let Err(depth) = every_pass(&store, max) {
                println!("TOO_DEEP {depth}");
                std::process::exit(4);
            }
            println!("OK {}", hex(&f.fingerprint()));
        }
        Err(e) => {
            println!("REFUSED {e}");
            std::process::exit(2);
        }
    }
}

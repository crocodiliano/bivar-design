//! The verifier process (ADR-0024): opens one file the way the program
//! would, with its memory capped, and says whether it is acceptable. It
//! never writes. If the file needs more memory than allowed, an allocation
//! fails and this process stops; the program that started it survives and
//! refuses the file.
//!
//! Arguments: path, memory limit (bytes), then the file limits: document
//! record, document total, records, large-file index.

use std::path::PathBuf;
use std::time::Duration;

use bivar_poc_arquivo::{Access, DocFile, Limits, hex};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::Store;

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
    };
    // Test hook: a verifier that never finishes, to prove the time limit.
    if std::env::var("BIVAR_VERIFIER_STALL").is_ok() {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    let _ = ALLOC.set_limit(usize::try_from(memory).unwrap_or(usize::MAX));
    match DocFile::open_with::<EntriesStore>(&path, 1, &limits, Access::ReadOnly, None) {
        Ok((f, store, report)) => {
            // Read everything: Loro decodes parts lazily, and a bomb hidden in
            // a part nobody reads would otherwise go off later, in the program.
            let state = store.state();
            let _ = (state.nodes.len(), report);
            println!("OK {}", hex(&f.fingerprint()));
        }
        Err(e) => {
            println!("REFUSED {e}");
            std::process::exit(2);
        }
    }
}

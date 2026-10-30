//! Disposable storage and sync proof of concept for Bivar (phase 0.6).
//! Not a code base for the product.
//!
//! `model` is the library-independent socket; `loro_doc` and
//! `automerge_doc` plug Loro and Automerge into it; `workload` drives both
//! with the same operations.

pub mod automerge_doc;
pub mod frac_index;
pub mod loro_doc;
pub mod model;
pub mod workload;

use std::time::{SystemTime, UNIX_EPOCH};

/// A document identity, new for every document and every clean copy.
/// A merge between different identities is refused (ADR-0019, decision 5).
pub fn new_doc_id(peer: u64) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("doc-{peer:x}-{nanos:x}")
}

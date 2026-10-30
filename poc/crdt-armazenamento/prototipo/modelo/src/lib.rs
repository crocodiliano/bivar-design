//! Library-independent document model for the phase 0.6 prototype
//! (ADR-0021). Operations, the undo layer and conflict detection live here;
//! the CRDT library lives only behind `Store`, in `loja-loro`.

pub mod detect;
pub mod document;
pub mod op;
pub mod store;
pub mod testkit;

pub use document::{Document, UndoOutcome};
pub use op::{Edge, NodeId, OP_FORMAT_VERSION, Op, Value};
pub use store::{
    Anchor, Applied, CompactionPlan, HiddenLayer, MergeReport, OpError, State, Store, VISIBLE_KEY,
    clean_copy, hidden_layers, random_u64,
};

//! Disposable text engine proof of concept for Bivar (phase 0.5).
//! Not a code base for the product.

pub mod cpu;
pub mod font_tables;
pub mod fonts;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod image;
pub mod page;
pub mod shaping;

//! seiri's pipeline, exposed as a library so the binary and the benchmarks can
//! share the same implementation.
//!
//! See `docs/interfaces.md` for the end-to-end diagram:
//! `File --> Parser --> Resolver --> Graph Nodes + Edges --> GUI / PNG / SVG`

pub mod analysis;
pub mod core;
pub mod discovery;
pub mod export;
pub mod gui;
pub mod layout;
pub mod parsers;
pub mod update;

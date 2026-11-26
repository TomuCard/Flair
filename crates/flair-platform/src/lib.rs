//! OS-specific code. Everything platform-dependent lives here, behind common
//! types and traits, so the rest of Flair has no scattered `#[cfg]`.
//!
//! Milestone 1 only provides the standard paths; global shortcut, app
//! listing, icons and IPC come with the next milestones.
//!
//! This is the only crate allowed to use `unsafe`; every block has a
//! `// SAFETY:` comment (enforced by clippy).
#![allow(unsafe_code)]

pub mod paths;
pub mod renderer;

//! OS-specific code. Everything platform-dependent lives here, behind common
//! types and functions, so the rest of Flair has no scattered `#[cfg]`.
//!
//! This is the only crate allowed to use `unsafe`; every block has a
//! `// SAFETY:` comment (enforced by clippy).
#![allow(unsafe_code)]

pub mod apps;
pub mod autostart;
pub mod console;
#[cfg(target_os = "linux")]
mod dbus;
pub mod dialog;
pub mod hotkey;
pub mod image;
pub mod install;
pub mod ipc;
pub mod memory;
pub mod open;
pub mod paths;
pub mod process;
pub mod renderer;
pub mod theme;
pub mod tray;
pub mod window;

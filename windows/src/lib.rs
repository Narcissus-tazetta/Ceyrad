//! Ceyrad for Windows.
//!
//! `core` is a hand port of the macOS Swift app's pure logic and contains no
//! platform calls, so it builds and tests anywhere. Everything Windows-specific
//! lives outside it.

pub mod core;
pub mod discord;

#[cfg(windows)]
pub mod app;
#[cfg(windows)]
pub mod catalog;
#[cfg(windows)]
pub mod launch_at_login;
#[cfg(windows)]
pub mod smtc;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod updater;
#[cfg(windows)]
pub mod winhttp;

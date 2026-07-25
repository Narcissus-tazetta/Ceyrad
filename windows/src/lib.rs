//! Ceyrad for Windows.
//!
//! `core` is a hand port of the macOS Swift app's pure logic and contains no
//! platform calls, so it builds and tests anywhere. Everything Windows-specific
//! lives outside it.

pub mod core;
pub mod discord;

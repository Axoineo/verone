//! Core library for Verone.
//!
//! Every front end (command line, desktop app, server) calls into this crate.
//! It owns the rules: what a phone is, where its data lives, how the host is
//! checked, and the interface that engines implement.

pub mod engine;
pub mod error;
pub mod process;
pub mod profile;
pub mod storage;

pub use error::{Error, ErrorKind, Result};

/// Version of the Verone core library.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_matches_package() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}

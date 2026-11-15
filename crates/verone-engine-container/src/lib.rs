//! Container engine for Verone.
//!
//! Runs Android (redroid) in a container that shares the host's Linux kernel.

pub mod adb;
pub mod engine;
pub mod host;
pub mod images;
pub mod runtime;

pub use engine::ContainerEngine;

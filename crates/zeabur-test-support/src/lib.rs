#![doc = include_str!("../README.md")]

#[cfg(feature = "allocations")]
pub mod allocations;
/// Loads the language-neutral test cases and compares results with them.
pub mod fixtures;
#[cfg(feature = "sandbox")]
pub mod sandbox;
#[cfg(feature = "tls")]
pub mod tls;

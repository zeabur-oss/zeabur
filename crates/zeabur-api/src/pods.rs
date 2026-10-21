//! A pod's containers: their logs, a terminal in them, and a debug container for one without a
//! shell.

pub mod debug;
pub mod exec;
pub(crate) mod logs;

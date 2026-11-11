//! What a piece of code allocates, for the manual cost runs (the `zeabur-overview-cost` skill).
//!
//! Linking this module makes the test binary's allocator a counting one. Only the thread that
//! runs [`measure`] is counted, so a server running on another thread in the same test does not
//! add to the numbers.

use std::future::Future;

/// Allocations made while a body ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocations {
    /// Bytes allocated in total, freed or not.
    pub bytes_total: u64,
    /// The most bytes alive at one time.
    pub bytes_peak: u64,
    /// How many allocations were made.
    pub count: u64,
}

/// Runs `body` to completion on the current thread and counts what that thread allocated.
///
/// The runtime must be a current-thread one that the caller built outside the measured region,
/// so that every task of the body, the connection tasks of an HTTP client included, runs on this
/// thread.
pub fn measure<F: Future>(runtime: &tokio::runtime::Runtime, body: F) -> (F::Output, Allocations) {
    let mut output = None;
    let info = allocation_counter::measure(|| output = Some(runtime.block_on(body)));
    let Some(output) = output else {
        unreachable!("the measured closure ran to completion")
    };
    (
        output,
        Allocations {
            bytes_total: info.bytes_total,
            bytes_peak: info.bytes_max,
            count: info.count_total,
        },
    )
}

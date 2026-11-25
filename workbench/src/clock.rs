//! The one wall clock: now, in Unix milliseconds. A caller that keeps seconds divides at its edge.

use std::time::UNIX_EPOCH;

/// Now, in Unix milliseconds; 0 on a clock set before 1970.
pub fn now_ms() -> u64 {
    UNIX_EPOCH
        .elapsed()
        .map_or(0, |since| since.as_millis() as u64)
}

//! Opt-in phase timings for the file pipeline (feature 023, T038).
//!
//! With `CRUSH_TIMINGS` set, `compress` and `decompress` print one line to
//! stderr with the time spent in each phase (read, codec, write, ...), so
//! single-core pipeline costs can be measured without a profiler.

use std::time::{Duration, Instant};

/// Phase timer; does nothing unless `CRUSH_TIMINGS` is set.
pub struct Phases {
    start: Instant,
    last: Instant,
    parts: Option<Vec<(&'static str, Duration)>>,
}

impl Phases {
    /// Start timing (reads `CRUSH_TIMINGS` once).
    #[must_use]
    pub fn start() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last: now,
            parts: std::env::var_os("CRUSH_TIMINGS").map(|_| Vec::new()),
        }
    }

    /// Close the current phase under `name`.
    pub fn mark(&mut self, name: &'static str) {
        if let Some(parts) = &mut self.parts {
            let now = Instant::now();
            parts.push((name, now - self.last));
            self.last = now;
        }
    }

    /// Print `crush-timings <what> bytes=N total=... <phase>=...` (milliseconds).
    pub fn report(&self, what: &str, bytes: u64) {
        if let Some(parts) = &self.parts {
            let ms = |d: Duration| d.as_secs_f64() * 1e3;
            let phases: Vec<String> = parts
                .iter()
                .map(|(n, d)| format!("{n}={:.2}", ms(*d)))
                .collect();
            eprintln!(
                "crush-timings {what} bytes={bytes} total={:.2} {}",
                ms(self.start.elapsed()),
                phases.join(" ")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_without_env_records_nothing() {
        // The test process does not set CRUSH_TIMINGS.
        if std::env::var_os("CRUSH_TIMINGS").is_none() {
            let mut p = Phases::start();
            p.mark("read");
            assert!(p.parts.is_none());
        }
    }
}

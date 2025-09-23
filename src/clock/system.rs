//! System clock implementation using standard library time functions.

use crate::clock::{BaseClock, ClockResult};
use std::time::Instant;

#[cfg(test)]
use std::time::Duration;

/// System clock implementation using Instant for high-resolution timing.
///
/// This clock uses `std::time::Instant` to provide monotonic time measurements
/// that are not affected by system clock adjustments. The time is measured
/// in nanoseconds from when the clock was started.
#[derive(Debug)]
pub struct SystemClock {
    start_time: Option<Instant>,
    is_running: bool,
}

impl SystemClock {
    /// Create a new system clock instance.
    ///
    /// The clock is created in a stopped state and must be started
    /// using the `start()` method before time measurements can be taken.
    pub fn new() -> Self {
        Self {
            start_time: None,
            is_running: false,
        }
    }

    /// Create and start a new system clock instance.
    ///
    /// This is a convenience method that creates a new clock and immediately
    /// starts it, returning a clock ready for time measurements.
    ///
    /// # Errors
    /// Returns an error if the clock cannot be started.
    pub fn new_started() -> ClockResult<Self> {
        let mut clock = Self::new();
        clock.start()?;
        Ok(clock)
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseClock for SystemClock {
    fn get_time(&self) -> ClockResult<u64> {
        if !self.is_running {
            return Err("Clock is not running. Call start() first.".into());
        }

        let start_time = self.start_time.ok_or("Clock start time not set")?;

        let elapsed = start_time.elapsed();
        Ok(elapsed.as_nanos() as u64)
    }

    fn start(&mut self) -> ClockResult<()> {
        self.start_time = Some(Instant::now());
        self.is_running = true;
        Ok(())
    }

    fn stop(&mut self) -> ClockResult<()> {
        self.is_running = false;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.is_running
    }

    fn resolution(&self) -> u64 {
        // System clock typically has nanosecond resolution
        1 // 1 nanosecond
    }

    fn reset(&mut self) -> ClockResult<()> {
        if self.is_running {
            self.start_time = Some(Instant::now());
        } else {
            self.start_time = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_time() {
        let mut clock = SystemClock::new();
        clock.start().unwrap();
        let time1 = clock.get_time().unwrap();

        std::thread::sleep(Duration::from_millis(1));

        let time2 = clock.get_time().unwrap();
        assert!(time2 > time1, "Time should advance");
    }

    #[test]
    fn test_system_clock_new_started() {
        let clock = SystemClock::new_started().unwrap();
        assert!(clock.is_running());
        assert!(clock.get_time().is_ok());
    }

    #[test]
    fn test_system_clock_reset() {
        let mut clock = SystemClock::new_started().unwrap();

        // Get initial time
        std::thread::sleep(Duration::from_millis(1));
        let time1 = clock.get_time().unwrap();

        // Reset and check time is smaller
        clock.reset().unwrap();
        let time2 = clock.get_time().unwrap();
        assert!(time2 < time1);
    }
}

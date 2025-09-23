//! Base clock interface for timing operations.
//!
//! Provides a common interface for timing operations used in AgentFlow
//! for synchronization, scheduling, and time-based processing.

/// Result type for clock operations
pub type ClockResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Abstract base trait for clock implementations.
///
/// Provides a common interface for timing operations used in AgentFlow
/// for synchronization, scheduling, and time-based processing.
pub trait BaseClock: Send + Sync + std::fmt::Debug {
    /// Get the current time value.
    ///
    /// Returns the current time as an integer value. The specific unit and
    /// reference point depend on the concrete implementation. Typically this
    /// should return time in nanoseconds for high precision timing.
    ///
    /// # Returns
    /// The current time as a u64 value in nanoseconds since some reference point.
    ///
    /// # Errors
    /// Returns an error if the clock cannot provide the current time.
    fn get_time(&self) -> ClockResult<u64>;

    /// Start or initialize the clock.
    ///
    /// Performs any necessary initialization or starts the timing mechanism.
    /// This method should be called before using get_time().
    ///
    /// # Errors
    /// Returns an error if the clock cannot be started or initialized.
    fn start(&mut self) -> ClockResult<()>;

    /// Stop the clock and clean up resources.
    ///
    /// This method should be called when the clock is no longer needed
    /// to properly clean up any resources.
    ///
    /// # Errors
    /// Returns an error if cleanup fails, though this is typically logged
    /// rather than propagated.
    fn stop(&mut self) -> ClockResult<()> {
        // Default implementation does nothing
        Ok(())
    }

    /// Check if the clock is currently running.
    ///
    /// # Returns
    /// `true` if the clock is running and can provide time values, `false` otherwise.
    fn is_running(&self) -> bool {
        true // Default implementation assumes always running
    }

    /// Get the clock's resolution in nanoseconds.
    ///
    /// Returns the smallest time interval that this clock can measure.
    /// This is useful for understanding the precision of timing operations.
    ///
    /// # Returns
    /// The clock resolution in nanoseconds.
    fn resolution(&self) -> u64 {
        1_000_000 // Default to 1ms resolution
    }

    /// Reset the clock to its initial state.
    ///
    /// This method resets any internal timing state while keeping the clock running.
    /// Useful for scenarios where you want to restart timing measurements.
    ///
    /// # Errors
    /// Returns an error if the clock cannot be reset.
    fn reset(&mut self) -> ClockResult<()> {
        // Default implementation does nothing
        Ok(())
    }
}

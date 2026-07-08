//! Chip-8 Timers
//!
//! The Chip-8 has two timers: a delay timer and a sound timer.
//! Both timers count down at 60 Hz.

/// Represents the two timers of the Chip-8 system.
#[derive(Debug, Default)]
pub struct Timers {
    /// The delay timer is used for timing events in games.
    pub delay: u8,
    /// The sound timer is used for generating sound.
    pub sound: u8,
}

impl Timers {
    /// Creates a new `Timers` instance with both timers initialized to 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates both timers.
    ///
    /// This method should be called at a rate of 60 Hz.
    pub fn update(&mut self) {
        if self.delay > 0 {
            self.delay -= 1;
        }
        if self.sound > 0 {
            self.sound -= 1;
        }
    }
}

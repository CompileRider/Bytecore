//! Frontend rendering backends for the Bytecore Chip-8 emulator.
//!
//! This module defines the `Frontend` trait that all display backends implement,
//! and provides a `TickSource` for frame-rate timing.
//!
//! Available backends (feature-gated):
//! - `terminal` (default): TUI using ratatui + crossterm
//! - `sdl2` (optional): Graphical window using SDL2

use crate::chip8::display::Display;
use crate::chip8::keypad::Keypad;
use std::time::{Duration, Instant};

/// Timing source for synchronizing the emulator's main loop.
///
/// Provides frame-rate limiting by sleeping until the next frame boundary.
#[derive(Debug)]
pub struct TickSource {
    /// Timestamp of the last rendered frame.
    last_frame: Instant,
    /// Duration of a single frame (e.g., 16.67 ms for 60 FPS).
    frame_duration: Duration,
}

impl TickSource {
    /// Creates a new `TickSource` at the given frame rate.
    ///
    /// # Arguments
    ///
    /// * `fps` - Target frames per second (e.g., 60).
    pub fn new(fps: u64) -> Self {
        Self {
            last_frame: Instant::now(),
            frame_duration: Duration::from_secs_f64(1.0 / fps as f64),
        }
    }

    /// Blocks until it's time for the next frame.
    ///
    /// This ensures the main loop runs at the configured frame rate
    /// and doesn't consume 100% CPU.
    pub fn wait_for_next_frame(&mut self) {
        let elapsed = self.last_frame.elapsed();
        if elapsed < self.frame_duration {
            std::thread::sleep(self.frame_duration - elapsed);
        }
        self.last_frame = Instant::now();
    }
}

/// Represents an action triggered by the user via the frontend interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    /// Continue execution normally.
    Continue,
    /// Gracefully exit the emulator.
    Exit,
    /// Toggle the paused state of the emulator.
    PauseToggle,
    /// Reset the emulator to its initial state.
    Reset,
}

/// Common trait for all display frontends.
///
/// Each frontend implements input handling and display rendering.
/// The two built-in implementations are:
/// - `TerminalFrontend` (ratatui + crossterm)
/// - `Sdl2Frontend` (SDL2 window + audio)
pub trait Frontend {
    /// Handles input events and updates the keypad state.
    ///
    /// This method is called once per frame. It should process all pending
    /// input events (key presses, window events) and update the Chip-8
    /// keypad accordingly.
    ///
    /// # Arguments
    ///
    /// * `keypad` - Mutable reference to the Chip-8 keypad to update.
    ///
    /// # Returns
    ///
    /// A `UserAction` indicating whether to continue, exit, pause, or reset.
    fn handle_events(&mut self, keypad: &mut Keypad) -> UserAction;

    /// Renders the current display state on screen.
    ///
    /// Called once per frame after input handling. The frontend should
    /// read the current pixel buffer from the display and present it
    /// on screen (terminal window, SDL2 window, etc.).
    ///
    /// # Arguments
    ///
    /// * `display` - Reference to the Chip-8 display state.
    fn render(&mut self, display: &Display);

    /// Waits until it's time for the next frame.
    ///
    /// This prevents the emulator from running at maximum speed.
    /// Called once per frame, after rendering.
    fn wait_for_next_frame(&mut self);

    /// Updates the sound timer state.
    ///
    /// Called once per frame with `true` when the Chip-8 sound timer
    /// is non-zero (should produce a beep).
    /// Default implementation does nothing.
    fn update_sound(&mut self, _active: bool) {}
}

/// Maps a PC keyboard key to a Chip-8 key code (0x0–0xF).
///
/// The mapping follows the Cowgod §2.3 layout:
///
/// ```text
/// Chip-8  →  PC Keyboard
/// 1 2 3 C     1 2 3 4
/// 4 5 6 D     Q W E R
/// 7 8 9 E     A S D F
/// A 0 B F     Z X C V
/// ```
///
/// Returns `None` if the key has no Chip-8 equivalent.
pub fn map_key_to_chip8(key: char) -> Option<u8> {
    match key {
        '1' => Some(0x1),
        '2' => Some(0x2),
        '3' => Some(0x3),
        '4' => Some(0xC),
        'q' | 'Q' => Some(0x4),
        'w' | 'W' => Some(0x1), // Map W to 1 (PONG Player 1 Up)
        's' | 'S' => Some(0x4), // Map S to 4 (PONG Player 1 Down)
        'e' | 'E' => Some(0x6),
        'r' | 'R' => Some(0xD),
        'i' | 'I' => Some(0xC), // Map I to C (PONG Player 2 Up)
        'k' | 'K' => Some(0xD), // Map K to D (PONG Player 2 Down)
        'a' | 'A' => Some(0x7),
        'd' | 'D' => Some(0x9),
        'f' | 'F' => Some(0xE),
        'z' | 'Z' => Some(0xA),
        'x' | 'X' => Some(0x0),
        'c' | 'C' => Some(0xB),
        'v' | 'V' => Some(0xF),
        ' ' => Some(0x5), // Map Space to 5 (Action/Fire)
        _ => None,
    }
}

#[cfg(feature = "terminal")]
pub mod terminal;

/// SDL2 graphical frontend (requires the `sdl2` feature).
#[cfg(feature = "sdl2")]
pub mod sdl2;

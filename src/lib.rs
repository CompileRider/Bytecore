#![forbid(unsafe_code)]
//! # Bytecore: A Simple Chip-8 Emulator
//!
//! `bytecore` is a library that provides the core components for emulating the Chip-8
//! virtual machine. It is designed to be a simple, modular, and easy-to-use library
//! that can be integrated into different frontends (e.g., SDL2, terminal).
//!
//! ## Features
//!
//! *   Modular design with separate components for CPU, memory, and display.
//! *   Configurable options for handling Chip-8 quirks.
//! *   A simple API for running ROMs.
//!
//! ## Usage
//!
//! ```no_run
//! use bytecore::Emulator;
//!
//! fn main() {
//!     let mut emulator = Emulator::new();
//!     if let Err(e) = emulator.run("path/to/rom.ch8") {
//!         eprintln!("Application error: {}", e);
//!     }
//! }
//! ```

// Declare the modules that will be part of our library.

/// Contains the core components of the Chip-8 virtual machine.
pub mod chip8;
/// Provides different frontends for displaying the emulator's output.
pub mod frontend;

use thiserror::Error;

/// The main error type for the emulator library.
///
/// This enum encapsulates all possible errors that can occur during the
/// emulator's operation.
#[derive(Error, Debug)]
pub enum EmulatorError {
    /// Represents an I/O error that occurred while reading a ROM file.
    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),
}

/// A type alias for `Result` with the `EmulatorError` type.
pub type Result<T> = std::result::Result<T, EmulatorError>;

/// The main struct that encapsulates the entire state of the Chip-8 emulator.
///
/// This struct holds all the components of the emulator, including the CPU,
/// memory, and display. It provides the main interface for running a ROM.
#[derive(Debug)]
pub struct Emulator;

impl Emulator {
    /// Creates a new, initialized emulator instance.
    ///
    /// # Returns
    ///
    /// A new `Emulator` instance with its components initialized.
    pub fn new() -> Self { Self {} }

    /// Runs the emulator by loading a ROM from the specified path.
    ///
    /// This method will load the ROM into memory and start the fetch-decode-execute
    /// cycle of the CPU.
    ///
    /// # Arguments
    ///
    /// * `rom_path` - The path to the Chip-8 ROM file to load.
    ///
    /// # Returns
    ///
    /// An empty `Result` indicating success or an `EmulatorError` if an error occurred.
    pub fn run(&mut self, rom_path: &str) -> Result<()> {
        println!("Running emulator with ROM: {}", rom_path);
        // The logic for loading the ROM and running the CPU cycle will go here.
        Ok(())
    }
}

impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}

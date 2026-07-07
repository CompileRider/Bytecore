//! # Bytecore Emulator Executable
//!
//! This is the main entry point for the Bytecore Chip-8 emulator.
//!
//! It is responsible for parsing command-line arguments, creating an emulator instance,
//! and running the specified ROM file.
//!
//! ## Usage
//!
//! ```sh
//! cargo run -- path/to/your/rom.ch8
//! ```
use bytecore::Emulator;
use clap::{Parser, ValueEnum};
use std::process;

/// A Chip-8 emulator written in Rust.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The path to the Chip-8 ROM file to load.
    #[arg(required = true)]
    rom_path: String,

    /// Display backend to use.
    #[arg(long, value_enum, default_value_t = Backend::Terminal)]
    backend: Backend,

    /// CPU clock speed in Hz.
    #[arg(long, default_value_t = 700)]
    hz: u32,

    /// Enable debug logging.
    #[arg(long)]
    debug: bool,
}

/// Represents the available display backends.
#[derive(ValueEnum, Clone, Debug)]
enum Backend {
    Sdl2,
    Terminal,
}

fn main() {
    // Parse command-line arguments.
    let args = Args::parse();

    // Create an emulator instance from our library.
    let mut emulator = Emulator::new();

    // Run the emulator and handle any errors that may occur.
    if let Err(e) = emulator.run(&args.rom_path) {
        eprintln!("Application error: {}", e);
        process::exit(1);
    }
}

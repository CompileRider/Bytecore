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
use bytecore::chip8::config::{Config, Quirks};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::process;

/// A Chip-8 emulator written in Rust.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The path to the Chip-8 ROM file to load.
    #[arg(required = true)]
    rom_path: String,

    /// Path to a TOML configuration file.
    /// If not provided, default settings are used.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override the quirk preset.
    /// Accepts: cosmac-vip, modern, hp48
    #[arg(long)]
    quirks: Option<String>,

    /// CPU clock speed in Hz (default: 700).
    #[arg(long, default_value_t = 700)]
    hz: u32,

    /// Display backend to use.
    #[arg(long, value_enum, default_value_t = Backend::Terminal)]
    backend: Backend,

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

    // Load configuration from TOML file if provided, otherwise use defaults.
    let mut config = match &args.config {
        Some(path) => {
            if path.exists() {
                Config::load(path)
            } else {
                eprintln!("Warning: config file '{}' not found, using defaults", path.display());
                Config::default()
            }
        }
        None => Config::default(),
    };

    // Override quirk preset from CLI if provided.
    if let Some(ref quirk_name) = args.quirks {
        config.quirks = match quirk_name.as_str() {
            "cosmac-vip" | "cosmac" => Quirks::cosmac_vip(),
            "hp48" => Quirks::hp48(),
            "modern" => Quirks::modern(),
            other => {
                eprintln!("Warning: unknown quirk preset '{}', using modern", other);
                Quirks::modern()
            }
        };
    }

    // Override CPU clock speed from CLI.
    config.cpu_hz = args.hz;

    // Create emulator with the assembled configuration.
    let mut emulator = Emulator::with_config(config);

    // Run the emulator and handle any errors that may occur.
    if let Err(e) = emulator.run(&args.rom_path) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

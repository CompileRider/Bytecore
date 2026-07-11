//! The core components of the CHIP-8 virtual machine.
//!
//! This module contains all the hardware components of the Chip-8 system:
//! the CPU, memory, display, keypad, stack, timers, and configuration.
//! The `Chip8` struct aggregates these components and provides the main
//! interface for running a ROM.

pub mod config;
pub mod cpu;
pub mod display;
pub mod keypad;
pub mod memory;
pub mod opcode;
pub mod stack;
pub mod timers;

use config::{Config, Quirks};
use cpu::Cpu;
use display::Display;
use keypad::Keypad;
use memory::Memory;
use stack::Stack;
use timers::Timers;

/// The main Chip-8 struct, which contains all the components of the system.
///
/// This struct aggregates the CPU, memory, display, keypad, stack, timers,
/// and configuration into a single unit. It provides methods for loading ROMs,
/// executing CPU ticks, and accessing individual components for frontend rendering.
#[derive(Debug)]
pub struct Chip8 {
    /// The system's CPU.
    cpu: Cpu,
    /// The system's display.
    display: Display,
    /// The quirk configuration.
    config: Config,
    /// The system's timers.
    timers: Timers,
    /// The system's stack.
    stack: Stack,
    /// The system's memory.
    memory: Memory,
    /// The system's keypad.
    keypad: Keypad,
}

impl Chip8 {
    /// Creates a new `Chip8` instance with all components initialized.
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            display: Display::new(),
            config: Config::new(),
            timers: Timers::new(),
            stack: Stack::new(),
            memory: Memory::new(),
            keypad: Keypad::new(),
        }
    }

    /// Returns a reference to the display for frontend rendering.
    pub fn display(&self) -> &Display {
        &self.display
    }

    /// Returns a mutable reference to the display for frontend rendering.
    pub fn display_mut(&mut self) -> &mut Display {
        &mut self.display
    }

    /// Returns a mutable reference to the keypad for frontend input.
    pub fn keypad(&mut self) -> &mut Keypad {
        &mut self.keypad
    }

    /// Returns a reference to the quirk configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns a mutable reference to the quirk configuration.
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Returns the current quirk flags.
    pub fn quirks(&self) -> Quirks {
        self.config.quirks
    }

    /// Loads a ROM into memory and resets the CPU to its initial state.
    pub fn load_rom(&mut self, rom: &[u8]) {
        self.memory.write_rom(rom);
        self.cpu.reset();
    }

    /// Executes one CPU tick (fetch-decode-execute cycle).
    pub fn tick(&mut self) -> Result<(), cpu::CpuError> {
        let quirks = self.config.quirks;
        self.cpu.tick(
            &mut self.memory,
            &self.keypad,
            &mut self.stack,
            &mut self.timers,
            &mut self.display,
            quirks,
        )
    }

    /// Updates timers at 60 Hz. Should be called separately from tick()
    /// to maintain the correct timer rate independent of CPU speed.
    pub fn update_timers(&mut self) {
        self.timers.update();
    }
}

impl Default for Chip8 {
    fn default() -> Self {
        Self::new()
    }
}

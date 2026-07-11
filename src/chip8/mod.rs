//! The core components of the Chip-8 virtual machine.

pub mod keypad;
pub mod memory;
pub mod opcode;
pub mod stack;
pub mod timers;

use keypad::Keypad;
use memory::Memory;
use stack::Stack;
use timers::Timers;

/// The main Chip-8 struct, which contains all the components of the system.
#[derive(Debug)]
pub struct Chip8 {
    /// The system's timers.
    pub timers: Timers,
    /// The system's stack.
    pub stack: Stack,
    /// The system's memory.
    pub memory: Memory,
    /// The system's keypad.
    pub keypad: Keypad,
}

impl Chip8 {
    /// Creates a new `Chip8` instance.
    pub fn new() -> Self {
        Self {
            timers: Timers::new(),
            stack: Stack::new(),
            memory: Memory::new(),
            keypad: Keypad::new(),
        }
    }
}

impl Default for Chip8 {
    fn default() -> Self {
        Self::new()
    }
}

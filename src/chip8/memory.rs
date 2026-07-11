//! The memory module for the Chip-8 emulator.
//!
//! The Chip-8 system has 4096 bytes of RAM. Programs are loaded at address
//! 0x200, and the built-in font sprites are stored at 0x050. All memory
//! access outside the 4KB address space returns an error rather than panicking.

use thiserror::Error;

/// The Chip-8 has 4096 bytes of memory.
const MEMORY_SIZE: usize = 4096;
/// Chip-8 programs are loaded starting at this address.
const PROG_START: u16 = 0x200;
/// The font set is loaded into this area of memory.
const FONT_SET_START: usize = 0x050;

/// Errors that can occur during memory operations.
///
/// The Chip-8 has a 4KB address space (0x000–0xFFF). Any access outside
/// this range is an error rather than a panic, allowing the caller to
/// handle invalid ROM jumps gracefully.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum MemoryError {
    /// Attempted to access memory outside the valid 4KB address space.
    #[error("Out of bounds memory access at address: {0:#06X}")]
    OutOfBounds(usize),
}

/// Represents the RAM of the Chip-8 system.
#[derive(Debug)]
pub struct Memory {
    /// An array representing the 4096 bytes of RAM.
    ram: [u8; MEMORY_SIZE],
}

impl Memory {
    /// Creates a new, initialized `Memory` instance.
    ///
    /// The memory is initialized to all zeros, and then the Chip-8 font set
    /// is loaded into its reserved memory area.
    pub fn new() -> Self {
        let mut memory = Memory { ram: [0; MEMORY_SIZE] };
        memory.load_fonts();
        memory
    }

    /// Reads a single byte from memory at the given address.
    ///
    /// Returns `Err(MemoryError::OutOfBounds)` if the address is >= 4096.
    ///
    /// # Arguments
    ///
    /// * `addr` - The memory address to read from (0x000–0xFFF).
    pub fn read_byte(&self, addr: u16) -> Result<u8, MemoryError> {
        self.ram.get(addr as usize).copied().ok_or(MemoryError::OutOfBounds(addr as usize))
    }

    /// Reads a 16-bit word from memory in big-endian format.
    ///
    /// Returns `Err(MemoryError::OutOfBounds)` if either byte falls
    /// outside the valid 4KB address space.
    ///
    /// # Arguments
    ///
    /// * `addr` - The memory address to read the word from.
    pub fn read_word(&self, addr: u16) -> Result<u16, MemoryError> {
        let hi = self.read_byte(addr)?;
        let lo = self.read_byte(addr.wrapping_add(1))?;
        Ok(((hi as u16) << 8) | (lo as u16))
    }

    /// Writes a single byte to memory at the given address.
    ///
    /// Returns `Err(MemoryError::OutOfBounds)` if the address is >= 4096.
    ///
    /// # Arguments
    ///
    /// * `addr` - The memory address to write to (0x000–0xFFF).
    /// * `value` - The byte value to write.
    pub fn write_byte(&mut self, addr: u16, value: u8) -> Result<(), MemoryError> {
        self.ram
            .get_mut(addr as usize)
            .map(|cell| *cell = value)
            .ok_or(MemoryError::OutOfBounds(addr as usize))
    }

    /// Writes a ROM's data into memory, starting at `PROG_START`.
    ///
    /// # Arguments
    ///
    /// * `rom` - A byte slice containing the ROM program.
    pub fn write_rom(&mut self, rom: &[u8]) {
        let start = PROG_START as usize;
        let end = start + rom.len();
        self.ram[start..end].copy_from_slice(rom);
    }

    /// Loads the hexadecimal font set into memory.
    fn load_fonts(&mut self) {
        let font_set = [
            0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
            0x20, 0x60, 0x20, 0x20, 0x70, // 1
            0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
            0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
            0x90, 0x90, 0xF0, 0x10, 0x10, // 4
            0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
            0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
            0xF0, 0x10, 0x20, 0x40, 0x40, // 7
            0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
            0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
            0xF0, 0x90, 0xF0, 0x90, 0x90, // A
            0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
            0xF0, 0x80, 0x80, 0x80, 0xF0, // C
            0xE0, 0x90, 0x90, 0x90, 0xE0, // D
            0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
            0xF0, 0x80, 0xF0, 0x80, 0x80, // F
        ];
        let start = FONT_SET_START;
        let end = start + font_set.len();
        self.ram[start..end].copy_from_slice(&font_set);
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

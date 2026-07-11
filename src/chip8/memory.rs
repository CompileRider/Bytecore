//! The memory module for the Chip-8 emulator.

/// The Chip-8 has 4096 bytes of memory.
const MEMORY_SIZE: usize = 4096;
/// Chip-8 programs are loaded starting at this address.
const PROG_START: u16 = 0x200;
/// The font set is loaded into this area of memory.
const FONT_SET_START: usize = 0x050;

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
        let mut memory = Memory {
            ram: [0; MEMORY_SIZE],
        };
        memory.load_fonts();
        memory
    }

    /// Reads a 16-bit word from memory in big-endian format.
    ///
    /// # Arguments
    ///
    /// * `addr` - The memory address to read the word from.
    pub fn read_word(&self, addr: u16) -> u16 {
        let addr = addr as usize;
        (self.ram[addr] as u16) << 8 | (self.ram[addr + 1] as u16)
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

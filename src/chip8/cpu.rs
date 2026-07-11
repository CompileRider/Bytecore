//! Chip-8 CPU
//!
//! The CPU is the heart of the Chip-8 virtual machine. It contains:
//! - 16 general-purpose 8-bit registers (V0–VF, where VF is the flag register)
//! - A 16-bit index register (I) for memory addressing
//! - A 16-bit program counter (PC), initialized to 0x200
//! - A state machine (Running, WaitingForKey, Halted)
//! - A xorshift32 PRNG for the RND instruction
//!
//! # Fetch-Decode-Execute Cycle
//!
//! Each call to `tick()` performs one instruction cycle:
//! 1. Check CPU state (halted? waiting for key?)
//! 2. Validate PC is within bounds (< 0xFFF)
//! 3. Fetch 2-byte opcode from memory at PC
//! 4. Decode into a typed `Opcode` enum variant
//! 5. Execute the instruction, modifying CPU/memory/display state
//!
//! # Quirk Support
//!
//! Several opcodes have platform-dependent behavior (quirks). The `Config`
//! struct controls which behavior is active. See the config module for details.

use thiserror::Error;

use super::config::Quirks;
use super::display::Display;
use super::keypad::Keypad;
use super::memory::{Memory, MemoryError};
use super::opcode::{Opcode, OpcodeError};
use super::stack::{Stack, StackError};
use super::timers::Timers;

/// Errors that can occur during CPU execution.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CpuError {
    /// A stack operation failed (overflow or underflow).
    #[error(transparent)]
    Stack(#[from] StackError),
    /// The program counter reached an invalid memory address.
    #[error("Invalid program counter: {0:#05X}")]
    InvalidProgramCounter(u16),
    /// A memory access failed (out of bounds).
    #[error(transparent)]
    Memory(#[from] MemoryError),
    /// An opcode decode error occurred.
    #[error(transparent)]
    Opcode(#[from] OpcodeError),
}

/// Represents the current state of the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuState {
    /// The CPU is executing instructions normally.
    Running,
    /// The CPU is waiting for a key press (FX0A instruction).
    /// The u8 is the register index where the key value will be stored.
    WaitingForKey(u8),
    /// The CPU has halted (e.g., invalid opcode).
    Halted,
}

/// The Chip-8 CPU, containing registers, index register, program counter, and state.
#[derive(Debug, PartialEq, Eq)]
pub struct Cpu {
    /// General-purpose registers V0 through VF (8-bit each).
    /// VF also serves as a flag register for carry, borrow, and collision.
    pub(crate) v: [u8; 16],
    /// The index register I (16-bit), used for memory addressing.
    pub(crate) i: u16,
    /// The program counter (16-bit), points to the current instruction.
    pub(crate) pc: u16,
    /// The current state of the CPU.
    pub(crate) state: CpuState,
    /// xorshift32 PRNG state for the RND instruction.
    /// Seeded from a hash of the current time at startup.
    pub(crate) rng_state: u32,
}

impl Cpu {
    /// Creates a new CPU in its initial state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets the CPU to its initial state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns the current CPU state.
    pub fn state(&self) -> CpuState {
        self.state
    }

    /// Returns a reference to the register file.
    pub fn registers(&self) -> &[u8; 16] {
        &self.v
    }

    /// Returns the current program counter.
    pub fn program_counter(&self) -> u16 {
        self.pc
    }

    /// Returns the current index register value.
    pub fn index(&self) -> u16 {
        self.i
    }
}

impl Default for Cpu {
    fn default() -> Self {
        // Seed the PRNG from system time. Using wrapping multiplication
        // to mix the nanosecond timestamp into a non-zero seed.
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u32;
        // Ensure seed is non-zero (xorshift requires non-zero state)
        let rng_state = seed.wrapping_add(1);

        Self { v: [0; 16], i: 0, pc: 0x200, state: CpuState::Running, rng_state }
    }
}

impl Cpu {
    /// Executes one CPU cycle: fetch, decode, and execute a single instruction.
    ///
    /// This is the main entry point for driving the CPU. It should be called
    /// at the configured clock rate (default 700 Hz).
    ///
    /// The fetch-decode-execute cycle follows the standard emulator pattern:
    /// 1. Check CPU state (halted? waiting for key?)
    /// 2. Validate PC is within bounds
    /// 3. Fetch the 2-byte opcode from memory at PC
    /// 4. Advance PC by 2 (instructions that change PC override this)
    /// 5. Decode and execute the instruction
    ///
    /// # Arguments
    ///
    /// * `memory` - Mutable reference to system memory.
    /// * `keypad` - Reference to the keypad state.
    /// * `stack` - Mutable reference to the call stack.
    /// * `timers` - Mutable reference to the timers.
    /// * `display` - Mutable reference to the display.
    /// * `config` - The quirk configuration flags.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        memory: &mut Memory,
        keypad: &Keypad,
        stack: &mut Stack,
        timers: &mut Timers,
        display: &mut Display,
        quirks: Quirks,
    ) -> Result<(), CpuError> {
        match self.state {
            CpuState::Halted => return Ok(()),
            CpuState::WaitingForKey(reg) => {
                // FX0A: Block until a key is pressed. While waiting, do NOT
                // advance PC — the next tick must re-check the same condition.
                if let Some(key) = keypad.get_key_pressed() {
                    self.v[reg as usize] = key;
                    self.state = CpuState::Running;
                    // Advance PC past the Fx0A instruction. Without this,
                    // the next tick re-executes Fx0A, creating an infinite loop.
                    self.pc = self.pc.wrapping_add(2);
                }
                return Ok(());
            }
            CpuState::Running => {}
        }

        // Validate PC before fetch — reading at 0xFFF or beyond would
        // attempt a 2-byte read past the end of the 4KB address space.
        if self.pc > 0xFFE {
            return Err(CpuError::InvalidProgramCounter(self.pc));
        }

        // Fetch the 2-byte opcode word from memory at the current PC.
        let opcode_word = memory.read_word(self.pc)?;
        let opcode = Opcode::decode(opcode_word)?;

        // Advance PC by 2 (default). Instructions that change control flow
        // (JP, CALL, RET, skip instructions) override this value in execute().
        self.pc = self.pc.wrapping_add(2);

        // Decode and execute the instruction.
        self.execute(opcode, memory, keypad, stack, timers, display, quirks)?;

        Ok(())
    }

    /// Executes a decoded opcode against the current CPU state.
    ///
    /// This method implements all 35 Chip-8 instructions. Each instruction
    /// modifies the CPU state, memory, display, or stack as specified by
    /// the Chip-8 technical reference.
    ///
    /// Control flow instructions (JP, CALL, RET, skip) modify `self.pc`
    /// directly, overriding the default +2 advance from `tick()`.
    ///
    /// Arithmetic instructions that target VF (8XY4, 8XY5, 8XY7, DXyn)
    /// compute the flag value BEFORE writing the result, ensuring correct
    /// behavior even when x=0xF (the result is written first, then
    /// immediately overwritten by the flag value).
    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        opcode: Opcode,
        memory: &mut Memory,
        keypad: &Keypad,
        stack: &mut Stack,
        timers: &mut Timers,
        display: &mut Display,
        quirks: Quirks,
    ) -> Result<(), CpuError> {
        match opcode {
            // 0nnn - SYS addr (ignored on modern interpreters)
            Opcode::Sys(_addr) => {}

            // 00E0 - CLS: Clear the display
            Opcode::Cls => {
                display.clear();
            }

            // 00EE - RET: Return from subroutine
            Opcode::Ret => {
                self.pc = stack.pop()?;
            }

            // 1nnn - JP addr: Jump to address nnn
            Opcode::Jp(addr) => {
                self.pc = addr;
            }

            // 2nnn - CALL addr: Call subroutine at nnn
            Opcode::Call(addr) => {
                stack.push(self.pc)?;
                self.pc = addr;
            }

            // 3xkk - SE Vx, byte: Skip next if Vx == kk
            Opcode::SeVxByte(x, kk) => {
                if self.v[x as usize] == kk {
                    self.pc = self.pc.wrapping_add(2);
                }
            }

            // 4xkk - SNE Vx, byte: Skip next if Vx != kk
            Opcode::SneVxByte(x, kk) => {
                if self.v[x as usize] != kk {
                    self.pc = self.pc.wrapping_add(2);
                }
            }

            // 5xy0 - SE Vx, Vy: Skip next if Vx == Vy
            Opcode::SeVxVy(x, y) => {
                if self.v[x as usize] == self.v[y as usize] {
                    self.pc = self.pc.wrapping_add(2);
                }
            }

            // 6xkk - LD Vx, byte: Load kk into Vx
            Opcode::LdVxByte(x, kk) => {
                self.v[x as usize] = kk;
            }

            // 7xkk - ADD Vx, byte: Add kk to Vx (no carry flag)
            Opcode::AddVxByte(x, kk) => {
                self.v[x as usize] = self.v[x as usize].wrapping_add(kk);
            }

            // 8xy0 - LD Vx, Vy: Vx = Vy
            Opcode::LdVxVy(x, y) => {
                self.v[x as usize] = self.v[y as usize];
            }

            // 8xy1 - OR Vx, Vy: Vx |= Vy
            Opcode::OrVxVy(x, y) => {
                self.v[x as usize] |= self.v[y as usize];
                if quirks.contains(Quirks::VF_RESET) {
                    self.v[0xF] = 0;
                }
            }

            // 8xy2 - AND Vx, Vy: Vx &= Vy
            Opcode::AndVxVy(x, y) => {
                self.v[x as usize] &= self.v[y as usize];
                if quirks.contains(Quirks::VF_RESET) {
                    self.v[0xF] = 0;
                }
            }

            // 8xy3 - XOR Vx, Vy: Vx ^= Vy
            Opcode::XorVxVy(x, y) => {
                self.v[x as usize] ^= self.v[y as usize];
                if quirks.contains(Quirks::VF_RESET) {
                    self.v[0xF] = 0;
                }
            }

            // 8xy4 - ADD Vx, Vy: Vx += Vy, VF = carry
            // The carry is computed from the INPUT values before any write,
            // so even when x=0xF, VF ends up with the correct carry flag.
            Opcode::AddVxVy(x, y) => {
                let (result, carry) = self.v[x as usize].overflowing_add(self.v[y as usize]);
                self.v[x as usize] = result;
                self.v[0xF] = u8::from(carry);
            }

            // 8xy5 - SUB Vx, Vy: Vx -= Vy, VF = NOT borrow
            // VF = 1 when Vx >= Vy (no borrow needed), 0 when Vx < Vy.
            Opcode::SubVxVy(x, y) => {
                let (result, borrow) = self.v[x as usize].overflowing_sub(self.v[y as usize]);
                self.v[x as usize] = result;
                self.v[0xF] = u8::from(!borrow);
            }

            // 8xy6 - SHR Vx {, Vy}: Shift right
            // The shifted-out bit (VF) must come from the SOURCE register,
            // which differs between COSMAC VIP and Modern modes:
            // - COSMAC VIP: copies Vy to Vx, then shifts Vx → source is Vy
            // - Modern: shifts Vx in place, ignores Vy → source is Vx
            // LSB is captured BEFORE the shift to preserve the lost bit.
            Opcode::ShrVxVy(x, y) => {
                let lsb = if quirks.contains(Quirks::SHIFT_VY) {
                    // COSMAC VIP: source is Vy
                    self.v[y as usize] & 1
                } else {
                    // Modern: source is Vx
                    self.v[x as usize] & 1
                };
                if quirks.contains(Quirks::SHIFT_VY) {
                    // COSMAC VIP: Vx = Vy >> 1
                    self.v[x as usize] = self.v[y as usize] >> 1;
                } else {
                    // Modern: Vx >>= 1
                    self.v[x as usize] >>= 1;
                }
                self.v[0xF] = lsb;
            }

            // 8xy7 - SUBN Vx, Vy: Vx = Vy - Vx, VF = NOT borrow
            Opcode::SubnVxVy(x, y) => {
                let (result, borrow) = self.v[y as usize].overflowing_sub(self.v[x as usize]);
                self.v[x as usize] = result;
                self.v[0xF] = u8::from(!borrow);
            }

            // 8xyE - SHL Vx {, Vy}: Shift left
            // Same quirk logic as 8XY6 but for left shift with MSB.
            Opcode::ShlVxVy(x, y) => {
                let msb = if quirks.contains(Quirks::SHIFT_VY) {
                    // COSMAC VIP: source is Vy
                    (self.v[y as usize] >> 7) & 1
                } else {
                    // Modern: source is Vx
                    (self.v[x as usize] >> 7) & 1
                };
                if quirks.contains(Quirks::SHIFT_VY) {
                    // COSMAC VIP: Vx = Vy << 1
                    self.v[x as usize] = self.v[y as usize] << 1;
                } else {
                    // Modern: Vx <<= 1
                    self.v[x as usize] <<= 1;
                }
                self.v[0xF] = msb;
            }

            // 9xy0 - SNE Vx, Vy: Skip next if Vx != Vy
            Opcode::SneVxVy(x, y) => {
                if self.v[x as usize] != self.v[y as usize] {
                    self.pc = self.pc.wrapping_add(2);
                }
            }

            // Annn - LD I, addr: I = nnn
            Opcode::LdI(addr) => {
                self.i = addr;
            }

            // Bnnn - JP V0/Vx, addr: Jump to nnn + V0/Vx
            // COSMAC VIP always uses V0. HP48 uses Vx where x is the
            // high nibble of the 12-bit address.
            Opcode::JpV0(addr) => {
                if quirks.contains(Quirks::JUMP_VX) {
                    // HP48: Jump to nnn + Vx (where x is the high nibble)
                    let x = (addr >> 8) & 0xF;
                    self.pc = addr.wrapping_add(self.v[x as usize] as u16);
                } else {
                    // COSMAC VIP: Jump to nnn + V0
                    self.pc = addr.wrapping_add(self.v[0] as u16);
                }
            }

            // Cxkk - RND Vx, byte: Vx = random byte AND kk
            // Uses xorshift32 PRNG — deterministic after seeding, no external crate.
            Opcode::Rnd(x, kk) => {
                self.rng_state ^= self.rng_state << 13;
                self.rng_state ^= self.rng_state >> 17;
                self.rng_state ^= self.rng_state << 5;
                let random = (self.rng_state & 0xFF) as u8;
                self.v[x as usize] = random & kk;
            }

            // Dxyn - DRW Vx, Vy, n: Draw sprite at (Vx, Vy) with height n
            //
            // The DRW instruction is the most complex opcode. It:
            // 1. Reads n bytes of sprite data from memory starting at I
            // 2. Draws each bit of each byte as a pixel at (x+col, y+row)
            // 3. Uses XOR drawing: if both source and destination are ON,
            //    the destination turns OFF (collision detected)
            // 4. Starting coordinates wrap (mod 64/32), but individual
            //    sprite pixels CLIP at screen edges (no wrapping)
            // 5. n=0 means 16 rows (original COSMAC VIP behavior)
            // 6. VF is set to 1 if any pixel was erased (collision)
            Opcode::Drw(x, y, n) => {
                let sprite_x = self.v[x as usize];
                let sprite_y = self.v[y as usize];

                // Read sprite data from memory starting at I.
                // Use a stack-allocated array (max 16 rows) to avoid heap allocation.
                // n=0 means 16 rows per the original COSMAC VIP behavior.
                let rows = if n == 0 { 16 } else { n as usize };
                let mut sprite_data = [0u8; 16];
                for (row, slot) in sprite_data.iter_mut().enumerate().take(rows) {
                    let addr = self.i.wrapping_add(row as u16);
                    *slot = memory.read_byte(addr)?;
                }

                // Draw and get collision flag
                let collision = display.draw_sprite(sprite_x, sprite_y, &sprite_data[..rows]);
                self.v[0xF] = u8::from(collision);
            }

            // Ex9E - SKP Vx: Skip next if key Vx is pressed
            Opcode::Skp(x) => {
                if keypad.is_key_pressed(self.v[x as usize]) {
                    self.pc = self.pc.wrapping_add(2);
                }
            }

            // ExA1 - SKNP Vx: Skip next if key Vx is NOT pressed
            Opcode::Sknp(x) => {
                if !keypad.is_key_pressed(self.v[x as usize]) {
                    self.pc = self.pc.wrapping_add(2);
                }
            }

            // Fx07 - LD Vx, DT: Load delay timer into Vx
            Opcode::LdVxDt(x) => {
                self.v[x as usize] = timers.delay;
            }

            // Fx0A - LD Vx, K: Wait for key press, store in Vx
            // Sets state to WaitingForKey; the tick() method handles the blocking.
            Opcode::LdVxK(x) => {
                self.state = CpuState::WaitingForKey(x);
            }

            // Fx15 - LD DT, Vx: Set delay timer to Vx
            Opcode::LdDtVx(x) => {
                timers.delay = self.v[x as usize];
            }

            // Fx18 - LD ST, Vx: Set sound timer to Vx
            Opcode::LdStVx(x) => {
                timers.sound = self.v[x as usize];
            }

            // Fx1E - ADD I, Vx: I += Vx
            Opcode::AddIVx(x) => {
                self.i = self.i.wrapping_add(self.v[x as usize] as u16);
            }

            // Fx29 - LD F, Vx: Set I to font sprite address for digit Vx
            // Font sprites are 5 bytes each, starting at 0x050.
            Opcode::LdF(x) => {
                self.i = 0x050 + (self.v[x as usize] as u16) * 5;
            }

            // Fx33 - LD B, Vx: Store BCD representation of Vx at I, I+1, I+2
            // Hundreds digit at I, tens at I+1, ones at I+2.
            // Example: Vx=156 → I[0]=1, I[1]=5, I[2]=6
            Opcode::LdB(x) => {
                let val = self.v[x as usize];
                memory.write_byte(self.i, val / 100)?;
                memory.write_byte(self.i + 1, (val / 10) % 10)?;
                memory.write_byte(self.i + 2, val % 10)?;
            }

            // Fx55 - LD [I], Vx: Store V0..Vx into memory starting at I
            // On the original COSMAC VIP, I is incremented by X + 1 after
            // the operation (I = I + X + 1). Modern interpreters leave I unchanged.
            Opcode::LdIVx(x) => {
                for offset in 0..=x {
                    memory.write_byte(self.i + offset as u16, self.v[offset as usize])?;
                }
                if quirks.contains(Quirks::I_INCREMENT) {
                    self.i = self.i.wrapping_add(x as u16 + 1);
                }
            }

            // Fx65 - LD Vx, [I]: Load V0..Vx from memory starting at I
            // Same I increment quirk as Fx55: COSMAC VIP advances I, modern doesn't.
            Opcode::LdVxI(x) => {
                for offset in 0..=x {
                    self.v[offset as usize] = memory.read_byte(self.i + offset as u16)?;
                }
                if quirks.contains(Quirks::I_INCREMENT) {
                    self.i = self.i.wrapping_add(x as u16 + 1);
                }
            }
        }

        Ok(())
    }
}

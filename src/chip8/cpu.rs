//! Chip-8 CPU
//!
//! The CPU is the heart of the Chip-8 virtual machine. It contains:
//! - 16 general-purpose 8-bit registers (V0–VF, where VF is the flag register)
//! - A 16-bit index register (I) for memory addressing
//! - A 16-bit program counter (PC), initialized to PROGRAM_START (0x200)
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

/// The Chip-8 font sprites are stored starting at this address.
const FONT_ADDR: u16 = 0x050;
/// The program counter starts at this address where programs are loaded.
const PROGRAM_START: u16 = 0x200;
/// Maximum valid program counter value (must leave room for a 2-byte opcode).
const MAX_PC: u16 = 0xFFE;

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
    /// The CPU is waiting for a previously-pressed key to be released
    /// (KEY_RELEASE quirk, COSMAC VIP).
    /// The first u8 is the register index, the second is the key already
    /// observed. The instruction only completes once that key is released.
    WaitingForKeyRelease(u8, u8),
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

        Self { v: [0; 16], i: 0, pc: PROGRAM_START, state: CpuState::Running, rng_state }
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
                    if quirks.contains(Quirks::KEY_RELEASE) {
                        // Stay in the wait cycle until the key we just saw
                        // is released. PC still does not advance.
                        self.state = CpuState::WaitingForKeyRelease(reg, key);
                    } else {
                        self.v[reg as usize] = key;
                        self.state = CpuState::Running;
                        // Advance PC past the Fx0A instruction. Without
                        // this, the next tick re-executes Fx0A, creating
                        // an infinite loop.
                        self.pc = self.pc.wrapping_add(2);
                    }
                }
                return Ok(());
            }
            CpuState::WaitingForKeyRelease(reg, key) => {
                // FX0A + KEY_RELEASE: complete the instruction only once
                // the previously-observed key has been released.
                if !keypad.is_key_pressed(key) {
                    self.v[reg as usize] = key;
                    self.state = CpuState::Running;
                    self.pc = self.pc.wrapping_add(2);
                }
                return Ok(());
            }
            CpuState::Running => {}
        }

        // Validate PC before fetch — reading at 0xFFF or beyond would
        // attempt a 2-byte read past the end of the 4KB address space.
        if self.pc > MAX_PC {
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
            Opcode::ShrVxVy(x, y) => self.execute_shr(x, y, quirks),

            // 8xy7 - SUBN Vx, Vy: Vx = Vy - Vx, VF = NOT borrow
            Opcode::SubnVxVy(x, y) => {
                let (result, borrow) = self.v[y as usize].overflowing_sub(self.v[x as usize]);
                self.v[x as usize] = result;
                self.v[0xF] = u8::from(!borrow);
            }

            // 8xyE - SHL Vx {, Vy}: Shift left
            Opcode::ShlVxVy(x, y) => self.execute_shl(x, y, quirks),

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

            // Bnnn - JP V0/Vx, addr: Jump to nnn + V0 or Vx (HP48 quirk)
            Opcode::JpV0(addr) => self.execute_jump_v0(addr, quirks),

            // Cxkk - RND Vx, byte: Vx = random byte AND kk
            Opcode::Rnd(x, kk) => self.execute_rnd(x, kk),

            // Dxyn - DRW Vx, Vy, n: Draw sprite at (Vx, Vy)
            Opcode::Drw(x, y, n) => self.execute_sprite(x, y, n, memory, display, quirks)?,

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
            // With the KEY_RELEASE quirk (COSMAC VIP), a press → release cycle
            // is required; only then is the key stored in Vx.
            Opcode::LdVxK(x) => {
                if quirks.contains(Quirks::KEY_RELEASE) {
                    if let Some(key) = keypad.get_key_pressed() {
                        // Key already held: wait for it to be released before
                        // accepting a new key event in the next phase.
                        self.state = CpuState::WaitingForKeyRelease(x, key);
                    } else {
                        self.state = CpuState::WaitingForKey(x);
                    }
                } else {
                    self.state = CpuState::WaitingForKey(x);
                }
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
            // With the I_OVERFLOW_VF quirk, VF=1 is set when the resulting I
            // would exceed the 12-bit address boundary (0xFFF).
            Opcode::AddIVx(x) => {
                let vx = self.v[x as usize] as u16;
                let result = self.i.wrapping_add(vx);
                if quirks.contains(Quirks::I_OVERFLOW_VF) && result > 0xFFF {
                    self.v[0xF] = 1;
                }
                self.i = result;
            }

            // Fx29 - LD F, Vx: Set I to font sprite address for digit Vx
            // Font sprites are 5 bytes each, starting at FONT_ADDR.
            Opcode::LdF(x) => {
                self.i = FONT_ADDR + (self.v[x as usize] as u16) * 5;
            }

            // Fx33 - LD B, Vx: Store BCD representation
            Opcode::LdB(x) => self.execute_bcd(x, memory)?,

            // Fx55 - LD [I], Vx: Store V0..Vx into memory starting at I
            Opcode::LdIVx(x) => self.execute_store_regs(x, memory, quirks)?,

            // Fx65 - LD Vx, [I]: Load V0..Vx from memory starting at I
            Opcode::LdVxI(x) => self.execute_load_regs(x, memory, quirks)?,
        }

        Ok(())
    }

    /// Handles the Bnnn jump instruction (JP V0/Vx, addr).
    ///
    /// COSMAC VIP always adds V0 to the target address.
    /// HP48 uses Vx where x is the high nibble of the 12-bit address.
    fn execute_jump_v0(&mut self, addr: u16, quirks: Quirks) {
        if quirks.contains(Quirks::JUMP_VX) {
            let x = (addr >> 8) & 0xF;
            self.pc = addr.wrapping_add(self.v[x as usize] as u16);
        } else {
            self.pc = addr.wrapping_add(self.v[0] as u16);
        }
    }

    /// Implements the RND instruction (Cxkk): Vx = random & kk.
    fn execute_rnd(&mut self, x: u8, kk: u8) {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        let random = (self.rng_state & 0xFF) as u8;
        self.v[x as usize] = random & kk;
    }

    /// Shift-right for 8XY6 with quirk support.
    ///
    /// - COSMAC VIP: copies Vy to Vx before shifting (source = Vy)
    /// - Modern: shifts Vx in place (source = Vx)
    fn execute_shr(&mut self, x: u8, y: u8, quirks: Quirks) {
        let lsb = if quirks.contains(Quirks::SHIFT_VY) {
            self.v[y as usize] & 1
        } else {
            self.v[x as usize] & 1
        };
        if quirks.contains(Quirks::SHIFT_VY) {
            self.v[x as usize] = self.v[y as usize] >> 1;
        } else {
            self.v[x as usize] >>= 1;
        }
        self.v[0xF] = lsb;
    }

    /// Shift-left for 8XYE with quirk support.
    ///
    /// - COSMAC VIP: copies Vy to Vx before shifting (source = Vy)
    /// - Modern: shifts Vx in place (source = Vx)
    fn execute_shl(&mut self, x: u8, y: u8, quirks: Quirks) {
        let msb = if quirks.contains(Quirks::SHIFT_VY) {
            (self.v[y as usize] >> 7) & 1
        } else {
            (self.v[x as usize] >> 7) & 1
        };
        if quirks.contains(Quirks::SHIFT_VY) {
            self.v[x as usize] = self.v[y as usize] << 1;
        } else {
            self.v[x as usize] <<= 1;
        }
        self.v[0xF] = msb;
    }

    /// Draw a sprite (DRW instruction).
    #[allow(clippy::too_many_arguments)]
    fn execute_sprite(
        &mut self,
        x: u8,
        y: u8,
        n: u8,
        memory: &Memory,
        display: &mut Display,
        quirks: Quirks,
    ) -> Result<(), CpuError> {
        let sprite_x = self.v[x as usize];
        let sprite_y = self.v[y as usize];
        let rows = if n == 0 { 16 } else { n as usize };
        let mut sprite_data = [0u8; 16];
        for (row, slot) in sprite_data.iter_mut().enumerate().take(rows) {
            let addr = self.i.wrapping_add(row as u16);
            *slot = memory.read_byte(addr)?;
        }
        let collision = display.draw_sprite(
            sprite_x,
            sprite_y,
            &sprite_data[..rows],
            quirks.contains(Quirks::SPRITE_WRAP),
        );
        self.v[0xF] = u8::from(collision);
        Ok(())
    }

    /// Store BCD representation of Vx at I, I+1, I+2.
    fn execute_bcd(&mut self, x: u8, memory: &mut Memory) -> Result<(), CpuError> {
        let val = self.v[x as usize];
        memory.write_byte(self.i, val / 100)?;
        memory.write_byte(self.i + 1, (val / 10) % 10)?;
        memory.write_byte(self.i + 2, val % 10)?;
        Ok(())
    }

    /// Store V0..Vx into memory starting at I (Fx55).
    fn execute_store_regs(
        &mut self,
        x: u8,
        memory: &mut Memory,
        quirks: Quirks,
    ) -> Result<(), CpuError> {
        for offset in 0..=x {
            memory.write_byte(self.i + offset as u16, self.v[offset as usize])?;
        }
        if quirks.contains(Quirks::I_INCREMENT) {
            self.i = self.i.wrapping_add(x as u16 + 1);
        }
        Ok(())
    }

    /// Load V0..Vx from memory starting at I (Fx65).
    fn execute_load_regs(
        &mut self,
        x: u8,
        memory: &Memory,
        quirks: Quirks,
    ) -> Result<(), CpuError> {
        for offset in 0..=x {
            self.v[offset as usize] = memory.read_byte(self.i + offset as u16)?;
        }
        if quirks.contains(Quirks::I_INCREMENT) {
            self.i = self.i.wrapping_add(x as u16 + 1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create all peripheral structs for testing.
    fn peripherals() -> (Memory, Keypad, Stack, Timers, Display) {
        (Memory::new(), Keypad::new(), Stack::new(), Timers::new(), Display::new())
    }

    // System & Display

    #[test]
    fn test_sys() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // SYS is a no-op on modern interpreters
        cpu.execute(
            Opcode::Sys(0x200),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, PROGRAM_START, "SYS should not change PC");
    }

    #[test]
    fn test_cls() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Draw a pixel first so display is non-empty
        let sprite = [0xFFu8];
        display.draw_sprite(0, 0, &sprite, false);
        assert!(display.get_pixels().iter().any(|&p| p), "Display should have pixels after draw");
        cpu.execute(
            Opcode::Cls,
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert!(display.get_pixels().iter().all(|&p| !p), "CLS should clear all pixels");
    }

    #[test]
    fn test_ret() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Push a return address onto the stack, then RET should jump to it
        stack.push(0x300).unwrap();
        cpu.execute(
            Opcode::Ret,
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, 0x300, "RET should pop 0x300 from stack into PC");
    }

    // Jumps & Calls

    #[test]
    fn test_jp() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.execute(
            Opcode::Jp(0x400),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, 0x400, "JP should set PC to 0x400");
    }

    #[test]
    fn test_call() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.execute(
            Opcode::Call(0x500),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, 0x500, "CALL should set PC to 0x500");
        assert_eq!(stack.pop().unwrap(), old_pc, "CALL should push old PC onto stack");
    }

    #[test]
    fn test_jp_v0_modern() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0] = 0x10;
        cpu.execute(
            Opcode::JpV0(0x200),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, 0x210, "JP V0 with addr=0x200, V0=0x10 should give 0x210");
    }

    #[test]
    fn test_jp_vx_hp48() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // HP48 quirk: Bnnn uses Vx where x is the high nibble of addr
        // For addr=0x230, x=2, so PC = 0x230 + V2
        cpu.v[2] = 0x50;
        cpu.execute(
            Opcode::JpV0(0x230),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::hp48(),
        )
        .unwrap();
        assert_eq!(cpu.pc, 0x280, "JP Vx HP48: addr=0x230, V2=0x50 should give 0x280");
    }

    // Skips

    #[test]
    fn test_se_vx_byte_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0xA] = 0x42;
        cpu.execute(
            Opcode::SeVxByte(0xA, 0x42),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        // execute() only adds the skip increment (+2); tick() adds the normal +2
        assert_eq!(cpu.pc, old_pc + 2, "SE Vx,byte: equal → skip (+2 from execute)");
    }

    #[test]
    fn test_se_vx_byte_not_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0xA] = 0x42;
        cpu.execute(
            Opcode::SeVxByte(0xA, 0x43),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        // execute() only adds +2 on skip; tick() adds the normal +2
        assert_eq!(cpu.pc, old_pc, "SE Vx,byte: not equal → no skip (PC unchanged from execute)");
    }

    #[test]
    fn test_sne_vx_byte_not_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0x1] = 0xFF;
        cpu.execute(
            Opcode::SneVxByte(0x1, 0x00),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc + 2, "SNE Vx,byte: not equal → skip (+2 from execute)");
    }

    #[test]
    fn test_sne_vx_byte_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0x1] = 0xFF;
        cpu.execute(
            Opcode::SneVxByte(0x1, 0xFF),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc, "SNE Vx,byte: equal → no skip (PC unchanged from execute)");
    }

    #[test]
    fn test_se_vx_vy_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0x2] = 0x77;
        cpu.v[0x3] = 0x77;
        cpu.execute(
            Opcode::SeVxVy(0x2, 0x3),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc + 2, "SE Vx,Vy: equal → skip (+2 from execute)");
    }

    #[test]
    fn test_se_vx_vy_not_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0x2] = 0x77;
        cpu.v[0x3] = 0x78;
        cpu.execute(
            Opcode::SeVxVy(0x2, 0x3),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc, "SE Vx,Vy: not equal → no skip (PC unchanged from execute)");
    }

    #[test]
    fn test_sne_vx_vy_not_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0x4] = 0x11;
        cpu.v[0x5] = 0x22;
        cpu.execute(
            Opcode::SneVxVy(0x4, 0x5),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc + 2, "SNE Vx,Vy: not equal → skip (+2 from execute)");
    }

    #[test]
    fn test_sne_vx_vy_equal() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        let old_pc = cpu.pc;
        cpu.v[0x4] = 0x11;
        cpu.v[0x5] = 0x11;
        cpu.execute(
            Opcode::SneVxVy(0x4, 0x5),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc, "SNE Vx,Vy: equal → no skip (PC unchanged from execute)");
    }

    // Load Immediate

    #[test]
    fn test_ld_vx_byte() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.execute(
            Opcode::LdVxByte(0xB, 0xAB),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xB], 0xAB, "LD Vx,byte should set V[0xB] = 0xAB");
    }

    #[test]
    fn test_add_vx_byte_no_carry() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xC] = 10;
        cpu.execute(
            Opcode::AddVxByte(0xC, 20),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xC], 30, "ADD Vx,byte: 10 + 20 = 30");
    }

    #[test]
    fn test_add_vx_byte_wrapping() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xC] = 0xFF;
        cpu.execute(
            Opcode::AddVxByte(0xC, 1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xC], 0, "ADD Vx,byte: 0xFF + 1 wraps to 0");
    }

    #[test]
    fn test_ld_i() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.execute(
            Opcode::LdI(0xABC),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.i, 0xABC, "LD I,addr should set I = 0xABC");
    }

    #[test]
    fn test_ld_f() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Font sprites start at FONT_ADDR = 0x050, each digit sprite is 5 bytes.
        // Digit 0 is at 0x050, digit 1 is at 0x055, etc.
        // Set V5 = 5 so that LdF loads the address for digit 5
        cpu.v[5] = 5;
        cpu.execute(
            Opcode::LdF(5),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.i, 0x050 + 5 * 5, "LD F,Vx: I should point to sprite for digit 5");
    }

    // Register Operations

    #[test]
    fn test_ld_vx_vy() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x1] = 0x99;
        cpu.execute(
            Opcode::LdVxVy(0x2, 0x1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x2], 0x99, "LD Vx,Vy: Vx should get Vy's value");
    }

    #[test]
    fn test_or_vx_vy() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x1] = 0xA0;
        cpu.v[0x2] = 0x0B;
        cpu.execute(
            Opcode::OrVxVy(0x1, 0x2),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x1], 0xAB, "OR Vx,Vy: 0xA0 | 0x0B = 0xAB");
    }

    #[test]
    fn test_or_vx_vy_vf_reset_quirk() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xF] = 0xFF; // preset VF
        cpu.v[0x1] = 0xF0;
        cpu.v[0x2] = 0x0F;
        cpu.execute(
            Opcode::OrVxVy(0x1, 0x2),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::cosmac_vip(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xF], 0, "OR with COSMAC VIP: VF should be reset to 0");
    }

    #[test]
    fn test_and_vx_vy() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x3] = 0xFF;
        cpu.v[0x4] = 0x0F;
        cpu.execute(
            Opcode::AndVxVy(0x3, 0x4),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x3], 0x0F, "AND Vx,Vy: 0xFF & 0x0F = 0x0F");
    }

    #[test]
    fn test_and_vx_vy_vf_reset_quirk() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xF] = 0x01;
        cpu.v[0x3] = 0xF0;
        cpu.v[0x4] = 0x0F;
        cpu.execute(
            Opcode::AndVxVy(0x3, 0x4),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::cosmac_vip(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xF], 0, "AND with COSMAC VIP: VF should be reset to 0");
    }

    #[test]
    fn test_xor_vx_vy() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x5] = 0xFF;
        cpu.v[0x6] = 0x0F;
        cpu.execute(
            Opcode::XorVxVy(0x5, 0x6),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x5], 0xF0, "XOR Vx,Vy: 0xFF ^ 0x0F = 0xF0");
    }

    #[test]
    fn test_xor_vx_vy_vf_reset_quirk() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xF] = 0x42;
        cpu.v[0x5] = 0xAA;
        cpu.v[0x6] = 0x55;
        cpu.execute(
            Opcode::XorVxVy(0x5, 0x6),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::cosmac_vip(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xF], 0, "XOR with COSMAC VIP: VF should be reset to 0");
    }

    #[test]
    fn test_add_vx_vy_no_carry() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x7] = 0x10;
        cpu.v[0x8] = 0x20;
        cpu.execute(
            Opcode::AddVxVy(0x7, 0x8),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x7], 0x30, "ADD Vx,Vy: 0x10 + 0x20 = 0x30");
        assert_eq!(cpu.v[0xF], 0, "ADD Vx,Vy: no carry, VF = 0");
    }

    #[test]
    fn test_add_vx_vy_carry() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x7] = 0xFF;
        cpu.v[0x8] = 0x01;
        cpu.execute(
            Opcode::AddVxVy(0x7, 0x8),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x7], 0x00, "ADD Vx,Vy: 0xFF + 0x01 wraps to 0x00");
        assert_eq!(cpu.v[0xF], 1, "ADD Vx,Vy: carry, VF = 1");
    }

    #[test]
    fn test_sub_vx_vy_no_borrow() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x9] = 0x50;
        cpu.v[0xA] = 0x30;
        cpu.execute(
            Opcode::SubVxVy(0x9, 0xA),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x9], 0x20, "SUB Vx,Vy: 0x50 - 0x30 = 0x20");
        assert_eq!(cpu.v[0xF], 1, "SUB Vx,Vy: no borrow, VF = 1 (Vx >= Vy)");
    }

    #[test]
    fn test_sub_vx_vy_borrow() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0x9] = 0x10;
        cpu.v[0xA] = 0x30;
        cpu.execute(
            Opcode::SubVxVy(0x9, 0xA),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x9], 0xE0, "SUB Vx,Vy: 0x10 - 0x30 = 0xE0 (borrow)");
        assert_eq!(cpu.v[0xF], 0, "SUB Vx,Vy: borrow, VF = 0");
    }

    #[test]
    fn test_subn_vx_vy_no_borrow() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xB] = 0x20;
        cpu.v[0xC] = 0x50;
        cpu.execute(
            Opcode::SubnVxVy(0xB, 0xC),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xB], 0x30, "SUBN Vx,Vy: 0x50 - 0x20 = 0x30");
        assert_eq!(cpu.v[0xF], 1, "SUBN Vx,Vy: no borrow, VF = 1 (Vy >= Vx)");
    }

    #[test]
    fn test_subn_vx_vy_borrow() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xB] = 0x50;
        cpu.v[0xC] = 0x20;
        cpu.execute(
            Opcode::SubnVxVy(0xB, 0xC),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xB], 0xD0, "SUBN Vx,Vy: 0x20 - 0x50 = 0xD0 (borrow)");
        assert_eq!(cpu.v[0xF], 0, "SUBN Vx,Vy: borrow, VF = 0");
    }

    #[test]
    fn test_shr_modern() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xD] = 0x0F; // odd: LSB = 1
        cpu.execute(
            Opcode::ShrVxVy(0xD, 0xE),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xD], 0x07, "SHR modern: 0x0F >> 1 = 0x07");
        assert_eq!(cpu.v[0xF], 1, "SHR modern: LSB = 1 → VF = 1");
    }

    #[test]
    fn test_shr_cosmac() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xD] = 0xFF;
        cpu.v[0xE] = 0x05; // Vy source: LSB = 1
        cpu.execute(
            Opcode::ShrVxVy(0xD, 0xE),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::cosmac_vip(),
        )
        .unwrap();
        // COSMAC VIP: Vx = Vy >> 1
        assert_eq!(cpu.v[0xD], 0x02, "SHR cosmac: Vy(0x05) >> 1 = 0x02");
        assert_eq!(cpu.v[0xF], 1, "SHR cosmac: LSB of Vy(0x05) = 1 → VF = 1");
    }

    #[test]
    fn test_shl_modern() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xD] = 0x80; // MSB = 1
        cpu.execute(
            Opcode::ShlVxVy(0xD, 0xE),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xD], 0x00, "SHL modern: 0x80 << 1 = 0x00 (overflow)");
        assert_eq!(cpu.v[0xF], 1, "SHL modern: MSB = 1 → VF = 1");
    }

    #[test]
    fn test_shl_cosmac() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[0xD] = 0x00;
        cpu.v[0xE] = 0x81; // Vy source: MSB = 1
        cpu.execute(
            Opcode::ShlVxVy(0xD, 0xE),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::cosmac_vip(),
        )
        .unwrap();
        // COSMAC VIP: Vx = Vy << 1
        assert_eq!(cpu.v[0xD], 0x02, "SHL cosmac: Vy(0x81) << 1 = 0x02");
        assert_eq!(cpu.v[0xF], 1, "SHL cosmac: MSB of Vy(0x81) = 1 → VF = 1");
    }

    // Random

    #[test]
    fn test_rnd() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // The PRNG is deterministic. With mask 0xFF, Vx gets the raw random byte.
        cpu.execute(
            Opcode::Rnd(0, 0xFF),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        // Can't test exact value (random), but verify the result is masked
        // With mask 0x00, Vx should always be 0
        cpu.execute(
            Opcode::Rnd(1, 0x00),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0x1], 0, "RND Vx,0x00: result should be 0");
        // Determinism tested via RNG state: two sequential calls with the same
        // seed produce the same sequence, but each Cpu::new() generates a
        // unique seed from system time. The masking behavior is what we test.
    }

    // Display

    #[test]
    fn test_drw_no_collision() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Draw a sprite that doesn't collide with existing pixels
        // Write sprite data to memory at I = 0x300
        cpu.i = 0x300;
        mem.write_byte(0x300, 0xFF).unwrap();
        cpu.v[0] = 10; // x
        cpu.v[0x1] = 20; // y
        cpu.execute(
            Opcode::Drw(0, 1, 1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        // VF should be 0 (no collision) since display was empty
        assert_eq!(cpu.v[0xF], 0, "DRW: no collision, VF = 0");
        // Check some pixels are on at the sprite location
        assert!(display.get_pixels()[20 * 64 + 10], "DRW: pixel at (10,20) should be set");
    }

    #[test]
    fn test_drw_collision() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Draw the same sprite twice at the same position → collision
        cpu.i = 0x300;
        mem.write_byte(0x300, 0xFF).unwrap();
        cpu.v[0] = 5;
        cpu.v[0x1] = 5;
        // First draw: no collision
        cpu.execute(
            Opcode::Drw(0, 1, 1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xF], 0, "DRW first draw: no collision");
        // Second draw at same position: pixels will be XOR'd back, collision
        cpu.execute(
            Opcode::Drw(0, 1, 1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xF], 1, "DRW second draw: XOR collision, VF = 1");
    }

    #[test]
    fn test_drw_n_zero() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // n = 0 means 16 rows (COSMAC VIP behavior)
        cpu.i = 0x300;
        for row in 0..16 {
            mem.write_byte(0x300 + row as u16, 0xFF).unwrap();
        }
        cpu.v[0] = 0;
        cpu.v[1] = 0;
        cpu.execute(
            Opcode::Drw(0, 1, 0),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        // Pixel at (0, 15) should be set (16th row)
        assert!(display.get_pixels()[15 * 64], "DRW n=0: row 15 should be drawn");
    }

    // Keyboard

    #[test]
    fn test_skp_pressed() {
        let mut cpu = Cpu::new();
        let mut keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        let old_pc = cpu.pc;
        // Press key 0x7
        keypad.set_key_pressed(0x7, true);
        cpu.v[0x0] = 0x7;
        cpu.execute(
            Opcode::Skp(0x0),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc + 2, "SKP: key pressed → skip (+2 from execute)");
    }

    #[test]
    fn test_skp_not_pressed() {
        let mut cpu = Cpu::new();
        let keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        let old_pc = cpu.pc;
        cpu.v[0x0] = 0x7;
        // Key not pressed
        cpu.execute(
            Opcode::Skp(0x0),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc, "SKP: key NOT pressed → no skip (PC unchanged from execute)");
    }

    #[test]
    fn test_sknp_not_pressed() {
        let mut cpu = Cpu::new();
        let keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        let old_pc = cpu.pc;
        cpu.v[0x0] = 0xA;
        // Key not pressed
        cpu.execute(
            Opcode::Sknp(0x0),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc + 2, "SKNP: key NOT pressed → skip (+2 from execute)");
    }

    #[test]
    fn test_sknp_pressed() {
        let mut cpu = Cpu::new();
        let mut keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        let old_pc = cpu.pc;
        keypad.set_key_pressed(0xA, true);
        cpu.v[0x0] = 0xA;
        cpu.execute(
            Opcode::Sknp(0x0),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, old_pc, "SKNP: key pressed → no skip (PC unchanged from execute)");
    }

    #[test]
    fn test_ld_vx_k() {
        let mut cpu = Cpu::new();
        let mut keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        // LdVxK sets the CPU state to WaitingForKey (does not store key yet)
        cpu.execute(
            Opcode::LdVxK(0x3),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(
            cpu.state(),
            CpuState::WaitingForKey(0x3),
            "LdVxK should set state to WaitingForKey(3)"
        );
        // Press a key and tick() processes the waiting state
        keypad.set_key_pressed(0x5, true);
        cpu.tick(&mut mem, &keypad, &mut stack, &mut timers, &mut display, Quirks::modern())
            .unwrap();
        assert_eq!(cpu.v[0x3], 0x5, "LD Vx,K: Vx should store pressed key 0x5 after tick");
        assert_eq!(cpu.state(), CpuState::Running, "After key press, CPU should be Running");
    }

    // Timers

    #[test]
    fn test_ld_vx_dt() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        timers.delay = 0x3C;
        cpu.execute(
            Opcode::LdVxDt(4),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[4], 0x3C, "LD Vx,DT: Vx should read delay timer");
    }

    #[test]
    fn test_ld_dt_vx() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[5] = 0x50;
        cpu.execute(
            Opcode::LdDtVx(5),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(timers.delay, 0x50, "LD DT,Vx: delay timer should get Vx value");
    }

    #[test]
    fn test_ld_st_vx() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.v[6] = 0x77;
        cpu.execute(
            Opcode::LdStVx(6),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(timers.sound, 0x77, "LD ST,Vx: sound timer should get Vx value");
    }

    // Index Register Operations

    #[test]
    fn test_add_i_vx() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.i = 0x200;
        cpu.v[7] = 0x30;
        cpu.execute(
            Opcode::AddIVx(7),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.i, 0x230, "ADD I,Vx: I(0x200) + V7(0x30) = 0x230");
    }

    #[test]
    fn test_ld_b() {
        let mut cpu = Cpu::new();
        let mut mem = Memory::new();
        let (keypad, mut stack, mut timers, mut display) =
            (Keypad::new(), Stack::new(), Timers::new(), Display::new());
        cpu.i = 0x300;
        cpu.v[8] = 156;
        cpu.execute(
            Opcode::LdB(8),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(mem.read_byte(0x300).unwrap(), 1, "BCD: hundreds digit of 156 = 1");
        assert_eq!(mem.read_byte(0x301).unwrap(), 5, "BCD: tens digit of 156 = 5");
        assert_eq!(mem.read_byte(0x302).unwrap(), 6, "BCD: ones digit of 156 = 6");
    }

    #[test]
    fn test_ld_b_zero() {
        let mut cpu = Cpu::new();
        let mut mem = Memory::new();
        let (keypad, mut stack, mut timers, mut display) =
            (Keypad::new(), Stack::new(), Timers::new(), Display::new());
        cpu.i = 0x300;
        cpu.v[8] = 0;
        cpu.execute(
            Opcode::LdB(8),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(mem.read_byte(0x300).unwrap(), 0, "BCD: 0 → hundreds = 0");
        assert_eq!(mem.read_byte(0x301).unwrap(), 0, "BCD: 0 → tens = 0");
        assert_eq!(mem.read_byte(0x302).unwrap(), 0, "BCD: 0 → ones = 0");
    }

    #[test]
    fn test_ld_i_vx() {
        let mut cpu = Cpu::new();
        let mut mem = Memory::new();
        let (keypad, mut stack, mut timers, mut display) =
            (Keypad::new(), Stack::new(), Timers::new(), Display::new());
        cpu.i = 0x400;
        cpu.v[0] = 0x11;
        cpu.v[1] = 0x22;
        cpu.v[2] = 0x33;
        // Store V0..V2 into memory starting at I
        cpu.execute(
            Opcode::LdIVx(2),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(mem.read_byte(0x400).unwrap(), 0x11, "LD [I],Vx: I[0] = V0");
        assert_eq!(mem.read_byte(0x401).unwrap(), 0x22, "LD [I],Vx: I[1] = V1");
        assert_eq!(mem.read_byte(0x402).unwrap(), 0x33, "LD [I],Vx: I[2] = V2");
        // Modern: I should NOT be incremented
        assert_eq!(cpu.i, 0x400, "LD [I],Vx modern: I should remain unchanged");
    }

    #[test]
    fn test_ld_i_vx_i_increment_quirk() {
        let mut cpu = Cpu::new();
        let mut mem = Memory::new();
        let (keypad, mut stack, mut timers, mut display) =
            (Keypad::new(), Stack::new(), Timers::new(), Display::new());
        cpu.i = 0x400;
        cpu.v[0] = 0xAA;
        cpu.v[1] = 0xBB;
        cpu.execute(
            Opcode::LdIVx(1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::cosmac_vip(),
        )
        .unwrap();
        // COSMAC VIP: I += X + 1 (where X is the register index)
        assert_eq!(cpu.i, 0x402, "LD [I],Vx cosmac: I should increment by 2 (X=1, so X+1=2)");
    }

    #[test]
    fn test_ld_vx_i() {
        let mut cpu = Cpu::new();
        let mut mem = Memory::new();
        let (keypad, mut stack, mut timers, mut display) =
            (Keypad::new(), Stack::new(), Timers::new(), Display::new());
        cpu.i = 0x400;
        mem.write_byte(0x400, 0x99).unwrap();
        mem.write_byte(0x401, 0x88).unwrap();
        mem.write_byte(0x402, 0x77).unwrap();
        // Load V0..V2 from memory starting at I
        cpu.execute(
            Opcode::LdVxI(2),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0], 0x99, "LD Vx,[I]: V0 = I[0]");
        assert_eq!(cpu.v[1], 0x88, "LD Vx,[I]: V1 = I[1]");
        assert_eq!(cpu.v[2], 0x77, "LD Vx,[I]: V2 = I[2]");
        // Modern: I should NOT be incremented
        assert_eq!(cpu.i, 0x400, "LD Vx,[I] modern: I should remain unchanged");
    }

    #[test]
    fn test_ld_vx_i_i_increment_quirk() {
        let mut cpu = Cpu::new();
        let mut mem = Memory::new();
        let (keypad, mut stack, mut timers, mut display) =
            (Keypad::new(), Stack::new(), Timers::new(), Display::new());
        cpu.i = 0x300;
        mem.write_byte(0x300, 0x01).unwrap();
        mem.write_byte(0x301, 0x02).unwrap();
        cpu.execute(
            Opcode::LdVxI(1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::cosmac_vip(),
        )
        .unwrap();
        // COSMAC VIP: I += X + 1
        assert_eq!(cpu.i, 0x302, "LD Vx,[I] cosmac: I should increment by 2");
    }

    // Edge Cases & Boundary Conditions

    #[test]
    fn test_stack_overflow() {
        let _cpu = Cpu::new();
        let (_mem, _keypad, mut stack, _timers, _display) = peripherals();
        // Fill the stack (16 levels)
        for _ in 0..16 {
            stack.push(0x200).unwrap();
        }
        // 17th push should overflow
        assert!(stack.push(0x300).is_err(), "Stack should overflow after 16 pushes");
    }

    #[test]
    fn test_stack_underflow() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Empty stack pop should fail (underflow)
        let result = cpu.execute(
            Opcode::Ret,
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        );
        assert!(result.is_err(), "RET on empty stack should return error");
    }

    #[test]
    fn test_jp_out_of_bounds() {
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Jumping to 0xFFF is allowed (PC can be set there), but ticking will
        // fail because the next opcode fetch would be out of bounds.
        cpu.execute(
            Opcode::Jp(0xABC),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.pc, 0xABC, "JP should set PC to any valid address");
    }

    #[test]
    fn test_write_rom_overflow() {
        // Direct test of write_rom with oversized data
        let mut mem = Memory::new();
        let rom = vec![0u8; 4000]; // > 4096
        let result = mem.write_rom(&rom);
        assert!(result.is_err(), "write_rom with 4000 bytes should fail");
    }

    #[test]
    fn test_memory_read_out_of_bounds() {
        let mem = Memory::new();
        let result = mem.read_byte(0xFFFF);
        assert!(result.is_err(), "Reading beyond memory should fail");
    }

    #[test]
    fn test_memory_write_out_of_bounds() {
        let mut mem = Memory::new();
        let result = mem.write_byte(0xFFFF, 0x00);
        assert!(result.is_err(), "Writing beyond memory should fail");
    }

    #[test]
    fn test_vf_as_flag_and_register() {
        // VF is both a general-purpose register AND the flag register.
        // Some opcodes modify VF (carry, borrow, collision), but VF can
        // also be read/written as a normal register via LD Vx,Vy, etc.
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        // Write to VF via LdVxByte
        cpu.execute(
            Opcode::LdVxByte(0xF, 0x42),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xF], 0x42, "VF can be loaded as a normal register");
        // Copy VF to another register
        cpu.execute(
            Opcode::LdVxVy(0xE, 0xF),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.v[0xE], 0x42, "VF value copied to VE via LD VE, VF");
    }

    #[test]
    fn test_reset() {
        let mut cpu = Cpu::new();
        cpu.v[3] = 0xFF;
        cpu.i = 0x999;
        cpu.state = CpuState::Halted;
        cpu.reset();
        assert_eq!(cpu.pc, PROGRAM_START, "Reset: PC should be PROGRAM_START");
        assert_eq!(cpu.i, 0, "Reset: I should be 0");
        assert_eq!(cpu.state, CpuState::Running, "Reset: state should be Running");
        // V registers are NOT reset by default (some interpreters preserve them)
    }

    #[test]
    fn test_cpu_state_transitions() {
        let mut cpu = Cpu::new();
        assert_eq!(cpu.state(), CpuState::Running, "New CPU should be Running");
        cpu.state = CpuState::WaitingForKey(0);
        assert_eq!(cpu.state(), CpuState::WaitingForKey(0), "State should reflect WaitingForKey");
        cpu.state = CpuState::Halted;
        assert_eq!(cpu.state(), CpuState::Halted, "State should reflect Halted");
    }

    #[test]
    fn test_get_pixels_returns_flat_slice() {
        let display = Display::new();
        let pixels = display.get_pixels();
        assert_eq!(pixels.len(), 64 * 32, "get_pixels should return 2048 elements");
    }

    #[test]
    fn test_timers_decrement() {
        let mut timers = Timers::new();
        timers.delay = 60;
        timers.sound = 60;
        timers.update();
        assert_eq!(timers.delay, 59, "Delay timer should decrement from 60 to 59");
        assert_eq!(timers.sound, 59, "Sound timer should decrement from 60 to 59");
    }

    #[test]
    fn test_timers_dont_decrement_below_zero() {
        let mut timers = Timers::new();
        timers.delay = 0;
        timers.sound = 0;
        timers.update();
        assert_eq!(timers.delay, 0, "Delay timer should not go below 0");
        assert_eq!(timers.sound, 0, "Sound timer should not go below 0");
    }

    // --- Quirks: SPRITE_WRAP ---

    #[test]
    fn test_drw_clip_default_drops_pixels_past_right_edge() {
        // Modern default: clipping. A sprite drawn at x=63 only lights the
        // leftmost of its 8 pixels; the rest are dropped.
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.i = 0x300;
        mem.write_byte(0x300, 0xFF).unwrap();
        cpu.v[0] = 63;
        cpu.v[1] = 0;
        cpu.execute(
            Opcode::Drw(0, 1, 1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        let pixels = display.get_pixels();
        assert!(pixels[63], "Clip: column 63 must be drawn");
        assert!(!pixels[0], "Clip: wrapped column 0 must NOT be drawn");
    }

    #[test]
    fn test_drw_wrap_quirk_wraps_pixels_past_right_edge() {
        // COSMAC VIP: SPRITE_WRAP. A sprite at x=62 spills cols 62, 63, 0..5.
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.i = 0x300;
        mem.write_byte(0x300, 0xFF).unwrap();
        cpu.v[0] = 62;
        cpu.v[1] = 0;
        cpu.execute(
            Opcode::Drw(0, 1, 1),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::SPRITE_WRAP,
        )
        .unwrap();
        let pixels = display.get_pixels();
        assert!(pixels[62], "Wrap: column 62 drawn");
        assert!(pixels[63], "Wrap: column 63 drawn");
        assert!(pixels[0], "Wrap: column 0 (wrap-around) drawn");
        assert!(pixels[5], "Wrap: column 5 drawn");
        assert!(!pixels[6], "Wrap: column 6 is past the 8-pixel sprite");
    }

    // --- Quirks: I_OVERFLOW_VF ---

    #[test]
    fn test_add_i_vx_modern_does_not_set_vf_on_overflow() {
        // Default: no VF change on overflow. I wraps naturally as u16.
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.i = 0xFF0;
        cpu.v[7] = 0x20; // 0xFF0 + 0x20 = 0x1010 (exceeds 0xFFF)
        cpu.v[0xF] = 0;
        cpu.execute(
            Opcode::AddIVx(7),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.i, 0x1010, "Modern FX1E: I wraps naturally with no flag");
        assert_eq!(cpu.v[0xF], 0, "Modern FX1E: VF must stay 0 even on overflow");
    }

    #[test]
    fn test_add_i_vx_overflow_quirk_sets_vf() {
        // I_OVERFLOW_VF quirk: when I overflows past 0xFFF, VF=1.
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.i = 0xFF0;
        cpu.v[7] = 0x20; // 0xFF0 + 0x20 = 0x1010 > 0xFFF
        cpu.v[0xF] = 0;
        cpu.execute(
            Opcode::AddIVx(7),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::I_OVERFLOW_VF,
        )
        .unwrap();
        assert_eq!(cpu.v[0xF], 1, "I_OVERFLOW_VF: VF must be set on overflow past 0xFFF");
        assert_eq!(cpu.i, 0x1010, "I_OVERFLOW_VF: I still holds the full u16 sum");
    }

    #[test]
    fn test_add_i_vx_overflow_quirk_no_overflow() {
        // No overflow -> VF untouched by the quirk.
        let mut cpu = Cpu::new();
        let (mut mem, keypad, mut stack, mut timers, mut display) = peripherals();
        cpu.i = 0x200;
        cpu.v[7] = 0x10; // 0x210, well within range
        cpu.v[0xF] = 0x42;
        cpu.execute(
            Opcode::AddIVx(7),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::I_OVERFLOW_VF,
        )
        .unwrap();
        assert_eq!(cpu.i, 0x210, "FX1E: I = 0x200 + 0x10 = 0x210");
        assert_eq!(cpu.v[0xF], 0x42, "I_OVERFLOW_VF: VF must be preserved when no overflow");
    }

    // --- Quirks: KEY_RELEASE ---

    #[test]
    fn test_ld_vx_k_modern_completes_on_press() {
        // Modern FX0A: completes the moment a key is pressed.
        let mut cpu = Cpu::new();
        let mut keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        cpu.execute(
            Opcode::LdVxK(0x3),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::modern(),
        )
        .unwrap();
        assert_eq!(cpu.state(), CpuState::WaitingForKey(0x3));
        keypad.set_key_pressed(0x5, true);
        cpu.tick(&mut mem, &keypad, &mut stack, &mut timers, &mut display, Quirks::modern())
            .unwrap();
        assert_eq!(cpu.v[0x3], 0x5, "Modern FX0A stores pressed key");
        assert_eq!(cpu.state(), CpuState::Running, "Modern FX0A completes on press");
    }

    #[test]
    fn test_ld_vx_k_key_release_requires_release() {
        // COSMAC VIP FX0A: completes only after press AND release.
        let mut cpu = Cpu::new();
        let mut keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        cpu.execute(
            Opcode::LdVxK(0x3),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::KEY_RELEASE,
        )
        .unwrap();
        assert_eq!(cpu.state(), CpuState::WaitingForKey(0x3));
        // Press the key: transition to the release-wait phase, no completion.
        keypad.set_key_pressed(0x5, true);
        cpu.tick(&mut mem, &keypad, &mut stack, &mut timers, &mut display, Quirks::KEY_RELEASE)
            .unwrap();
        assert_eq!(
            cpu.state(),
            CpuState::WaitingForKeyRelease(0x3, 0x5),
            "KEY_RELEASE: press must transition to release-wait, not complete"
        );
        assert_eq!(cpu.v[0x3], 0, "Vx must not be written until the key is released");
        // While still held, additional ticks must not complete the instruction.
        cpu.tick(&mut mem, &keypad, &mut stack, &mut timers, &mut display, Quirks::KEY_RELEASE)
            .unwrap();
        assert_eq!(cpu.state(), CpuState::WaitingForKeyRelease(0x3, 0x5));
        // Release the key: now the instruction completes.
        keypad.set_key_pressed(0x5, false);
        cpu.tick(&mut mem, &keypad, &mut stack, &mut timers, &mut display, Quirks::KEY_RELEASE)
            .unwrap();
        assert_eq!(cpu.v[0x3], 0x5, "KEY_RELEASE: Vx stores the released key");
        assert_eq!(cpu.state(), CpuState::Running, "KEY_RELEASE: completes on release");
    }

    #[test]
    fn test_ld_vx_k_key_release_key_already_held_at_execute() {
        // If a key is already held when LdVxK runs with KEY_RELEASE,
        // the CPU goes straight to the release-wait phase for that key.
        let mut cpu = Cpu::new();
        let mut keypad = Keypad::new();
        let (mut mem, mut stack, mut timers, mut display) =
            (Memory::new(), Stack::new(), Timers::new(), Display::new());
        keypad.set_key_pressed(0x9, true);
        cpu.execute(
            Opcode::LdVxK(0x2),
            &mut mem,
            &keypad,
            &mut stack,
            &mut timers,
            &mut display,
            Quirks::KEY_RELEASE,
        )
        .unwrap();
        assert_eq!(
            cpu.state(),
            CpuState::WaitingForKeyRelease(0x2, 0x9),
            "KEY_RELEASE: pre-held key must skip the press phase"
        );
        // Releasing completes it.
        keypad.set_key_pressed(0x9, false);
        cpu.tick(&mut mem, &keypad, &mut stack, &mut timers, &mut display, Quirks::KEY_RELEASE)
            .unwrap();
        assert_eq!(cpu.v[0x2], 0x9);
        assert_eq!(cpu.state(), CpuState::Running);
    }
}

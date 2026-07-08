//! Chip-8 Opcode Definitions
//
// This module defines the `Opcode` enum, which represents all possible instructions
// in the Chip-8 instruction set. By parsing the raw u16 opcodes into this
// strongly-typed enum, we leverage Rust's type system to ensure that all
// instructions are handled correctly and to make the CPU's execution loop
// more readable and less error-prone.

use std::fmt;

/// Represents a single Chip-8 instruction, decoded into a structured format.
///
/// Each variant corresponds to one of the 35 Chip-8 opcodes. The variants store
/// the decoded parameters (like register indices, addresses, and literal values)
/// in their correct types, providing type safety and clarity.
#[derive(Debug, PartialEq, Eq)]
pub enum Opcode {
    /// 0nnn - SYS addr
    ///
    /// Jumps to a machine code routine at `nnn`. This instruction is only used on
    /// the original COSMAC VIP computers. Modern Chip-8 interpreters, and this one,
    /// will ignore it. It's included for completeness.
    Sys(u16),

    /// 00E0 - CLS
    ///
    /// Clears the entire screen. This is a fundamental operation for rendering in
    /// any game or application, preparing the display for the next frame.
    Cls,

    /// 00EE - RET
    ///
    /// Returns from a subroutine. The interpreter sets the program counter to the
    /// address at the top of the stack, then subtracts 1 from the stack pointer.
    /// This is the counterpart to `2nnn - CALL`.
    Ret,

    /// 1nnn - JP addr
    ///
    /// Jumps to location `nnn`. The program counter is set to `nnn`, causing
    /// execution to continue from that address. This is the primary mechanism for
    /// flow control.
    Jp(u16),

    /// 2nnn - CALL addr
    ///
    /// Calls a subroutine at `nnn`. The interpreter increments the stack pointer,
    /// then puts the current program counter on the top of the stack. The PC is
    /// then set to `nnn`.
    Call(u16),

    /// 3xkk - SE Vx, byte
    ///
    /// Skips the next instruction if register `Vx` equals `kk`. This is a conditional
    /// branch, essential for implementing logic like checking for game state or input.
    SeVxByte(u8, u8),

    /// 4xkk - SNE Vx, byte
    ///
    /// Skips the next instruction if register `Vx` does not equal `kk`. This is the
    /// inverse of `3xkk`, providing complementary conditional logic.
    SneVxByte(u8, u8),

    /// 5xy0 - SE Vx, Vy
    ///
    /// Skips the next instruction if register `Vx` equals register `Vy`. This allows
    /// for comparisons between two variable values.
    SeVxVy(u8, u8),

    /// 6xkk - LD Vx, byte
    ///
    /// Loads the value `kk` into register `Vx`. This is the primary way to set a
    /// register to a specific constant value.
    LdVxByte(u8, u8),

    /// 7xkk - ADD Vx, byte
    ///
    /// Adds the value `kk` to the value of register `Vx`, then stores the result in
    /// `Vx`. This is used for accumulating values, like scores or counters. The carry
    /// flag (VF) is not affected.
    AddVxByte(u8, u8),

    /// 8xy0 - LD Vx, Vy
    ///
    /// Stores the value of register `Vy` in register `Vx`. This is used to copy
    /// values between registers.
    LdVxVy(u8, u8),

    /// 8xy1 - OR Vx, Vy
    ///
    /// Performs a bitwise OR on the values in `Vx` and `Vy`, then stores the
    /// result in `Vx`. `Vx = Vx | Vy`.
    OrVxVy(u8, u8),

    /// 8xy2 - AND Vx, Vy
    ///
    /// Performs a bitwise AND on the values in `Vx` and `Vy`, then stores the
    /// result in `Vx`. `Vx = Vx & Vy`.
    AndVxVy(u8, u8),

    /// 8xy3 - XOR Vx, Vy
    ///
    /// Performs a bitwise XOR on the values in `Vx` and `Vy`, then stores the
    /// result in `Vx`. `Vx = Vx ^ Vy`. This is famously used for sprite drawing.
    XorVxVy(u8, u8),

    /// 8xy4 - ADD Vx, Vy
    ///
    /// Adds the value of `Vy` to `Vx`. If the result is greater than 255 (an
    /// overflow), the carry flag `VF` is set to 1, otherwise 0. Only the lower
    /// 8 bits of the result are stored in `Vx`.
    AddVxVy(u8, u8),

    /// 8xy5 - SUB Vx, Vy
    ///
    /// Subtracts the value of `Vy` from `Vx`. If `Vx` > `Vy`, the borrow flag `VF`
    /// is set to 1, otherwise 0. `Vx = Vx - Vy`.
    SubVxVy(u8, u8),

    /// 8xy6 - SHR Vx {, Vy}
    ///
    /// Shifts `Vx` right by one bit. `VF` is set to the value of the least
    /// significant bit of `Vx` before the shift. Some early interpreters ignored
    /// Vy and shifted Vx, while modern ones may use Vy as the source. This
    /// implementation follows the modern standard of shifting `Vx`.
    ShrVx(u8),

    /// 8xy7 - SUBN Vx, Vy
    ///
    /// Subtracts the value of `Vx` from `Vy` and stores the result in `Vx`. If
    /// `Vy` > `Vx`, the borrow flag `VF` is set to 1, otherwise 0. `Vx = Vy - Vx`.
    SubnVxVy(u8, u8),

    /// 8xyE - SHL Vx {, Vy}
    ///
    /// Shifts `Vx` left by one bit. `VF` is set to the value of the most
    /// significant bit of `Vx` before the shift. Similar to SHR, this follows the
    /// modern standard of shifting `Vx`.
    ShlVx(u8),

    /// 9xy0 - SNE Vx, Vy
    ///
    /// Skips the next instruction if register `Vx` does not equal register `Vy`.
    SneVxVy(u8, u8),

    /// Annn - LD I, addr
    ///
    /// Loads the address `nnn` into the index register `I`. This register is
    /// special and is used to point to memory locations for various operations.
    LdI(u16),

    /// Bnnn - JP V0, addr
    ///
    /// Jumps to the address `nnn` plus the value in register `V0`.
    JpV0(u16),

    /// Cxkk - RND Vx, byte
    ///
    /// Generates a random number between 0 and 255, which is then bitwise ANDed
    /// with the value `kk`. The result is stored in `Vx`.
    Rnd(u8, u8),

    /// Dxyn - DRW Vx, Vy, nibble
    ///
    /// Draws a sprite at coordinate (`Vx`, `Vy`) that has a width of 8 pixels and
    /// a height of `n` pixels. The sprite data is read from memory starting at
    /// the address in the `I` register. `VF` is set to 1 if any screen pixels
    /// are flipped from set to unset during the draw, and 0 otherwise (collision).
    Drw(u8, u8, u8),

    /// Ex9E - SKP Vx
    ///
    /// Skips the next instruction if the key with the value of `Vx` is currently
    /// pressed. This is used for user input.
    Skp(u8),

    /// ExA1 - SKNP Vx
    ///
    /// Skips the next instruction if the key with the value of `Vx` is *not*
    /// currently pressed. The inverse of SKP.
    Sknp(u8),

    /// Fx07 - LD Vx, DT
    ///
    /// Loads the current value of the delay timer into register `Vx`. The delay
    /// timer is a special timer that decrements at 60Hz when non-zero.
    LdVxDt(u8),

    /// Fx0A - LD Vx, K
    ///
    /// Halts execution until a key is pressed, then stores the value of that key
    /// in register `Vx`. This is a blocking operation.
    LdVxK(u8),

    /// Fx15 - LD DT, Vx
    ///
    /// Sets the delay timer to the value in register `Vx`.
    LdDtVx(u8),

    /// Fx18 - LD ST, Vx
    ///
    /// Sets the sound timer to the value in register `Vx`. The sound timer also
    /// decrements at 60Hz and causes a beep for as long as it is non-zero.
    LdStVx(u8),

    /// Fx1E - ADD I, Vx
    ///
    /// Adds the value of `Vx` to the index register `I`.
    AddIVx(u8),

    /// Fx29 - LD F, Vx
    ///
    /// Sets the index register `I` to the memory location of the sprite for the
    /// digit stored in `Vx`. The built-in fontset starts at memory address 0x000.
    LdF(u8),

    /// Fx33 - LD B, Vx
    ///
    /// Stores the Binary-Coded Decimal (BCD) representation of the value in `Vx`
    /// at memory locations `I`, `I+1`, and `I+2`. The hundreds digit is at `I`,
    /// tens at `I+1`, and ones at `I+2`.
    LdB(u8),

    /// Fx55 - LD [I], Vx
    ///
    /// Stores the values from registers `V0` to `Vx` (inclusive) into memory,
    /// starting at the address in `I`. `I` is not modified.
    LdIVx(u8),

    /// Fx65 - LD Vx, [I]
    ///
    /// Fills registers `V0` to `Vx` (inclusive) with values from memory, starting
    /// at the address in `I`. `I` is not modified.
    LdVxI(u8),
}

impl Opcode {
    /// Decodes a `u16` into an `Opcode`.
    pub fn from(opcode: u16) -> Self {
        let op_1 = (opcode & 0xF000) >> 12;
        let op_2 = (opcode & 0x0F00) >> 8;
        let op_3 = (opcode & 0x00F0) >> 4;
        let op_4 = opcode & 0x000F;

        let nnn = opcode & 0x0FFF;
        let kk = (opcode & 0x00FF) as u8;
        let x = op_2 as u8;
        let y = op_3 as u8;
        let n = op_4 as u8;

        match (op_1, op_2, op_3, op_4) {
            (0, 0, 0xE, 0) => Self::Cls,
            (0, 0, 0xE, 0xE) => Self::Ret,
            (0, _, _, _) => Self::Sys(nnn),
            (1, _, _, _) => Self::Jp(nnn),
            (2, _, _, _) => Self::Call(nnn),
            (3, _, _, _) => Self::SeVxByte(x, kk),
            (4, _, _, _) => Self::SneVxByte(x, kk),
            (5, _, _, 0) => Self::SeVxVy(x, y),
            (6, _, _, _) => Self::LdVxByte(x, kk),
            (7, _, _, _) => Self::AddVxByte(x, kk),
            (8, _, _, 0) => Self::LdVxVy(x, y),
            (8, _, _, 1) => Self::OrVxVy(x, y),
            (8, _, _, 2) => Self::AndVxVy(x, y),
            (8, _, _, 3) => Self::XorVxVy(x, y),
            (8, _, _, 4) => Self::AddVxVy(x, y),
            (8, _, _, 5) => Self::SubVxVy(x, y),
            (8, _, _, 6) => Self::ShrVx(x),
            (8, _, _, 7) => Self::SubnVxVy(x, y),
            (8, _, _, 0xE) => Self::ShlVx(x),
            (9, _, _, 0) => Self::SneVxVy(x, y),
            (0xA, _, _, _) => Self::LdI(nnn),
            (0xB, _, _, _) => Self::JpV0(nnn),
            (0xC, _, _, _) => Self::Rnd(x, kk),
            (0xD, _, _, _) => Self::Drw(x, y, n),
            (0xE, _, 9, 0xE) => Self::Skp(x),
            (0xE, _, 0xA, 1) => Self::Sknp(x),
            (0xF, _, 0, 7) => Self::LdVxDt(x),
            (0xF, _, 0, 0xA) => Self::LdVxK(x),
            (0xF, _, 1, 5) => Self::LdDtVx(x),
            (0xF, _, 1, 8) => Self::LdStVx(x),
            (0xF, _, 1, 0xE) => Self::AddIVx(x),
            (0xF, _, 2, 9) => Self::LdF(x),
            (0xF, _, 3, 3) => Self::LdB(x),
            (0xF, _, 5, 5) => Self::LdIVx(x),
            (0xF, _, 6, 5) => Self::LdVxI(x),
            _ => panic!("Unknown opcode: {:#06X}", opcode),
        }
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sys(addr) => write!(f, "SYS {:#05X}", addr),
            Self::Cls => write!(f, "CLS"),
            Self::Ret => write!(f, "RET"),
            Self::Jp(addr) => write!(f, "JP {:#05X}", addr),
            Self::Call(addr) => write!(f, "CALL {:#05X}", addr),
            Self::SeVxByte(x, kk) => write!(f, "SE V{:X}, {:#04X}", x, kk),
            Self::SneVxByte(x, kk) => write!(f, "SNE V{:X}, {:#04X}", x, kk),
            Self::SeVxVy(x, y) => write!(f, "SE V{:X}, V{:X}", x, y),
            Self::LdVxByte(x, kk) => write!(f, "LD V{:X}, {:#04X}", x, kk),
            Self::AddVxByte(x, kk) => write!(f, "ADD V{:X}, {:#04X}", x, kk),
            Self::LdVxVy(x, y) => write!(f, "LD V{:X}, V{:X}", x, y),
            Self::OrVxVy(x, y) => write!(f, "OR V{:X}, V{:X}", x, y),
            Self::AndVxVy(x, y) => write!(f, "AND V{:X}, V{:X}", x, y),
            Self::XorVxVy(x, y) => write!(f, "XOR V{:X}, V{:X}", x, y),
            Self::AddVxVy(x, y) => write!(f, "ADD V{:X}, V{:X}", x, y),
            Self::SubVxVy(x, y) => write!(f, "SUB V{:X}, V{:X}", x, y),
            Self::ShrVx(x) => write!(f, "SHR V{:X}", x),
            Self::SubnVxVy(x, y) => write!(f, "SUBN V{:X}, V{:X}", x, y),
            Self::ShlVx(x) => write!(f, "SHL V{:X}", x),
            Self::SneVxVy(x, y) => write!(f, "SNE V{:X}, V{:X}", x, y),
            Self::LdI(nnn) => write!(f, "LD I, {:#05X}", nnn),
            Self::JpV0(nnn) => write!(f, "JP V0, {:#05X}", nnn),
            Self::Rnd(x, kk) => write!(f, "RND V{:X}, {:#04X}", x, kk),
            Self::Drw(x, y, n) => write!(f, "DRW V{:X}, V{:X}, {}", x, y, n),
            Self::Skp(x) => write!(f, "SKP V{:X}", x),
            Self::Sknp(x) => write!(f, "SKNP V{:X}", x),
            Self::LdVxDt(x) => write!(f, "LD V{:X}, DT", x),
            Self::LdVxK(x) => write!(f, "LD V{:X}, K", x),
            Self::LdDtVx(x) => write!(f, "LD DT, V{:X}", x),
            Self::LdStVx(x) => write!(f, "LD ST, V{:X}", x),
            Self::AddIVx(x) => write!(f, "ADD I, V{:X}", x),
            Self::LdF(x) => write!(f, "LD F, V{:X}", x),
            Self::LdB(x) => write!(f, "LD B, V{:X}", x),
            Self::LdIVx(x) => write!(f, "LD [I], V{:X}", x),
            Self::LdVxI(x) => write!(f, "LD V{:X}, [I]", x),
        }
    }
}

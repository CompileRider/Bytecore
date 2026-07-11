//! Chip-8 Stack
//!
//! The Chip-8 stack is used to store return addresses for subroutines.
//! It has 16 levels of nesting.

use std::fmt;

const STACK_SIZE: usize = 16;

/// Represents an error that occurred during a stack operation.
#[derive(Debug)]
#[non_exhaustive]
pub enum StackError {
    /// The stack has overflowed (pushed beyond capacity).
    Overflow,
    /// The stack has underflowed (popped from empty stack).
    Underflow,
}

impl fmt::Display for StackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => write!(f, "Stack overflow"),
            Self::Underflow => write!(f, "Stack underflow"),
        }
    }
}

impl std::error::Error for StackError {}

/// Represents the Chip-8 stack and stack pointer.
#[derive(Debug)]
pub struct Stack {
    /// The stack, which can hold 16 16-bit addresses.
    entries: [u16; STACK_SIZE],
    /// The stack pointer, which points to the top of the stack.
    pointer: usize,
}

impl Stack {
    /// Creates a new, empty `Stack` instance.
    pub fn new() -> Self {
        Self { entries: [0; STACK_SIZE], pointer: 0 }
    }

    /// Pushes a value onto the stack.
    ///
    /// Returns `Err(StackError::Overflow)` if the stack is full.
    pub fn push(&mut self, value: u16) -> Result<(), StackError> {
        if self.pointer >= STACK_SIZE {
            return Err(StackError::Overflow);
        }
        self.entries[self.pointer] = value;
        self.pointer += 1;
        Ok(())
    }

    /// Pops a value from the stack.
    ///
    /// Returns `Err(StackError::Underflow)` if the stack is empty.
    pub fn pop(&mut self) -> Result<u16, StackError> {
        if self.pointer == 0 {
            return Err(StackError::Underflow);
        }
        self.pointer -= 1;
        Ok(self.entries[self.pointer])
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

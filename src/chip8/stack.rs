//! Chip-8 Stack
//!
//! The Chip-8 stack is used to store return addresses for subroutines.
//! It has 16 levels of nesting.

const STACK_SIZE: usize = 16;

/// Represents the Chip-8 stack and stack pointer.
#[derive(Debug)]
pub struct Stack {
    /// The stack, which can hold 16 16-bit addresses.
    memory: [u16; STACK_SIZE],
    /// The stack pointer, which points to the top of the stack.
    pointer: usize,
}

impl Stack {
    /// Creates a new, empty `Stack` instance.
    pub fn new() -> Self {
        Self { memory: [0; STACK_SIZE], pointer: 0 }
    }

    /// Pushes a value onto the stack.
    ///
    /// # Panics
    ///
    /// This method will panic if the stack overflows.
    pub fn push(&mut self, value: u16) {
        if self.pointer >= STACK_SIZE {
            panic!("Stack overflow");
        }
        self.memory[self.pointer] = value;
        self.pointer += 1;
    }

    /// Pops a value from the stack.
    ///
    /// # Panics
    ///
    /// This method will panic if the stack underflows.
    pub fn pop(&mut self) -> u16 {
        if self.pointer == 0 {
            panic!("Stack underflow");
        }
        self.pointer -= 1;
        self.memory[self.pointer]
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

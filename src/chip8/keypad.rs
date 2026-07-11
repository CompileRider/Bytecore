//! The keypad module for the Chip-8 emulator.

/// The Chip-8 keypad has 16 keys.
const KEY_COUNT: usize = 16;

/// Represents the 16-key hexadecimal keypad of the Chip-8.
#[derive(Debug, Clone, Copy)]
pub struct Keypad {
    /// An array representing the state of the 16 keys (pressed or not).
    keys: [bool; KEY_COUNT],
}

impl Keypad {
    /// Creates a new `Keypad` instance with all keys released.
    pub fn new() -> Self {
        Keypad { keys: [false; KEY_COUNT] }
    }

    /// Checks if a specific key is currently pressed.
    ///
    /// # Arguments
    ///
    /// * `key_code` - The code of the key (0x0-0xF).
    pub fn is_key_pressed(&self, key_code: u8) -> bool {
        self.keys.get(key_code as usize).copied().unwrap_or(false)
    }

    /// Sets the state of a key (pressed or released).
    ///
    /// # Arguments
    ///
    /// * `key_code` - The code of the key (0x0-0xF).
    /// * `pressed` - `true` if the key is pressed, `false` otherwise.
    pub fn set_key_pressed(&mut self, key_code: u8, pressed: bool) {
        if let Some(key) = self.keys.get_mut(key_code as usize) {
            *key = pressed;
        }
    }

    /// Returns the code of the first key found to be pressed.
    ///
    /// This is needed for the `LD Vx, K` instruction, which waits for a
    /// key press.
    pub fn get_key_pressed(&self) -> Option<u8> {
        for (i, &key) in self.keys.iter().enumerate() {
            if key {
                return u8::try_from(i).ok();
            }
        }
        None
    }
}

impl Default for Keypad {
    fn default() -> Self {
        Self::new()
    }
}

//! Chip-8 Display
//!
//! The Chip-8 system has a 64×32 monochrome (1-bit) display. Each pixel is
//! either ON (lit) or OFF (dark). The display uses XOR drawing for sprites,
//! meaning that drawing a pixel over an existing ON pixel turns it OFF
//! (collision detection).
//!
//! # Coordinate System
//!
//! - Origin (0, 0) is the top-left corner
//! - X increases to the right (0–63)
//! - Y increases downward (0–31)
//! - Starting coordinates wrap: (Vx % 64, Vy % 32)
//! - Individual sprite pixels CLIP at screen edges (no wrap-around)
//!
//! # Sprite Drawing (DRW instruction)
//!
//! Sprites are stored as bytes in memory, where each byte represents 8
//! horizontal pixels (MSB = leftmost). The height is specified by the
//! instruction (1–16 rows). Drawing uses XOR logic with collision detection.

/// The width of the Chip-8 display in pixels.
pub const VIDEO_W: usize = 64;
/// The height of the Chip-8 display in pixels.
pub const VIDEO_H: usize = 32;

/// Represents the 64x32 monochrome display of the Chip-8 system.
#[derive(Debug)]
pub struct Display {
    /// The pixel buffer, stored as a flat array of booleans.
    /// `true` means the pixel is on (lit), `false` means off.
    pixels: [bool; VIDEO_W * VIDEO_H],
}

impl Display {
    /// Creates a new Display with all pixels cleared (off).
    pub fn new() -> Self {
        Self { pixels: [false; VIDEO_W * VIDEO_H] }
    }

    /// Clears the entire display, setting all pixels to off.
    ///
    /// Implements the 00E0 (CLS) instruction.
    pub fn clear(&mut self) {
        self.pixels = [false; VIDEO_W * VIDEO_H];
    }

    /// Draws a sprite at the given coordinates using XOR logic.
    ///
    /// This implements the Dxyn (DRW) instruction. The starting coordinates
    /// are wrapped: (x % 64, y % 32). However, the sprite itself is CLIPPED
    /// at screen edges — pixels that would fall outside the display are not
    /// drawn (they do NOT wrap to the opposite side).
    ///
    /// # Arguments
    ///
    /// * `x` - The x-coordinate (will be wrapped mod 64).
    /// * `y` - The y-coordinate (will be wrapped mod 32).
    /// * `sprite` - A slice of bytes representing the sprite rows.
    ///   Each byte represents 8 horizontal pixels.
    ///
    /// # Returns
    ///
    /// `true` if any pixel was flipped from on to off (collision), `false` otherwise.
    pub fn draw_sprite(&mut self, x: u8, y: u8, sprite: &[u8]) -> bool {
        let start_x = x % VIDEO_W as u8;
        let start_y = y % VIDEO_H as u8;
        let mut collision = false;

        for (row, &sprite_byte) in sprite.iter().enumerate() {
            let py = start_y as usize + row;
            // Clip: if this row is past the bottom edge, stop drawing
            if py >= VIDEO_H {
                break;
            }

            for col in 0..8u8 {
                // Check if the sprite pixel is set (bit 7 - col, MSB first)
                if sprite_byte & (0x80 >> col) != 0 {
                    let px = start_x + col;
                    // Clip: if this column is past the right edge, skip it
                    if px >= VIDEO_W as u8 {
                        break;
                    }
                    let idx = py * VIDEO_W + px as usize;

                    // XOR: if both are true, collision occurs
                    if self.pixels[idx] {
                        collision = true;
                    }
                    self.pixels[idx] = !self.pixels[idx];
                }
            }
        }

        collision
    }

    /// Returns a reference to the pixel buffer for frontend rendering.
    pub fn get_pixels(&self) -> &[bool; VIDEO_W * VIDEO_H] {
        &self.pixels
    }
}

impl Default for Display {
    fn default() -> Self {
        Self::new()
    }
}

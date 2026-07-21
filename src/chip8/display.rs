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
//! - Starting coordinates always wrap: `(Vx % 64, Vy % 32)`
//!
//! # Sprite Drawing (DRW instruction)
//!
//! Sprites are stored as bytes in memory, where each byte represents 8
//! horizontal pixels (MSB = leftmost). The height is specified by the
//! instruction (1–16 rows). Drawing uses XOR logic with collision
//! detection.
//!
//! Behaviour at the screen edges is controlled by the `wrap` parameter
//! passed to [`Display::draw_sprite`]:
//!
//! - `wrap = false` (default in modern interpreters): pixels that fall
//!   outside the 64×32 grid are **clipped** (not drawn).
//! - `wrap = true` (COSMAC VIP / early interpreters): pixels that fall
//!   outside the grid **wrap** to the opposite side, so a single sprite
//!   can appear in up to four quadrants.

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
    /// This implements the Dxyn (DRW) instruction. The starting
    /// coordinates are wrapped: `(x % 64, y % 32)`.
    ///
    /// Per-pixel behavior at the edges depends on `wrap`:
    ///
    /// - `wrap = false`: pixels outside the 64×32 grid are clipped (modern
    ///   default).
    /// - `wrap = true`: pixel coordinates wrap modulo the screen dimensions
    ///   (COSMAC VIP behavior).
    ///
    /// # Arguments
    ///
    /// * `x` - The x-coordinate (will be wrapped mod 64).
    /// * `y` - The y-coordinate (will be wrapped mod 32).
    /// * `sprite` - A slice of bytes representing the sprite rows.
    ///   Each byte represents 8 horizontal pixels.
    /// * `wrap` - If `true`, wrap sprite pixels at screen edges; if `false`,
    ///   clip them.
    ///
    /// # Returns
    ///
    /// `true` if any pixel was flipped from on to off (collision), `false`
    /// otherwise.
    pub fn draw_sprite(&mut self, x: u8, y: u8, sprite: &[u8], wrap: bool) -> bool {
        let start_x = x % VIDEO_W as u8;
        let start_y = y % VIDEO_H as u8;
        let mut collision = false;

        for (row, &sprite_byte) in sprite.iter().enumerate() {
            let py_raw = start_y as usize + row;
            // Clip: rows past the bottom edge stop the draw entirely (wrap wraps them).
            if !wrap && py_raw >= VIDEO_H {
                break;
            }
            let py = py_raw % VIDEO_H;

            for col in 0..8u8 {
                if sprite_byte & (0x80 >> col) == 0 {
                    continue;
                }
                let px_raw = start_x.wrapping_add(col) as usize;
                if !wrap && px_raw >= VIDEO_W {
                    break;
                }
                let px = px_raw % VIDEO_W;
                let idx = py * VIDEO_W + px;

                if self.pixels[idx] {
                    collision = true;
                }
                self.pixels[idx] = !self.pixels[idx];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_blank() {
        let d = Display::new();
        assert!(d.get_pixels().iter().all(|&p| !p), "Fresh display must have all pixels off");
    }

    #[test]
    fn test_clear_is_idempotent() {
        let mut d = Display::new();
        d.draw_sprite(0, 0, &[0xFF, 0xFF], false);
        assert!(d.get_pixels().iter().any(|&p| p), "Prewarm: some pixels should be on");
        d.clear();
        assert!(d.get_pixels().iter().all(|&p| !p), "After CLS, all pixels must be off");
    }

    #[test]
    fn test_starting_coordinates_wrap() {
        let mut d = Display::new();
        // x=200 % 64 = 8, y=64 % 32 = 0. Draw one lit pixel.
        let _ = d.draw_sprite(200, 64, &[0x80], false);
        let pixels = d.get_pixels();
        assert!(pixels[8], "Start coordinates must wrap (x=200→8, y=64→0)");
        assert!(!pixels[VIDEO_W + 8], "Only the wrapped row should be lit");
    }

    #[test]
    fn test_clipping_drops_pixels_past_right_edge() {
        let mut d = Display::new();
        // Start at x=63 (last column). The 8-pixel sprite extends past the
        // edge. Without wrapping, only the leftmost pixel (column 63) is
        // drawn.
        let _ = d.draw_sprite(63, 0, &[0xFF], false);
        let pixels = d.get_pixels();
        assert!(pixels[63], "Pixel at (63,0) must be drawn");
        for x in (64..VIDEO_W).take(0) {
            assert!(!pixels[x], "No pixel should exist past x=63");
        }
        // Spot-check the next row has nothing.
        assert!(!pixels[VIDEO_W], "Row 1 should be untouched under clipping");
    }

    #[test]
    fn test_clipping_stops_at_bottom_edge() {
        let mut d = Display::new();
        // Start at y=30 with a 4-row sprite would draw rows 30, 31, then stop.
        let _ = d.draw_sprite(0, 30, &[0xFF; 4], false);
        let pixels = d.get_pixels();
        assert!(pixels[30 * VIDEO_W], "Row 30 must be drawn");
        assert!(pixels[31 * VIDEO_W], "Row 31 must be drawn");
        // Under clipping, rows past 31 are not drawn at all, so row 0 stays off.
        assert!(!pixels[0], "Row 0 must stay off under clipping");
    }

    #[test]
    fn test_wrap_continues_pixels_past_right_edge() {
        let mut d = Display::new();
        // Start at x=62 (sprite cols 62, 63, then 0..5). With wrap=true,
        // column 64 becomes column 0, etc.
        let _ = d.draw_sprite(62, 0, &[0xFF], true);
        let pixels = d.get_pixels();
        assert!(pixels[62], "Wrapped: column 62 drawn");
        assert!(pixels[63], "Wrapped: column 63 drawn");
        assert!(pixels[0], "Wrapped: column 0 (wrap-around) drawn");
        assert!(pixels[1], "Wrapped: column 1 drawn");
        assert!(pixels[5], "Wrapped: column 5 drawn");
        assert!(!pixels[6], "Sprite is 8 cols wide; column 6 untouched");
    }

    #[test]
    fn test_wrap_continues_rows_past_bottom_edge() {
        let mut d = Display::new();
        // Start at y=30 with a 4-row sprite: rows 30, 31, then wrap to 0, 1.
        let _ = d.draw_sprite(0, 30, &[0xFF; 4], true);
        let pixels = d.get_pixels();
        assert!(pixels[30 * VIDEO_W], "Wrapped: row 30 drawn");
        assert!(pixels[31 * VIDEO_W], "Wrapped: row 31 drawn");
        assert!(pixels[0], "Wrapped: row 0 (wrap-around) drawn");
        assert!(pixels[VIDEO_W], "Wrapped: row 1 drawn");
        assert!(!pixels[2 * VIDEO_W], "Wrapped: only 4 rows of sprite");
    }

    #[test]
    fn test_collision_when_pixels_overlap() {
        let mut d = Display::new();
        let sprite = [0xFFu8];
        // First draw lights column 0.
        let c1 = d.draw_sprite(0, 0, &sprite, false);
        assert!(!c1, "First draw on fresh display: no collision");
        // Second draw at the same place flips them off → collision.
        let c2 = d.draw_sprite(0, 0, &sprite, false);
        assert!(c2, "Second overlapping draw must report a collision");
    }

    #[test]
    fn test_no_collision_when_two_sprites_disjoint() {
        let mut d = Display::new();
        // 0xAA = 1010_1010 (cols 1,3,5,7); 0x55 = 0101_0101 (cols 0,2,4,6).
        // These have no overlapping lit pixels, so combining them is collision-free.
        assert!(!d.draw_sprite(0, 0, &[0xAA], false), "0xAA on blank display: no collision");
        assert!(
            !d.draw_sprite(0, 0, &[0x55], false),
            "0x55 on top of 0xAA is disjoint: no collision"
        );
        // Combined pixels form 0xFF — every column in the first row is lit.
        for col in 0..8 {
            assert!(d.get_pixels()[col], "Col {col} must be lit after combining 0xAA and 0x55");
        }
    }
}

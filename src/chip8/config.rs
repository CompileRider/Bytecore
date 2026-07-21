//! Chip-8 Quirk Configuration
//!
//! Different Chip-8 interpreters and hardware platforms exhibit slightly
//! different behavior for certain opcodes. These behavioral differences
//! are called "quirks." This module provides a `Quirks` bitflag type that
//! allows the emulator to be configured to match the behavior of specific platforms.
//!
//! # Supported Platforms
//!
//! - **COSMAC VIP** — The original 1977 hardware. Uses VY as the shift source,
//!   increments I after store/load, resets VF after logic ops, and jumps with V0.
//! - **Modern** — Common behavior in modern interpreters (CHIP-48, SCHIP).
//!   Shifts VX in place, preserves I, resets VF to 0, and supports HP48 jumps.
//! - **HP48** — Similar to Modern but uses VX in BNNN jumps.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

bitflags! {
    /// Configuration flags for Chip-8 quirk compatibility.
    ///
    /// Each flag represents a behavioral difference between Chip-8 platforms.
    /// Use `contains()` to check if a quirk is active, and bitwise OR to
    /// combine multiple flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Quirks: u8 {
        /// Shift quirk (8XY6/8XYE): COSMAC VIP uses VY as shift source.
        const SHIFT_VY    = 0b0000_0001;
        /// I increment quirk (FX55/FX65): COSMAC VIP increments I after store/load.
        const I_INCREMENT = 0b0000_0010;
        /// VF reset quirk (8XY1/8XY2/8XY3): COSMAC VIP resets VF after OR/AND/XOR.
        const VF_RESET    = 0b0000_0100;
        /// Jump quirk (BNNN): HP48 uses NNN+Vx instead of NNN+V0.
        const JUMP_VX     = 0b0000_1000;
        /// VBlank wait quirk (DXYN): COSMAC VIP waits for VBlank before drawing.
        const WAIT_VBLANK = 0b0001_0000;
        /// Sprite wrap quirk (DXYN): older interpreters wrap sprite pixels around
        /// screen edges instead of clipping them. The starting coordinates are
        /// wrapped in all variants; only the per-pixel behavior changes.
        const SPRITE_WRAP   = 0b0010_0000;
        /// Key release quirk (FX0A): COSMAC VIP only registers a key once it has
        /// been pressed and then released.
        const KEY_RELEASE   = 0b0100_0000;
        /// I overflow flag quirk (FX1E): some interpreters set VF=1 when the index
        /// register would overflow past address 0xFFF.
        const I_OVERFLOW_VF = 0b1000_0000;
    }
}

impl Default for Quirks {
    fn default() -> Self {
        Self::modern()
    }
}

impl Quirks {
    /// COSMAC VIP configuration — original 1977 hardware behavior.
    pub fn cosmac_vip() -> Self {
        Self::SHIFT_VY
            | Self::I_INCREMENT
            | Self::VF_RESET
            | Self::WAIT_VBLANK
            | Self::SPRITE_WRAP
            | Self::KEY_RELEASE
            | Self::I_OVERFLOW_VF
    }

    /// Modern configuration — common interpreter behavior (default).
    pub fn modern() -> Self {
        Self::empty()
    }

    /// HP48 configuration — uses Vx in BNNN jump.
    pub fn hp48() -> Self {
        Self::JUMP_VX
    }
}

impl fmt::Display for Quirks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (name, _) in self.iter_names() {
            if !first {
                write!(f, " | ")?;
            }
            write!(f, "{}", name)?;
            first = false;
        }
        if first {
            write!(f, "empty")?;
        }
        Ok(())
    }
}

/// Persistent configuration loaded from a TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The active quirk flags.
    pub quirks: Quirks,
    /// CPU clock speed in Hz.
    pub cpu_hz: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self { quirks: Quirks::modern(), cpu_hz: 700 }
    }
}

impl Config {
    /// Creates a new Config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads configuration from a TOML file.
    ///
    /// If the file doesn't exist or fails to parse, returns defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Saves configuration to a TOML file.
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let toml = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quick_flag_values() {
        assert_eq!(Quirks::SHIFT_VY.bits(), 0b0000_0001);
        assert_eq!(Quirks::I_INCREMENT.bits(), 0b0000_0010);
        assert_eq!(Quirks::VF_RESET.bits(), 0b0000_0100);
        assert_eq!(Quirks::JUMP_VX.bits(), 0b0000_1000);
        assert_eq!(Quirks::WAIT_VBLANK.bits(), 0b0001_0000);
        assert_eq!(Quirks::SPRITE_WRAP.bits(), 0b0010_0000);
        assert_eq!(Quirks::KEY_RELEASE.bits(), 0b0100_0000);
        assert_eq!(Quirks::I_OVERFLOW_VF.bits(), 0b1000_0000);
    }

    #[test]
    fn test_no_overlapping_bits() {
        let all = Quirks::SHIFT_VY
            | Quirks::I_INCREMENT
            | Quirks::VF_RESET
            | Quirks::JUMP_VX
            | Quirks::WAIT_VBLANK
            | Quirks::SPRITE_WRAP
            | Quirks::KEY_RELEASE
            | Quirks::I_OVERFLOW_VF;
        assert_eq!(all.bits(), 0xFF, "All eight flags must combine to 0xFF with no overlap");
    }

    #[test]
    fn test_cosmac_vip_includes_new_quirks() {
        let q = Quirks::cosmac_vip();
        assert!(q.contains(Quirks::SPRITE_WRAP), "COSMAC VIP must include SPRITE_WRAP");
        assert!(q.contains(Quirks::KEY_RELEASE), "COSMAC VIP must include KEY_RELEASE");
        assert!(q.contains(Quirks::I_OVERFLOW_VF), "COSMAC VIP must include I_OVERFLOW_VF");
    }

    #[test]
    fn test_modern_excludes_new_quirks() {
        let q = Quirks::modern();
        assert!(!q.contains(Quirks::SPRITE_WRAP), "Modern must NOT include SPRITE_WRAP");
        assert!(!q.contains(Quirks::KEY_RELEASE), "Modern must NOT include KEY_RELEASE");
        assert!(!q.contains(Quirks::I_OVERFLOW_VF), "Modern must NOT include I_OVERFLOW_VF");
    }

    #[test]
    fn test_hp48_is_minimal() {
        let q = Quirks::hp48();
        assert_eq!(q, Quirks::JUMP_VX, "HP48 must only set JUMP_VX");
    }

    #[test]
    fn test_default_is_modern() {
        assert_eq!(Quirks::default(), Quirks::modern());
    }
}

//! SDL2 graphical frontend for the Bytecore Chip-8 emulator.
//!
//! Provides a graphical window with keyboard input and audio output
//! using the SDL2 library. Implements the `Frontend` trait for use
//! with the emulator's main loop.
//!
//! ## Features
//!
//! - Scaled pixel-perfect rendering (10× scale, 640×320 window)
//! - Bytecore Neon color scheme (green-on-dark)
//! - Chip-8 hex keypad mapped to PC keyboard
//! - Audio square wave for sound timer (440 Hz)
//!
//! ## Prerequisites
//!
//! Requires SDL2 development libraries:
//! - Linux: `sudo apt install libsdl2-dev`
//! - macOS: `brew install sdl2`
//! - Windows: bundled via cargo

#![cfg(feature = "sdl2")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sdl2::Sdl;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;

use crate::chip8::display::{Display, VIDEO_H, VIDEO_W};
use crate::chip8::keypad::Keypad;
use crate::frontend::{Frontend, TickSource, UserAction};

/// Display scale factor (each Chip-8 pixel becomes SCALE×SCALE screen pixels).
const SCALE: u32 = 10;

/// Window width: 64 columns × 10 px = 640 px.
const WINDOW_W: u32 = VIDEO_W as u32 * SCALE;

/// Window height: 32 rows × 10 px = 320 px.
const WINDOW_H: u32 = VIDEO_H as u32 * SCALE;

const PIXEL_ON: Color = Color::RGB(0x00, 0xFF, 0x88);
const BG: Color = Color::RGB(0x0A, 0x0A, 0x0F);

/// Maps an SDL2 `Keycode` to a Chip-8 key code (0x0–0xF).
///
/// Layout (Cowgod §2.3):
/// ```text
/// Chip-8  →  PC Keyboard
/// 1 2 3 C     1 2 3 4
/// 4 5 6 D     Q W E R
/// 7 8 9 E     A S D F
/// A 0 B F     Z X C V
/// ```
fn map_sdl2_keycode(keycode: Keycode) -> Option<u8> {
    match keycode {
        // Player 1 controls (PONG: 1/Q) mapped to W/S or UP/DOWN arrows
        Keycode::NUM_1 | Keycode::W | Keycode::UP => Some(0x1), // PONG Player 1 Up
        Keycode::Q | Keycode::S | Keycode::DOWN => Some(0x4),   // PONG Player 1 Down
        Keycode::NUM_2 => Some(0x2),
        Keycode::NUM_3 => Some(0x3),
        // Player 2 controls (PONG: 4/R) mapped to I/K
        Keycode::NUM_4 | Keycode::I => Some(0xC), // PONG Player 2 Up (C)
        Keycode::E => Some(0x6),
        Keycode::R | Keycode::K => Some(0xD), // PONG Player 2 Down (D)
        Keycode::A | Keycode::LEFT => Some(0x7),
        Keycode::D | Keycode::RIGHT => Some(0x9),
        Keycode::SPACE => Some(0x5), // Action/Fire (5)
        Keycode::F => Some(0xE),
        Keycode::Z => Some(0xA),
        Keycode::X => Some(0x0),
        Keycode::C => Some(0xB),
        Keycode::V => Some(0xF),
        _ => None,
    }
}

/// Audio callback that generates a square wave at ~440 Hz when active.
///
/// The callback is invoked by the SDL2 audio subsystem to fill the
/// audio buffer. When `sound_active` is true, it produces a 440 Hz
/// square wave; otherwise it outputs silence (zero).
struct SoundCallback {
    /// Current phase of the waveform (0.0–1.0).
    phase: f64,
    /// Sample rate in Hz (e.g., 44100).
    sample_rate: f64,
    /// Shared flag indicating whether sound should be active.
    sound_active: Arc<AtomicBool>,
}

impl AudioCallback for SoundCallback {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        if self.sound_active.load(Ordering::Relaxed) {
            for sample in out.iter_mut() {
                *sample = if self.phase < 0.5 { i16::MAX } else { i16::MIN };
                self.phase += 440.0 / self.sample_rate;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
            }
        } else {
            for sample in out.iter_mut() {
                *sample = 0;
            }
            self.phase = 0.0;
        }
    }
}

/// SDL2 graphical frontend with audio support.
///
/// Creates a window showing the Chip-8 display scaled 10×, handles
/// keyboard input mapped to the Chip-8 hex keypad, and produces a
/// 440 Hz beep when the sound timer is active.
pub struct Sdl2Frontend {
    /// The SDL2 context (keeps SDL initialized).
    _sdl: Sdl,
    /// Event pump for polling input events.
    event_pump: sdl2::EventPump,
    /// Window canvas for rendering.
    canvas: WindowCanvas,
    /// Frame-rate timing (60 FPS).
    tick_source: TickSource,
    /// Shared flag for the audio callback (sound timer active).
    sound_active: Arc<AtomicBool>,
    /// Audio device (ownership keeps the audio thread running).
    #[allow(dead_code)]
    audio_device: AudioDevice<SoundCallback>,
}

impl std::fmt::Debug for Sdl2Frontend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sdl2Frontend").finish_non_exhaustive()
    }
}

impl Sdl2Frontend {
    /// Creates a new SDL2 frontend, initializing the window and audio.
    ///
    /// # Errors
    ///
    /// Returns an error string if SDL2 initialization fails (e.g., no
    /// display available, audio device not found).
    pub fn new() -> Result<Self, String> {
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        let window = video
            .window("Bytecore Chip-8", WINDOW_W, WINDOW_H)
            .position_centered()
            .build()
            .map_err(|e| format!("Failed to create window: {e}"))?;

        let canvas =
            window.into_canvas().build().map_err(|e| format!("Failed to create canvas: {e}"))?;

        let event_pump =
            sdl.event_pump().map_err(|e| format!("Failed to create event pump: {e}"))?;

        // Audio setup: 44100 Hz, mono, 512-sample buffer.
        let audio = sdl.audio()?;
        let sound_active = Arc::new(AtomicBool::new(false));
        let sound_active_clone = Arc::clone(&sound_active);

        let desired_spec = AudioSpecDesired {
            freq: Some(44100),
            channels: Some(1), // mono
            samples: Some(512),
        };

        let audio_device = audio
            .open_playback(None, &desired_spec, |spec| SoundCallback {
                phase: 0.0,
                sample_rate: spec.freq as f64,
                sound_active: sound_active_clone,
            })
            .map_err(|e| format!("Failed to open audio device: {e}"))?;

        // Start audio playback (silent until sound timer activates).
        audio_device.resume();

        Ok(Self {
            _sdl: sdl,
            event_pump,
            canvas,
            tick_source: TickSource::new(60),
            sound_active,
            audio_device,
        })
    }
}

impl Frontend for Sdl2Frontend {
    fn handle_events(&mut self, keypad: &mut Keypad) -> UserAction {
        let mut action = UserAction::Continue;
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    action = UserAction::Exit;
                }
                Event::KeyDown { keycode: Some(keycode), repeat: false, .. } => {
                    if keycode == Keycode::ESCAPE {
                        action = UserAction::Exit;
                    } else if keycode == Keycode::P {
                        action = UserAction::PauseToggle;
                    } else if keycode == Keycode::R {
                        action = UserAction::Reset;
                    } else if let Some(chip8_key) = map_sdl2_keycode(keycode) {
                        keypad.set_key_pressed(chip8_key, true);
                    }
                }
                Event::KeyUp { keycode: Some(keycode), .. } => {
                    if let Some(chip8_key) = map_sdl2_keycode(keycode) {
                        keypad.set_key_pressed(chip8_key, false);
                    }
                }
                _ => {}
            }
        }
        action
    }

    fn render(&mut self, display: &Display) {
        let pixels = display.get_pixels();

        // Clear to background color.
        self.canvas.set_draw_color(BG);
        self.canvas.clear();

        // Draw ON pixels as scaled rectangles.
        // Only drawing ON pixels is more efficient than drawing all 2048.
        for (i, &pixel) in pixels.iter().enumerate() {
            if !pixel {
                continue;
            }
            let x = (i % VIDEO_W) as i32 * SCALE as i32;
            let y = (i / VIDEO_W) as i32 * SCALE as i32;
            self.canvas.set_draw_color(PIXEL_ON);
            // Ignore fill_rect errors in the render loop.
            let _ = self.canvas.fill_rect(Rect::new(x, y, SCALE, SCALE));
        }

        self.canvas.present();
    }

    fn wait_for_next_frame(&mut self) {
        self.tick_source.wait_for_next_frame();
    }

    fn update_sound(&mut self, active: bool) {
        self.sound_active.store(active, Ordering::Relaxed);
    }
}

//! Terminal frontend backend using ratatui + crossterm.
//!
//! Renders the Chip-8 display in a terminal window using Unicode block
//! characters (`█` for ON, space for OFF) with Bytecore Neon color scheme.
//! Handles keyboard input via crossterm's event system.

use crate::chip8::display::{Display, VIDEO_H, VIDEO_W};
use crate::chip8::keypad::Keypad;
use crate::frontend::{Frontend, TickSource, UserAction, map_key_to_chip8};
use std::time::Instant;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io::stdout;
use std::time::Duration;

/// Bytecore Neon brand colors (private to this module).
mod palette {
    use ratatui::style::Color;
    pub(super) const NEON_GREEN: Color = Color::Rgb(0x00, 0xFF, 0x88);
    pub(super) const VOID: Color = Color::Rgb(0x0A, 0x0A, 0x0F);
    pub(super) const DIM_GREEN: Color = Color::Rgb(0x00, 0x55, 0x2E);
}

/// Terminal-based frontend for the Chip-8 emulator.
///
/// Uses ratatui (TUI framework) and crossterm (terminal backend) for
/// rendering and input handling. Renders the 64×32 pixel display using
/// Unicode block characters with the Bytecore Neon color scheme.
pub struct TerminalFrontend {
    /// The ratatui terminal instance.
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    /// Frame timing source.
    tick_source: TickSource,
    /// Whether the sound beep is currently active, used to only emit the
    /// bell on the off -> on transition (debounce).
    sound_active: bool,
    /// Tracks the Instant each key was last pressed (for key decay/simulated release on Unix).
    last_press_times: [Instant; 16],
}

impl std::fmt::Debug for TerminalFrontend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalFrontend")
            .field("tick_source", &self.tick_source)
            .finish_non_exhaustive()
    }
}

impl TerminalFrontend {
    /// Creates a new terminal frontend and switches to alternate screen.
    ///
    /// Enables raw mode and enters the alternate screen buffer, then
    /// creates a ratatui terminal. The terminal is restored on drop.
    ///
    /// # Errors
    ///
    /// Returns an error if raw mode cannot be enabled, the alternate
    /// screen cannot be entered, or the ratatui terminal fails to init.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        // Initialize last press times to 1 second in the past so decay triggers immediately
        let past = Instant::now() - std::time::Duration::from_secs(1);

        Ok(Self {
            terminal,
            tick_source: TickSource::new(60),
            sound_active: false,
            last_press_times: [past; 16],
        })
    }

    /// Builds the styled display text lines from the pixel buffer.
    ///
    /// Each ON pixel is rendered as a green `█` character; each OFF pixel
    /// as a space. Returns one `Line` per row of the Chip-8 display.
    fn build_display_lines(pixels: &[bool; VIDEO_W * VIDEO_H]) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(VIDEO_H);
        for y in 0..VIDEO_H {
            let mut spans = Vec::with_capacity(VIDEO_W);
            for x in 0..VIDEO_W {
                let idx = y * VIDEO_W + x;
                if pixels[idx] {
                    spans.push(Span::styled("█", Style::default().fg(palette::NEON_GREEN)));
                } else {
                    spans.push(Span::styled(" ", Style::default().fg(palette::VOID)));
                }
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    /// Builds a status line showing helpful keybindings.
    fn build_status_line() -> Line<'static> {
        Line::from(vec![Span::styled(
            " ESC: Salir  |  P: Pausa  |  R: Reset",
            Style::default().fg(palette::DIM_GREEN),
        )])
    }
}

impl Drop for TerminalFrontend {
    fn drop(&mut self) {
        // Best-effort terminal restore. Ignore errors in drop.
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

impl Frontend for TerminalFrontend {
    fn handle_events(&mut self, keypad: &mut Keypad) -> UserAction {
        let mut action = UserAction::Continue;

        // Poll with a short timeout so we don't block rendering.
        while event::poll(Duration::from_millis(1)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Esc => {
                        action = UserAction::Exit;
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        // Pause key: only toggle on initial press or if kind is not release
                        if key.kind != KeyEventKind::Release {
                            action = UserAction::PauseToggle;
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        // Reset key
                        if key.kind != KeyEventKind::Release {
                            action = UserAction::Reset;
                        }
                    }
                    KeyCode::Up => {
                        // Alias to CHIP-8 Key 1 (Up)
                        let chip8_key = 0x1;
                        if key.kind != KeyEventKind::Release {
                            keypad.set_key_pressed(chip8_key, true);
                            self.last_press_times[chip8_key as usize] = Instant::now();
                        } else {
                            keypad.set_key_pressed(chip8_key, false);
                        }
                    }
                    KeyCode::Down => {
                        // Alias to CHIP-8 Key 4 (Down)
                        let chip8_key = 0x4;
                        if key.kind != KeyEventKind::Release {
                            keypad.set_key_pressed(chip8_key, true);
                            self.last_press_times[chip8_key as usize] = Instant::now();
                        } else {
                            keypad.set_key_pressed(chip8_key, false);
                        }
                    }
                    KeyCode::Left => {
                        // Alias to CHIP-8 Key 7 (Left)
                        let chip8_key = 0x7;
                        if key.kind != KeyEventKind::Release {
                            keypad.set_key_pressed(chip8_key, true);
                            self.last_press_times[chip8_key as usize] = Instant::now();
                        } else {
                            keypad.set_key_pressed(chip8_key, false);
                        }
                    }
                    KeyCode::Right => {
                        // Alias to CHIP-8 Key 9 (Right)
                        let chip8_key = 0x9;
                        if key.kind != KeyEventKind::Release {
                            keypad.set_key_pressed(chip8_key, true);
                            self.last_press_times[chip8_key as usize] = Instant::now();
                        } else {
                            keypad.set_key_pressed(chip8_key, false);
                        }
                    }
                    KeyCode::Char(c) => {
                        if let Some(chip8_key) = map_key_to_chip8(c) {
                            if key.kind != KeyEventKind::Release {
                                keypad.set_key_pressed(chip8_key, true);
                                self.last_press_times[chip8_key as usize] = Instant::now();
                            } else {
                                keypad.set_key_pressed(chip8_key, false);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Apply key decay for Unix/Linux terminals that do not report key release events.
        // If a key has not had a press event in 150ms, we automatically release it.
        let now = Instant::now();
        let decay_duration = Duration::from_millis(150);
        for i in 0..16 {
            if now.duration_since(self.last_press_times[i]) > decay_duration {
                keypad.set_key_pressed(i as u8, false);
            }
        }

        action
    }

    fn render(&mut self, display: &Display) {
        let _ = self.terminal.draw(|f| {
            let area = f.area();
            let pixels = display.get_pixels();
            let display_lines = Self::build_display_lines(pixels);
            let status_line = Self::build_status_line();

            // Combine display (VIDEO_H rows) + spacer + status line. The
            // paragraph also wraps its content in Borders::ALL, which consume
            // one char on each side and one row on top/bottom. Size the
            // outer rect to leave room for the borders so nothing is clipped:
            //   inner width  = outer width  - 2  (left + right border)
            //   inner height = outer height - 2  (top + bottom border)
            // so outer width must be VIDEO_W + 2 and outer height must be
            // (VIDEO_H + spacer + status) + 2.
            let mut all_lines = display_lines;
            all_lines.push(Line::from("")); // spacer
            all_lines.push(status_line);

            let paragraph = Paragraph::new(all_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Bytecore Chip-8 ")
                        .title_alignment(Alignment::Center)
                        .style(Style::default().fg(palette::NEON_GREEN).bg(palette::VOID)),
                )
                .style(Style::default().bg(palette::VOID));

            // VIDEO_W cols of pixels need VIDEO_W inner width, so reserve +2
            // for borders. The content is VIDEO_H + spacer + status lines,
            // so reserve +2 more for the top/bottom border.
            let outer_width = VIDEO_W as u16 + 2;
            let outer_height = (VIDEO_H as u16) + 2 /* spacer + status */ + 2;
            let display_rect = centered_rect(area, outer_width, outer_height);
            f.render_widget(paragraph, display_rect);
        });
    }

    fn wait_for_next_frame(&mut self) {
        self.tick_source.wait_for_next_frame();
    }

    fn update_sound(&mut self, active: bool) {
        // Only emit the bell on the off -> on transition so we don't spam
        // the terminal every frame while the sound timer is held high.
        if active && !self.sound_active {
            // \u{0007} is the ASCII BEL character. Most terminals emit a
            // short beep (or visible bell) when receiving it.
            let _ = execute!(std::io::stdout(), crossterm::style::Print('\u{0007}'));
        }
        self.sound_active = active;
    }
}

/// Returns a centered `Rect` of the given width and height within `outer`.
fn centered_rect(outer: Rect, width: u16, height: u16) -> Rect {
    let x = outer.x.saturating_add(outer.width.saturating_sub(width) / 2);
    let y = outer.y.saturating_add(outer.height.saturating_sub(height) / 2);
    Rect { x, y, width: width.min(outer.width), height: height.min(outer.height) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centered_rect_smaller_than_outer() {
        let outer = Rect { x: 0, y: 0, width: 100, height: 40 };
        let r = centered_rect(outer, 64, 32);
        assert_eq!(r.width, 64);
        assert_eq!(r.height, 32);
        assert_eq!(r.x, (100 - 64) / 2);
        assert_eq!(r.y, (40 - 32) / 2);
    }

    #[test]
    fn test_centered_rect_clamps_to_outer() {
        let outer = Rect { x: 0, y: 0, width: 32, height: 16 };
        let r = centered_rect(outer, 64, 32);
        assert_eq!(r.width, 32, "Width must clamp to outer width");
        assert_eq!(r.height, 16, "Height must clamp to outer height");
    }

    #[test]
    fn test_build_display_lines_empty_buffer() {
        let pixels = [false; VIDEO_W * VIDEO_H];
        let lines = TerminalFrontend::build_display_lines(&pixels);
        assert_eq!(lines.len(), VIDEO_H, "One line per display row");
    }

    #[test]
    fn test_build_display_lines_full_buffer() {
        let mut pixels = [false; VIDEO_W * VIDEO_H];
        for p in pixels.iter_mut() {
            *p = true;
        }
        let lines = TerminalFrontend::build_display_lines(&pixels);
        assert_eq!(lines.len(), VIDEO_H);
        // Each line has VIDEO_W spans (one per pixel).
        for (i, line) in lines.iter().enumerate() {
            assert_eq!(line.spans.len(), VIDEO_W, "Line {} must have one span per column", i);
        }
    }

    /// Regression: the framed paragraph must reserve room for Borders::ALL
    /// so the inner content area fits the full 64-column display plus the
    /// spacer and status line. Previously the outer rect was exactly
    /// VIDEO_W x (VIDEO_H + 3), leaving only 62x33 inner and clipping the
    /// last 2 columns and the status line (visible in PONG).
    #[test]
    fn test_render_rect_fits_full_display_with_borders() {
        let outer = Rect { x: 0, y: 0, width: 200, height: 80 };
        let outer_width = VIDEO_W as u16 + 2;
        let outer_height = (VIDEO_H as u16) + 2 + 2;
        let rect = centered_rect(outer, outer_width, outer_height);

        // The Borders::ALL block consumes 1 char on each side and 1 row
        // on top/bottom, so the inner area is width-2 columns and
        // height-2 rows. That must fit the content exactly:
        //   - VIDEO_W columns of pixels
        //   - VIDEO_H display rows + 1 spacer + 1 status line
        let inner_width = rect.width.saturating_sub(2);
        let inner_height = rect.height.saturating_sub(2);
        assert!(
            inner_width >= VIDEO_W as u16,
            "inner width {} must be >= VIDEO_W {} so pixels are not clipped",
            inner_width,
            VIDEO_W
        );
        assert!(
            inner_height >= VIDEO_H as u16 + 2,
            "inner height {} must be >= {} (display + spacer + status) so nothing is dropped",
            inner_height,
            VIDEO_H + 2
        );
    }
}

#### I'm developing this, and most of the features are not implemented yet.

# Chip-8 Emulator

A software CPU virtual machine that runs classic Chip-8 ROMs (Pong, Space Invaders, etc.). Reads a binary ROM into simulated RAM, runs a fetch-decode-execute cycle: read opcode, interpret it (arithmetic, memory, jumps), update CPU state.

## Features

- Full Chip-8 instruction set (35 opcodes)
- 64×32 monochrome display with XOR sprite drawing
- 16-key hex keypad input
- 60 Hz delay and sound timers
- Quirks configuration for compatibility modes (COSMAC VIP, modern, HP48)
- Modular backend architecture (SDL2 or terminal)
- `#![forbid(unsafe_code)]` — pure safe Rust

## Getting Started

### Prerequisites

- Rust toolchain (stable): `rustup install stable`
- (Optional) SDL2 for the graphical backend:
  - Linux: `sudo apt install libsdl2-dev`
  - macOS: `brew install sdl2`
  - Windows: SDL2 is bundled via cargo

### Running a ROM

```bash
cargo run --release -- roms/PONG.ch8
```

Or with the terminal backend:

```bash
cargo run --release -- roms/PONG.ch8 --backend terminal
```

### Options

```
chip8-emu <rom_path> [options]

Options:
  --backend <sdl2|terminal>  Display backend (default: terminal)
  --hz <N>                   CPU clock speed in Hz (default: 700)
  --debug                    Enable debug logging
  --help                     Show help
  --version                  Show version
```

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## Test ROMs

Validation is done using the [Timendus Chip-8 Test Suite](https://github.com/Timendus/chip8-test-suite). Recommended order:

1. `1-chip8-logo` — Basic display and font loading
2. `2-ibm-logo` — IBM logo, core instruction test
3. `3-corax` — Corax+ instruction test
4. `4-flags` — Flag behavior validation
5. `5-quirks` — Quirk compatibility testing

Game ROMs are available from [kripod/chip8-roms](https://github.com/kripod/chip8-roms).

## Architecture

```
ROM at 0x200 → [Mem 4096] → fetch 2 B → decode → execute → draw/timer → loop
                    Font at 0x000                           PC += 2     60 Hz
```

| Component | Description |
|-----------|-------------|
| **CPU** | 16×V regs (8-bit), I (16), PC (12), SP (8), DT/ST (8) |
| **Memory** | 4096 B flat RAM, programs loaded at 0x200, font at 0x000 |
| **Stack** | 16 levels LIFO |
| **Display** | 64×32 monochrome, XOR draw, collision detection |
| **Input** | 16-key hex keypad |
| **Timers** | DT/ST decrement at 60 Hz |

## Quirks Support

Configurable behavior for compatibility with original COSMAC VIP, modern interpreters, and HP48.

| Quirk | COSMAC VIP (orig) | Modern (default) |
|-------|-------------------|-------------------|
| Shift | `Vx >>= 1` | `Vx = Vy >> 1` |
| I inc | I += N | I unchanged |
| Wait | Wait VBlank | Immediate |
| VF | Preserved | VF = 0 |
| Jump | `NNN+V0` only | `NNN+Vx` (HP48) |

## References

- [Cowgod Chip-8 Technical Reference](http://devernay.free.fr/hacks/chip8/C8TECH10.HTM) — Canonical opcode spec
- [Write a Chip-8 Emulator](https://tobiasvl.github.io/blog/write-a-chip-8-emulator/) — Best educational walkthrough
- [Timendus Test Suite](https://github.com/Timendus/chip8-test-suite) — Validation standard
- [Chip-8 Community Spec](https://chip-8.github.io/) — Spec clarifications + extensions
- [Octo](https://github.com/JohnEarnest/Octo) — Modern CHIP-8 assembler/IDE

## License

MIT — see [LICENSE](LICENSE) for details.

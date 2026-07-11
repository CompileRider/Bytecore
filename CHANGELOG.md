# Changelog

All notable changes to Bytecore will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3] - 2026-07-11

### Fixed
- Font memory address corrected from 0x000 to 0x050 in documentation
- Quirks table: swapped shift columns (COSMAC VIP vs Modern), corrected I increment notation
- `cargo fmt --check` command syntax fixed to `cargo fmt -- --check`

### Changed
- `opcode.rs`: renamed `from()` to `decode()` returning `Result<Opcode, OpcodeError>`
- `opcode.rs`: renamed `ShrVx`/`ShlVx` to `ShrVxVy`/`ShlVxVy` with `(u8, u8)` parameters
- `opcode.rs`: added `OpcodeError` type with `#[non_exhaustive]`, `Display`, and `Error` traits
- `opcode.rs`: added `TryFrom<u16>` implementation for `Opcode`
- `opcode.rs`: added `Clone` and `Copy` derives
- `keypad.rs`: bounds safety using `get()` instead of direct indexing
- `keypad.rs`: added `Clone` and `Copy` derives
- `stack.rs`: replaced `panic!` with `Result<_, StackError>` return types
- `stack.rs`: added `StackError` type with `#[non_exhaustive]`, `Display`, and `Error` traits
- `stack.rs`: renamed `memory` field to `entries`
- `timers.rs`: changed `pub` fields to `pub(crate)`
- `mod.rs`: prefixed unused fields with `_` to suppress dead_code warnings

### Added
- CI/CD pipeline with automated releases
- Cross-platform binaries (Linux amd64/arm64, macOS arm64, Windows amd64)
- Docker images on ghcr.io (terminal-only and SDL2 variants)
- Automated patch version bumping on merge to main

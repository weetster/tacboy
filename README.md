# tacboy

`tacboy` is an experimental Game Boy emulator built primarily in [Tacit](https://github.com/weetster/tacit), with a small Rust host layer for ROM access, display, input, and generated FFI bindings.

The project is currently focused on emulator bring-up and correctness work rather than polished end-user packaging. The long-term target is to pass blargg CPU test ROMs while keeping as much of the emulator core as possible inside Tacit.

## Architecture

This repository follows a "Tacit core, Rust host" split:

- `src/`: emulator logic in Tacit
- `host/`: Rust binary that loads ROMs, binds Tacit callbacks, and presents output through headless, terminal, or SDL frontends
- `docs/`: development notes and emulator plan

Tacit owns the emulator working set and instruction logic. The Rust host owns the pieces that are better handled outside Tacit, including:

- reading cartridge ROM bytes
- serial output to stdout
- framebuffer presentation
- joypad polling
- generated host bindings for the exported Tacit entrypoint

## Current Status

This is an in-progress emulator, not a feature-complete Game Boy implementation.

Current code includes:

- a Tacit `run` export used as the host entrypoint
- cartridge ROM reads bridged through host callbacks
- serial output support for test ROMs
- joypad polling
- framebuffer presentation hooks
- multiple host frontends: `headless`, `tui`, and `sdl`

For current emulator goals and staged implementation notes, see [docs/emulator-plan.md](docs/emulator-plan.md).

## Repository Layout

```text
src/
  main.tac         Exported Tacit entrypoint
  cpu.tac          CPU implementation
  mem.tac          Memory map helpers
  ppu.tac          PPU/framebuffer logic
  timer.tac        Timer registers and timing logic
  interrupts.tac   Interrupt handling
  joypad.tac       Joypad state handling
  serial.tac       Serial port behavior
  cart.tac         Cartridge parsing
  mbc.tac          Mapper state
  regs.tac         Register layout/helpers
  machine.tac      Top-level machine state
  cb.tac           CB-prefixed opcodes

host/
  src/main.rs      Host executable
  src/frontend.rs  Headless, terminal, and SDL frontends
  build.rs         Loads generated Tacit host artifacts

docs/
  emulator-plan.md
```

## Prerequisites

You need:

- the Tacit toolchain pinned by [`tacit-toolchain.toml`](tacit-toolchain.toml) (currently `0.7.9`)
- Rust/Cargo
- SDL2 development libraries for the SDL frontend

The host build expects generated Tacit host artifacts to exist under `.tacit/derived/.../host`. If they are missing, generate them with:

```bash
tacit interface . --emit-library
```

## Building

Check the Tacit package:

```bash
tacit lock
tacit check . --format json
tacit test . --format json
```

Generate the Tacit host interface:

```bash
tacit interface . --emit-library
```

Build the Rust host:

```bash
cd host
cargo build
```

## Running

Run the host binary with a ROM path and optional cycle limit:

```bash
cd host
cargo run -- /path/to/rom.gb 5000000
```

Arguments:

- first positional: ROM path
- second positional: max cycles

Cycle count also accepts `inf`, `infinite`, `unlimited`, `none`, or `0` for an effectively unbounded run.

Frontend selection is done with `--frontend`:

```bash
cargo run -- --frontend headless /path/to/rom.gb
cargo run -- --frontend tui /path/to/rom.gb
cargo run -- --frontend sdl /path/to/rom.gb
```

Additional frontend options:

- `--size COLSxROWS` for the terminal frontend, or window size for SDL
- `--color 256` or `--color truecolor` for the terminal frontend

The host prints serial bytes from the emulated Game Boy to stdout, which is useful for ROM-driven test output.

If `TACBOY_DUMP` is set, each presented frame is written to that path, leaving the last produced raw frame on disk.

## Development Notes

Tacit source in this repository is stored canonically as `.tac` plus generated `.tacd` metadata. Do not hand-edit `.tac` files directly. Follow the round-trip workflow described in [CLAUDE.md](CLAUDE.md):

1. Render a `.tac` file to authoring syntax outside the repo.
2. Edit the scratch `.taca`.
3. Canonicalize back into `src/`.
4. Refresh `tacit.lock`.
5. Regenerate the host interface if exported hashes changed.

In practice, the usual validation flow is:

```bash
tacit lock
tacit check . --format json
tacit test . --format json
tacit interface . --emit-library
cd host
cargo build
```

## Why Tacit?

`tacboy` is partly an emulator project and partly a vehicle for exploring how far a nontrivial systems program can be pushed in [Tacit](https://github.com/weetster/tacit). The Rust host is intentionally thin; the core emulator logic is meant to live in Tacit wherever practical.

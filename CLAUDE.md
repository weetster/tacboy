# tacboy Agent Instructions

tacboy is a Game Boy emulator written in Tacit-Lite, with a thin Rust host
that bridges to SDL, mmap'd ROM, and stdio. The toolchain is pinned by
`tacit-toolchain.toml` (currently 0.7.7).

## Vision and current state

**Vision.** All blargg CPU test ROMs (`cpu_instrs`, `instr_timing`, and
ideally `mem_timing*`) pass with a real GB CPU + PPU implemented in Tacit.
See `docs/emulator-plan.md` for the staged plan, target test ROMs, and
known risks.

**Architecture: Shape B.** Tacit owns the working set (register file, WRAM,
VRAM, OAM, HRAM, I/O page, MBC state, framebuffer). The Rust host owns only
what Tacit can't or shouldn't: mmap'd ROM (carts can exceed `@u8vec-alloc`
limits), SDL window/audio, stdout, input polling. Host imports are declared
in Tacit as capability callbacks and satisfied by implementing the generated
`TacboyCallbacks` trait in `host/src/main.rs`.

**Current state.** Stage 2 skeleton landed. `src/main.tac` exports
`run : Int -> Int -> Int / {Alloc, Div, IO, Mut}` taking (rom_len,
max_cycles). It allocates the GB working set (VRAM, ERAM, WRAM, OAM, IO,
HRAM, IE, an `@i64-alloc` MBC slot, and a 16-slot `@i64-alloc` register
file laid out as `regs[0..7]=B,C,D,E,H,L,_,A`, `regs[8]=F`, `regs[9]=SP`,
`regs[10]=PC`, `regs[11]=IME`, `regs[12]=HALTED`, `regs[13]` scratch for
the last unknown opcode). `read8`/`write8` are a `rec` group with
region-dispatched if/else chains; writes to 0xFF02 with the high bit set
trigger the host `write_serial` callback with the SB byte and then clear
SC. The CPU loop is an immediate `@loop` over a cycle counter that
fetches PC, advances, dispatches on the opcode, and recognises NOP,
HALT, JR, JP nn, and LD A,A — every other opcode stashes itself in
`regs[13]` and exits with code 2. Return codes from `run`: 0=HALT,
1=cycle cap, 2=unknown opcode. The host implements both `rom_byte` and
`write_serial`, defaults to 5,000,000 cycles, and prints each serial
byte to stdout as it arrives. Against blargg `cpu_instrs.gb` the
skeleton currently runs ~handful of instructions before hitting an
opcode it doesn't know — that's the expected hand-off point for the
next session, which should expand the opcode table.

## Reading the language and workflow contracts

- Run `tacit primer` to print the Tacit-Lite language primer that matches
  this toolchain. Read it before writing or editing source. Do not copy
  primer prose from another repository or another toolchain version.
- The agent workflow companion is installed at
  `share/tacit/workflow/agent-workflow.md` in the toolchain prefix
  (`/usr/local/share/tacit/workflow/agent-workflow.md` on this machine).
  Read it before running tools.

## Editing source

Do not hand-edit `.tac` files. The `.tac` format is canonical S-expression
bytes with BLAKE3 definition-hash references; the primer teaches the
authoring view (`.taca`), which is a different surface syntax. The two do
not line up token for token, and `.tac` hashes change with every edit. Edit
via the round-trip loop instead:

1. Render existing source as authoring view to a scratch path outside the
   project, e.g.
   `tacit render src/main.tac --as authoring -o /tmp/tacboy2.taca`.
2. Edit the scratch `.taca` using the authoring-view syntax from the
   primer.
3. Canonicalize back into the project:
   `tacit canonicalize /tmp/tacboy2.taca -o src/main.tac --force`. That
   rewrites both `src/main.tac` and `src/main.tacd`.
4. Delete the scratch `.taca`. Do not check `.taca` files into this
   project.

After any source edit, definition hashes change. Run `tacit lock` to refresh
`tacit.lock`, then update any `[exports]`, `[bin]`, or `[[tests]]` entries
in `tacit.toml` that reference the old hashes. Use
`tacit view src --as inspection --hashes` (or read `tacit.lock`) to look up
the new ones. If the public-export hash changed, also regenerate host
bindings with `tacit interface . --emit-library` and update the matching
`tacit_p_<…>_e_<…>` symbol in `host/src/main.rs`.

## Tacit is experimental — three-strikes rule

Tacit-Lite is an actively evolving experimental language. The primer
documents the intended surface, but the implementation has real gaps and
parser quirks that aren't in the docs (the per-toolchain gotchas below are
the ones found so far). If you try three reasonable variations of a
language feature — different syntaxes, different placements, different
type annotations, etc. — and none of them work, **stop and ask the user**.
A persistent failure on something the primer implies should work is more
likely a missing language feature than a mistake in your code. Surfacing it
to the user lets them weigh implementing it in the language vs.
working around it in tacboy, instead of you silently choosing a worse
workaround. Examples of "ask, don't grind": effect annotations in a
position the parser rejects, a primitive that the naming convention
implies should exist but doesn't resolve, a record/closure shape the type
checker keeps refusing.

## Tacit-Lite gotchas pinned to this toolchain (0.7.7)

- **`tacit check` is stale after `canonicalize`.** It only re-checks bodies
  after `tacit lock` runs. Always sequence as `canonicalize` → `lock` →
  `check`, or you can ship a "green" build that contains real type errors.
- **Typed-vector capture in callbacks.** An *immediate* `@loop` callback
  (lambda literally written as the second arg to `@loop`) may access
  `u8vec` / `Buf` / `I64Vec` handles from the surrounding scope. Indirect
  closures, `@for-each`, `@map`, `@fold`, etc. may not. If you need handle
  access in a non-immediate callback, fall back to a `rec` helper (and add
  `Div` to the effect set).
- **`@loop` adds `Div`** to the enclosing function's effect set per primer
  §3.

## Hand-off

Before handing changes back: `tacit lock`, then `tacit check . --format
json` (confirm `errors: []`), then `tacit test . --format json`. If the
host export hash changed, also `tacit interface . --emit-library` and
`cargo build` (and ideally `cargo run`) from `host/`.

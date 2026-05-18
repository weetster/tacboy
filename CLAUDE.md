# tacboy Agent Instructions

tacboy is a Game Boy emulator written in Tacit-Lite, with a thin Rust host
that bridges to SDL, mmap'd ROM, and stdio. The toolchain is pinned by
`tacit-toolchain.toml` (currently 0.7.6).

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

**Current state.** Stage 0 is done: toolchain pinned, Shape B skeleton wired
end-to-end through the host trait, dispatch loop uses an immediate `@loop`
callback (the 0.7.6 form that may capture typed-vector handles from outer
scope). `src/main.tac` still runs a 4-instruction toy VM (LD A,n / LD B,n /
ADD A,B / HALT) on a ROM passed in as a `u8vec` borrow; `cargo run` from
`host/` prints the result of one tiny program. Stage 1 (real cart loader +
region-dispatched memory map) is the next step.

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

## Tacit-Lite gotchas pinned to this toolchain (0.7.6)

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

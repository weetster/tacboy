# tacboy Emulator Plan: Passing blargg CPU Tests

Goal: pass all blargg CPU test ROMs (`cpu_instrs` and `instr_timing` at minimum;
`mem_timing*`, `halt_bug`, `oam_bug`, `interrupt_time` as stretch) while
keeping as much of the emulator in Tacit as possible. Architecture is Shape B
(Tacit owns the working set; the Rust host bridges to SDL, ROM mmap, stdio,
and input).

## Strategy

**The lever:** blargg's CPU ROMs write their pass/fail report to the serial
port (`SB` = FF01, `SC` = FF02). Writing `0x81` to `SC` means "send this
byte." So we can validate `cpu_instrs` end-to-end with **no PPU**: the host
sinks serial bytes to stdout and we look for `"Passed"`. PPU comes later for
visual output. This collapses the critical path to CPU + memory map + serial.

**Shape B division:**

- **Tacit owns:** register file, WRAM (8 KB), VRAM (8 KB), OAM (160 B),
  HRAM (127 B), I/O page (128 B), MBC bank state, interrupt-enable/flag,
  timer regs, PPU mode/scanline, the `@loop` dispatch.
- **Host owns:** the mmap'd ROM (>32 KB exceeds Tacit's `@u8vec-alloc` limits
  for some carts), SDL window, stdout serial sink, input polling, frame
  pacing.
- **Host imports (capability callbacks):** `read-rom`, `write-serial`,
  `present-frame`, `poll-input`, `queue-audio`.

This keeps ROM out of Tacit (it can be 32 KB–8 MB depending on cartridge)
while every cycle of simulation logic stays in Tacit.

## Source tree

```
tacboy/
  src/
    cart.tac         cartridge header parse + MBC-type detection
    mbc.tac          MBC1/MBC3/MBC5 bank-switch state
    mem.tac          region dispatch: read8 / write8 / (helpers for read16)
    regs.tac         register file layout + flag (F) helpers
    cpu.tac          fetch/decode/execute for one instruction; returns cycles
    cb.tac           the CB-prefixed opcode table (bit/rotate/shift)
    interrupts.tac   IF/IE handling + interrupt vector dispatch
    timer.tac        DIV / TIMA / TMA / TAC
    serial.tac       FF01/FF02 — bytes out via host callback
    ppu.tac          LCD modes, scanline counter, VBlank, framebuffer write
    joypad.tac       FF00 button state
    machine.tac      top-level state record + per-cycle @loop
    main.tac         exports `run` (the only host entry point)
  host/
    src/main.rs      SDL window, ROM mmap, serial→stdout, input
    Cargo.toml, build.rs
  tests/             rom-driven harness (calls run, captures serial)
  docs/
```

One open question: **whether Tacit packages support multiple unit files in a
single `[package]`.** Workflow doc implies yes (the `[exports]` table can
reference any unit), but it's unverified. If it doesn't, we start with one
`main.tac` and split when the language gets there. Confirm in stage 1.

## Stages

### Stage 1 — Cart + memory map (no CPU yet)

- Replace the toy 6-byte VM with: load a real ROM via host, parse the cart
  header (title, MBC type, ROM size, RAM size), allocate the region buffers
  in Tacit.
- Implement `read8` / `write8` as a Tacit function that switches on address
  range and dispatches: `< 0x4000` → `read-rom 0 addr`, `< 0x8000` →
  `read-rom bank addr`, `< 0xA000` → VRAM, etc. `write8` to ROM space updates
  MBC bank state.
- MBC1 only at this stage (cpu_instrs is MBC1).
- **Test:** read every byte of `cpu_instrs.gb` through Tacit's `read8` and
  verify it matches the raw bytes. Also measures `@loop` throughput: 64 KB
  sequential reads should be near-instantaneous.

### Stage 2 — CPU correctness, no timing

- Register file as a record `{a, f, b, c, d, e, h, l, sp, pc}` carried in
  `@loop` state. (Records of `Int` fit the loop-state rule.)
- Opcode dispatch: one big `match op with | ...` in `cpu.tac`, plus a
  separate CB table.
- All 245 unprefixed + 256 CB opcodes implemented. Flag semantics correct
  (Z/N/H/C). DAA included.
- Serial: `write8` to FF02 with value 0x81 calls host
  `write-serial(mem[FF01])` and clears bit 7 of FF02.
- Cycles returned but ignored.
- HALT implemented as a state flag; STOP optional (cpu_instrs doesn't use
  it).
- **Test:** run `01-special.gb` through `11-op a,(hl).gb` individually,
  scrape serial for `"Passed"` / `"Failed"`. Target: 11/11 pass.

### Stage 3 — Interrupts + timer + cycle counting

- DIV (FF04), TIMA (FF05), TMA (FF06), TAC (FF07) implemented with proper
  increment rates.
- IF (FF0F) and IE (FFFF), IME flag, interrupt vector dispatch (RST
  40/48/50/58/60).
- Each opcode returns its correct M-cycle count from the blargg timing
  table.
- `@loop` advances a `cycles_remaining` counter; main loop runs until N
  cycles consumed per frame (70,224 dots ÷ 4 = 17,556 M-cycles).
- **Test:** `instr_timing.gb` passes. `cpu_instrs.gb` (multi-rom variant)
  passes — this needs interrupts working because it uses them to coordinate
  sub-tests.

### Stage 4 — PPU + display

- Mode 2 (OAM scan) / mode 3 (transfer) / mode 0 (HBlank) / mode 1 (VBlank)
  with cycle-accurate-enough transitions to fire `LY=144` → VBlank
  interrupt at the right moment.
- Background tile rendering (window + sprites in a later substage if
  needed).
- 160×144 framebuffer as a `u8vec` (palette-indexed) in Tacit; per-frame
  `present-frame` callback hands the buffer to host SDL.
- LCDC, STAT, SCX/SCY, LY, LYC, BGP, OBP0/1, WX/WY all in the I/O page.
- **Test:** visually verify `cpu_instrs.gb` shows "Passed". `dmg-acid2` (if
  desired) for PPU correctness — not strictly needed for blargg CPU.

### Stage 5 (stretch) — Sub-instruction timing + edge cases

- `mem_timing.gb`, `mem_timing-2.gb` — requires modeling memory access at
  the M-cycle level within each instruction.
- `halt_bug.gb`, `oam_bug.gb`, `interrupt_time.gb`.
- These each have their own quirks; optional for "all CPU tests pass"
  depending on how strictly that's read.

## Risks worth naming up front

1. **`@loop` throughput at GB scale.** `cpu_instrs.gb` runs for ~30 seconds
   on real hardware ≈ 125M M-cycles. Tacit's `@loop` lowers to a
   basic-block back-edge per the primer, but it hasn't been measured at this
   scale. **First measurement happens in stage 1** (read 64 KB via Tacit
   dispatch); if it's prohibitive, we'd need to push hot paths through host
   imports, which would erode Shape B.
2. **Record-of-Int loop state with ~10 fields, mutated every cycle.**
   Should be fine but it's a new pattern for this project — worth a
   microbenchmark in stage 2.
3. **MBC + region dispatch through Tacit on every memory access.** A
   `match` on the high nibble per access. Stage 1 tells us whether the
   per-access cost is acceptable.
4. **Multi-unit packages.** If Tacit doesn't support multiple `.tac` files
   per package today, the tree above collapses to one large `main.tac`
   until it does. The logical division is still useful as documentation.

## Commitment

Stages 1–3 deliver "all blargg CPU instruction tests pass." Stage 4 adds
visible output. Stage 5 is for completeness.

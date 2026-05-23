# Tacit-Owned Game Boy APU Plan

Goal: implement Game Boy audio while preserving tacboy's Shape B boundary:
Tacit emulates Game Boy hardware state and timing; the Rust host only moves
data between Tacit, SDL, ROM/input, and stdout.

## Host Boundary

The host should not emulate sound channels, registers, envelopes, sweep,
frequency timers, wave RAM, length counters, or mixing policy.

Allowed host responsibilities:

- Queue PCM bytes produced by Tacit to SDL audio.
- Report queue capacity/backpressure if Tacit needs it.
- Keep existing non-audio callbacks: ROM bytes, serial output, frame
  presentation, joypad polling, SRAM load/save.

Target audio callback:

```text
queue_audio : u8vec -> Int -> Int / {IO}
```

Tacit owns the sample buffer contents. The host receives a byte vector and
logical byte count, queues those bytes to SDL, and returns a status/count.
PCM format should initially be signed 16-bit little-endian stereo at 48 kHz.

## Stage 1 - Restore Green Baseline

Status: complete as of this plan. The interrupted host-backed APU edit was
reverted, and `tacit check . --format json` reports `errors: []`.

## Stage 2 - Split Machine Responsibilities

Status: complete on Tacit 0.7.10. Bus helpers now live in `bus.tac`, take
explicit state and callback parameters, and pass host-interface generation.

Objective: reduce `machine.tac` enough that APU state can be added without
making the top-level loop unmaintainable.

- Keep `machine.tac` responsible for allocation, reset defaults, and the main
  cycle loop.
- Extract bus helpers into `bus.tac` with explicit parameters for ROM/MBC,
  RAM, IO, HRAM, IE, regs, PPU state, APU state, and callbacks.
- Preserve current behavior before adding audio semantics.
- Avoid first-class closures that capture vectors; use direct function calls
  with explicit state handles, matching Tacit's region/capture constraints.

Validation:

- `tacit lock`
- `tacit check . --format json`
- `tacit test . --format json`
- `tacit interface . --emit-library`
- `cargo build` in `host/`

## Stage 3 - Define APU State and Register Surface

Objective: model the APU memory-mapped register interface in Tacit.

- Add `apu.tac`.
- Allocate APU state in `machine.tac`, likely using `u8vec` for NR registers
  and wave RAM plus `i64vec` for counters/phases.
- Route `0xFF10..0xFF3F` through APU read/write helpers in the bus layer.
- Implement readable/write-only masks for NR10-NR52 and wave RAM behavior.
- Implement NR52 global enable/disable behavior, including clearing channel
  state when power is disabled.

Validation:

- Add focused Tacit tests for register masks, wave RAM storage, and NR52
  enable/status reads.
- Confirm non-audio ROM behavior remains unchanged.

## Stage 4 - Add APU Timing Skeleton

Objective: advance APU time from the same elapsed cycle delta used by timer
and PPU.

- Add `apu_step(cycles, state, pcm_buffer)` called once per main-loop
  iteration after CPU cycle advancement is known.
- Implement the 512 Hz frame sequencer.
- Implement length-counter clocking.
- Implement envelope clocking.
- Implement sweep clocking for channel 1.
- Add fixed-point sample-rate accumulation from Game Boy CPU cycles to 48 kHz
  output samples.

Validation:

- Add tests for frame sequencer phase progression at known cycle counts.
- Add tests for sample accumulator output count over one video frame.

## Stage 5 - Implement Channels Incrementally

Status: complete on Tacit 0.7.10. All four channels produce per-channel
amplitudes in Tacit; APU i64 state grew from 24 to 29 slots to host CH4
length/envelope/freq-timer/LFSR. Mixing remains in Stage 6.

- Channel 1: square wave with duty, frequency timer, length, envelope, and
  sweep.
- Channel 2: square wave with duty, frequency timer, length, and envelope.
- Channel 3: wave channel with wave RAM, DAC enable, output level, frequency
  timer, playback position, and length.
- Channel 4: noise channel with LFSR width mode, divisor/shift frequency,
  length, and envelope.

Validation:

- Unit-test trigger behavior for each channel.
- Unit-test DAC-disable behavior.
- Unit-test length/envelope/sweep edge cases separately from mixing.

## Stage 6 - Mix and Queue PCM

Status: complete on Tacit 0.7.10. Tacit owns a 2 KB `u8vec` PCM buffer
(512 stereo s16 frames at 48 kHz) and a write-position cursor in APU
i64 slot 29. `apu_step` was extended to take wave RAM and the PCM buffer;
on every sample boundary it mixes via `apu_emit_sample` and returns 1 when
the buffer fills, at which point `machine.tac` calls the host
`queue_audio` capability. Headless/tui keep `queue_audio` as a no-op so
CPU/PPU behaviour is unchanged; the SDL frontend opens a 48 kHz s16 stereo
audio queue and feeds it the bytes Tacit produced.

- Implement NR50 master volume and VIN bits as volume controls.
- Implement NR51 left/right channel routing.
- Mix four channel amplitudes to signed 16-bit stereo.
- Fill a bounded `u8vec` PCM buffer with little-endian samples.
- Call `queue_audio pcm byte_count` when the buffer reaches a practical chunk
  size, such as 512 or 1024 frames.
- Clamp output to avoid overflow.

Validation:

- Confirm headless mode can leave `queue_audio` as a no-op without affecting
  CPU/PPU tests.
- Confirm SDL frontend queues and plays audio without moving emulation logic
  into Rust.

## Stage 7 - Accuracy Pass

Status: complete on Tacit 0.7.10 for the well-documented length/sweep
quirks. APU i64 state grew from 30 to 31 slots to host slot 30 = "CH1
sweep negate-calc-since-trigger" flag.

Landed:

- NR52 power-off on DMG now preserves length counters in slots 7/16/21/24
  (only channel-enable and channel-state slots are cleared, plus the
  sweep negate-calc flag).
- While APU is powered off, writes to NR11/NR21/NR31/NR41 (via the new
  `apu_is_nrx1_addr` predicate) still route through
  `apu_handle_length_write` to update length counters; other NR writes
  remain ignored.
- Extra length clock on NRx4 length-enable transition: new
  `apu_handle_nrx4_length_enable` runs before the NR-vec store. When
  enable goes 0->1 and the next frame-sequencer step is non-length-clock
  and length > 0, length is decremented; if it reaches 0 with no
  concurrent trigger, the channel is disabled.
- Trigger-time length clock: each `apu_chN_trigger` reads the new NRx4
  byte (already stored by `apu_write8`) and, when length-enable is 1 and
  the next FS step is non-length-clock, decrements the just-reloaded
  length once.
- Sweep negate-mode clear: `apu_ch1_clock_sweep` and `apu_ch1_trigger`
  set i64 slot 30 to 1 on any sweep calc performed in negate mode; the
  new `apu_handle_nr10_write` runs on NR10 writes and disables CH1 when
  the negate bit goes 1->0 after such a calc. The trigger function
  clears slot 30 on every trigger.

Deferred:

- Wave RAM "returns current sample while CH3 playing" / DMG wave-write
  windows: skipped because no APU test ROMs are available locally, so
  the quirk has no driver to validate against.
- Add APU test ROMs to the manual validation list: skipped for the same
  reason; revisit when test ROMs land in the repo.
- Effectful unit tests covering the new mutating helpers
  (`apu_clear_powered_state`, `apu_handle_nrx4_length_enable`,
  `apu_handle_nr10_write`, trigger-time length clocks): Tacit 0.7.10's
  package-test surface accepts `Bool` test definitions without
  per-definition effect annotations, and the value-type signature
  `Bool / {Alloc, Mut}` does not parse. Two pure-logic tests landed
  (`apu-nrx4-quirk-phases`, `apu-is-nrx1-addr`) covering the
  decision-logic edges; deeper coverage of the mutating helpers
  requires either toolchain support for effectful value tests or
  ROM-based validation.

Validation:

- `tacit lock` / `tacit check . --format json` / `tacit test .`
  (18 tests, all passing).
- `tacit interface . --emit-library` regenerated.
- `cargo build` in `host/` succeeds.

## Risks

- Splitting the existing monolithic `machine.tac` may expose Tacit language
  limits around passing vector handles through package exports.
- A fully Tacit mixer may be CPU-heavy; if throughput becomes a blocker, keep
  hardware state in Tacit and consider a narrowly scoped host PCM conversion
  only after measuring.
- The current source is generated-looking authoring output, so small semantic
  edits can produce large canonical diffs. Each stage should preserve green
  checks before starting the next.

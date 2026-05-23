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

Objective: mix Tacit-owned channel output into a host-queued audio buffer.

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

Objective: tighten behavior against known APU edge cases after audible output
works.

- Audit trigger timing and length-counter reload quirks.
- Audit sweep overflow disabling for channel 1.
- Audit wave RAM access quirks if test ROMs require them.
- Audit frame sequencer behavior when writing NR52/NR14/NR24/NR34/NR44.
- Add APU test ROMs to the manual validation list if available locally.

## Risks

- Splitting the existing monolithic `machine.tac` may expose Tacit language
  limits around passing vector handles through package exports.
- A fully Tacit mixer may be CPU-heavy; if throughput becomes a blocker, keep
  hardware state in Tacit and consider a narrowly scoped host PCM conversion
  only after measuring.
- The current source is generated-looking authoring output, so small semantic
  edits can produce large canonical diffs. Each stage should preserve green
  checks before starting the next.

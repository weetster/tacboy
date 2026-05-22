//! Host-side display frontends.
//!
//! Under Shape B, Tacit owns the framebuffer and hands the host a finished
//! frame through the `present_frame` capability callback. A `Frontend` is
//! purely a host-side concern: it turns that byte slice into something a
//! human can see. Adding a frontend never touches `.tac` source.
//!
//! Frame format (from `ppu.tac`): 160x144 pixels, one byte per pixel, row
//! major, each value a Game Boy palette index 0..=3 (0 = lightest).

use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::os::raw::{c_int, c_void};
use std::process::Command;
use std::time::{Duration, Instant};

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;

pub const FB_WIDTH: usize = 160;
pub const FB_HEIGHT: usize = 144;
pub const FB_LEN: usize = FB_WIDTH * FB_HEIGHT;

/// One Game Boy frame at the DMG refresh rate (59.7275 Hz).
const FRAME_DURATION: Duration = Duration::from_nanos(16_742_706);

/// The four DMG shades, lightest (0) to darkest (3), given as both a 24-bit
/// RGB triple and the nearest xterm-256 palette index. The Game Boy only
/// has these four colors, so 256-color is visually indistinguishable from
/// truecolor here — but far more widely supported over SSH and in tmux.
const PALETTE: [((u8, u8, u8), u8); 4] = [
    ((155, 188, 15), 148),
    ((139, 172, 15), 142),
    ((48, 98, 48), 65),
    ((15, 56, 15), 22),
];

/// How to encode color in the escape stream.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// `\x1b[38;5;Nm` — xterm-256 indexed. The compatible default.
    Indexed256,
    /// `\x1b[38;2;r;g;bm` — 24-bit truecolor. Exact, but many terminals and
    /// multiplexers silently drop it, which makes every half-block render in
    /// default colors (uniform horizontal stripes).
    Truecolor,
}

impl ColorMode {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "256" | "indexed" => Some(ColorMode::Indexed256),
            "truecolor" | "24bit" | "rgb" => Some(ColorMode::Truecolor),
            _ => None,
        }
    }

    /// Emit a foreground (`base` 38) or background (`base` 48) color escape
    /// for DMG `shade` (0..=3) into `buf`.
    fn write_escape(self, buf: &mut String, base: u8, shade: u8) {
        let ((r, g, b), idx) = PALETTE[(shade & 3) as usize];
        match self {
            ColorMode::Indexed256 => {
                let _ = write!(buf, "\x1b[{base};5;{idx}m");
            }
            ColorMode::Truecolor => {
                let _ = write!(buf, "\x1b[{base};2;{r};{g};{b}m");
            }
        }
    }
}

/// A display target for finished frames. Restoration of any terminal/window
/// state happens in the implementor's `Drop`, so callers only need to drop
/// the `Frontend` (e.g. by dropping the owning context) to clean up.
pub trait Frontend {
    /// Present one completed frame: `frame` is `FB_LEN` bytes, values 0..=3.
    /// Implementations must tolerate a short/long slice without panicking.
    fn present(&mut self, frame: &[u8]);

    /// Return the current raw joypad byte. Bit layout (1=pressed):
    /// bit0=Right, bit1=Left, bit2=Up, bit3=Down, bit4=A, bit5=B, bit6=Select, bit7=Start
    fn joypad_state(&self) -> u8 {
        0
    }
}

/// Selectable by name, so `main` can map a `--frontend` flag to a value.
/// `size` overrides terminal-size detection for `tui`, or sets the window
/// pixel dimensions (WxH) for `sdl`; `color` is `tui`-only.
pub fn make(
    name: &str,
    size: Option<(u16, u16)>,
    color: Option<&str>,
) -> Result<Box<dyn Frontend>, String> {
    match name {
        "headless" | "none" => Ok(Box::new(HeadlessFrontend)),
        "tui" | "terminal" => {
            let color = match color {
                None => ColorMode::Indexed256,
                Some(s) => ColorMode::parse(s)
                    .ok_or_else(|| format!("unknown --color '{s}' (expected: 256, truecolor)"))?,
            };
            Ok(Box::new(TerminalFrontend::new(size, color)))
        }
        "sdl" => {
            let (win_w, win_h) = size
                .map(|(w, h)| (w as u32, h as u32))
                .unwrap_or((FB_WIDTH as u32 * 3, FB_HEIGHT as u32 * 3));
            SdlFrontend::new(win_w, win_h).map(|f| Box::new(f) as Box<dyn Frontend>)
        }
        other => Err(format!(
            "unknown frontend '{other}' (expected: headless, tui, sdl)"
        )),
    }
}

/// Does nothing with the frame. Preserves the original host behaviour, so
/// headless blargg runs (serial-only output on stdout) are unaffected.
pub struct HeadlessFrontend;

impl Frontend for HeadlessFrontend {
    fn present(&mut self, _frame: &[u8]) {}
}

/// Renders to the terminal with truecolor half-block characters: the upper
/// half-block `▀` carries the top pixel as foreground and the bottom pixel
/// as background, so each character cell shows two vertically-stacked
/// pixels. Escape codes are emitted only when a color changes, which keeps
/// the per-frame byte count low enough to be comfortable over SSH.
///
/// The frame is nearest-neighbour scaled to fit the terminal (detected once
/// at construction) while preserving the 160:144 aspect ratio, so it never
/// line-wraps or scrolls. Presents are paced to the DMG refresh rate, which
/// is what keeps a no-cycle-limit run going at the correct game speed.
pub struct TerminalFrontend {
    /// Reused across frames to avoid reallocating tens of KB each present.
    buf: String,
    /// Scaled output size in pixels; `out_h` is always even (half-blocks).
    out_w: usize,
    out_h: usize,
    color: ColorMode,
    /// Deadline for the next present, advanced by one frame each time.
    next_frame: Option<Instant>,
}

impl TerminalFrontend {
    pub fn new(size: Option<(u16, u16)>, color: ColorMode) -> Self {
        let (cols, rows) = size.or_else(detect_terminal_size).unwrap_or((80, 24));
        let (out_w, out_h) = fit(cols as usize, rows as usize);

        // The cursor is about to be hidden, so make sure Ctrl+C (and a kill)
        // still un-hide it instead of leaving the user with a broken prompt.
        install_signal_handlers();

        // Hide the cursor and clear the screen once up front.
        print!("\x1b[?25l\x1b[2J");
        let _ = io::stdout().flush();
        Self {
            buf: String::new(),
            out_w,
            out_h,
            color,
            next_frame: None,
        }
    }

    /// Block until the next frame's deadline so the emulator runs at GB speed.
    fn pace(&mut self) {
        let now = Instant::now();
        let target = self.next_frame.unwrap_or(now);
        if now < target {
            std::thread::sleep(target - now);
        }
        // Advance the deadline, but resync instead of accumulating debt if a
        // slow present (or terminal) has already put us a frame behind.
        let advanced = target + FRAME_DURATION;
        self.next_frame = Some(advanced.max(Instant::now()));
    }
}

impl Frontend for TerminalFrontend {
    fn present(&mut self, frame: &[u8]) {
        if frame.len() < FB_LEN {
            return;
        }
        let (out_w, out_h, color) = (self.out_w, self.out_h, self.color);
        let buf = &mut self.buf;
        buf.clear();
        buf.push_str("\x1b[H"); // cursor home; overdraw the previous frame

        let cell_rows = out_h / 2;
        for cell_row in 0..cell_rows {
            // A fresh `\x1b[0m` at each row end resets the active colors, so
            // the change tracking must restart per row.
            let mut last_fg: Option<u8> = None;
            let mut last_bg: Option<u8> = None;
            for ox in 0..out_w {
                let fg = sample(frame, ox, cell_row * 2, out_w, out_h);
                let bg = sample(frame, ox, cell_row * 2 + 1, out_w, out_h);
                if last_fg != Some(fg) {
                    color.write_escape(buf, 38, fg);
                    last_fg = Some(fg);
                }
                if last_bg != Some(bg) {
                    color.write_escape(buf, 48, bg);
                    last_bg = Some(bg);
                }
                buf.push('\u{2580}'); // ▀ upper half block
            }
            buf.push_str("\x1b[0m");
            // Skip the newline after the final row: printing it where the row
            // sits on the terminal's last line would scroll the whole frame.
            if cell_row + 1 < cell_rows {
                buf.push_str("\r\n");
            }
        }

        let stdout = io::stdout();
        let mut h = stdout.lock();
        let _ = h.write_all(buf.as_bytes());
        let _ = h.flush();
        drop(h);

        self.pace();
    }
}

impl Drop for TerminalFrontend {
    fn drop(&mut self) {
        // Reset colors and restore the cursor on the way out.
        let mut out = io::stdout();
        let _ = out.write_all(TERMINAL_RESTORE);
        let _ = out.flush();
    }
}

/// ANSI sequence that resets colors and re-shows the cursor — the minimum
/// needed to hand a sane terminal back to the user.
const TERMINAL_RESTORE: &[u8] = b"\x1b[0m\x1b[?25h\r\n";

// libc symbols, declared directly to avoid pulling in the `libc` crate.
// `signal`/`write`/`_exit` are always available in the linked C library.
extern "C" {
    fn signal(signum: c_int, handler: extern "C" fn(c_int)) -> usize;
    fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    fn _exit(code: c_int) -> !;
}

/// SIGINT/SIGTERM handler. A signal handler may only call async-signal-safe
/// functions, which rules out buffered stdio (`print!` takes a lock) — so it
/// restores the terminal with a raw `write` and exits via `_exit`.
extern "C" fn restore_on_signal(sig: c_int) {
    unsafe {
        let _ = write(
            1,
            TERMINAL_RESTORE.as_ptr() as *const c_void,
            TERMINAL_RESTORE.len(),
        );
        // 128 + signal number: the conventional shell exit code.
        _exit(128 + sig);
    }
}

/// Install the terminal-restoring handler for SIGINT (Ctrl+C) and SIGTERM.
/// `process::exit`/default signal disposition both skip `Drop`, so without
/// this a Ctrl+C would leave the cursor hidden.
fn install_signal_handlers() {
    const SIGINT: c_int = 2;
    const SIGTERM: c_int = 15;
    unsafe {
        signal(SIGINT, restore_on_signal);
        signal(SIGTERM, restore_on_signal);
    }
}

/// Renders into an SDL2 window scaled up from the native 160×144 framebuffer.
/// Default scale is 3× (480×432); pass `--size WxH` to override.
/// Quit on window-close or Escape; paced to the DMG 59.7 Hz refresh rate.
pub struct SdlFrontend {
    _sdl: sdl2::Sdl,
    canvas: Canvas<Window>,
    event_pump: sdl2::EventPump,
    next_frame: Option<Instant>,
    /// Bit-packed pressed-button state. Layout: bit0=Right, bit1=Left, bit2=Up,
    /// bit3=Down, bit4=A(Z), bit5=B(X), bit6=Select(Backspace), bit7=Start(Return)
    buttons: u8,
}

impl SdlFrontend {
    pub fn new(win_w: u32, win_h: u32) -> Result<Self, String> {
        let sdl = sdl2::init().map_err(|e| format!("SDL init: {e}"))?;
        let video = sdl.video().map_err(|e| format!("SDL video: {e}"))?;
        let window = video
            .window("tacboy", win_w, win_h)
            .position_centered()
            .build()
            .map_err(|e| format!("SDL window: {e}"))?;
        let canvas = window
            .into_canvas()
            .accelerated()
            .build()
            .map_err(|e| format!("SDL canvas: {e}"))?;
        let event_pump = sdl.event_pump().map_err(|e| format!("SDL event pump: {e}"))?;
        Ok(Self {
            _sdl: sdl,
            canvas,
            event_pump,
            next_frame: None,
            buttons: 0,
        })
    }

    fn pace(&mut self) {
        let now = Instant::now();
        let target = self.next_frame.unwrap_or(now);
        if now < target {
            std::thread::sleep(target - now);
        }
        let advanced = target + FRAME_DURATION;
        self.next_frame = Some(advanced.max(Instant::now()));
    }
}

impl Frontend for SdlFrontend {
    fn present(&mut self, frame: &[u8]) {
        if frame.len() < FB_LEN {
            return;
        }

        // Keep the SDL texture scoped to the present call instead of storing a
        // self-referential texture/creator pair with an invented lifetime.
        let texture_creator = self.canvas.texture_creator();
        let mut texture = match texture_creator.create_texture_streaming(
            PixelFormatEnum::RGB24,
            FB_WIDTH as u32,
            FB_HEIGHT as u32,
        ) {
            Ok(texture) => texture,
            Err(_) => return,
        };

        let _ = texture.with_lock(None, |data, pitch| {
            for row in 0..FB_HEIGHT {
                for col in 0..FB_WIDTH {
                    let shade = (frame[row * FB_WIDTH + col] & 3) as usize;
                    let (r, g, b) = PALETTE[shade].0;
                    let off = row * pitch + col * 3;
                    data[off] = r;
                    data[off + 1] = g;
                    data[off + 2] = b;
                }
            }
        });

        self.canvas.clear();
        let _ = self.canvas.copy(&texture, None, None);
        self.canvas.present();

        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => std::process::exit(0),
                Event::KeyDown { keycode: Some(kc), repeat: false, .. } => {
                    if let Some(bit) = keycode_to_bit(kc) {
                        self.buttons |= 1 << bit;
                    }
                }
                Event::KeyUp { keycode: Some(kc), .. } => {
                    if let Some(bit) = keycode_to_bit(kc) {
                        self.buttons &= !(1 << bit);
                    }
                }
                _ => {}
            }
        }

        self.pace();
    }

    fn joypad_state(&self) -> u8 {
        self.buttons
    }
}

/// Map an SDL Keycode to a joypad bit index.
/// bit0=Right, bit1=Left, bit2=Up, bit3=Down, bit4=A(Z), bit5=B(X),
/// bit6=Select(Backspace), bit7=Start(Return)
fn keycode_to_bit(kc: Keycode) -> Option<u8> {
    match kc {
        Keycode::Right => Some(0),
        Keycode::Left => Some(1),
        Keycode::Up => Some(2),
        Keycode::Down => Some(3),
        Keycode::Z => Some(4),
        Keycode::X => Some(5),
        Keycode::Backspace => Some(6),
        Keycode::Return => Some(7),
        _ => None,
    }
}

/// Sample the source frame at scaled output coordinates (nearest pixel),
/// returning the DMG shade index 0..=3.
fn sample(frame: &[u8], ox: usize, oy: usize, out_w: usize, out_h: usize) -> u8 {
    let sx = ox * FB_WIDTH / out_w;
    let sy = oy * FB_HEIGHT / out_h;
    frame[sy * FB_WIDTH + sx] & 3
}

/// Largest nearest-neighbour output size (pixels) that fits a terminal of
/// `cols` x `rows` cells, preserving the 160:144 aspect ratio. Each cell is
/// one pixel wide and two pixels tall; the height is rounded down to even.
fn fit(cols: usize, rows: usize) -> (usize, usize) {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let scale = f64::min(cols as f64 / FB_WIDTH as f64, (rows * 2) as f64 / FB_HEIGHT as f64);
    let out_w = ((FB_WIDTH as f64 * scale) as usize).clamp(1, cols);
    let mut out_h = ((FB_HEIGHT as f64 * scale) as usize).clamp(2, rows * 2);
    out_h &= !1; // even: half-blocks pair rows
    (out_w, out_h.max(2))
}

/// Ask `stty` for the controlling terminal's `(cols, rows)`. Returns `None`
/// when output is not a terminal or `stty` is unavailable.
fn detect_terminal_size() -> Option<(u16, u16)> {
    let out = Command::new("stty").arg("size").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let mut parts = text.split_whitespace();
    let rows: u16 = parts.next()?.parse().ok()?;
    let cols: u16 = parts.next()?.parse().ok()?;
    Some((cols, rows))
}

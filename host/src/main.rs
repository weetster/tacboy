use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::ptr;

#[allow(non_camel_case_types, non_snake_case, dead_code)]
mod tacit;
mod frontend;
#[allow(dead_code)]
mod shim {
    include!(env!("TACIT_SHIM_PATH"));
}

use frontend::Frontend;
use shim::{run, TacboyContext};
use tacit::{tacit_status, Error, TacboyCallbacks};

const DEFAULT_MAX_CYCLES: i64 = 5_000_000;

struct Host {
    rom: Vec<u8>,
    serial: Vec<u8>,
    frames_presented: u64,
    last_frame_len: usize,
    frontend: Box<dyn Frontend>,
    /// When set (via `TACBOY_DUMP`), each frame's raw bytes are written here,
    /// so the final file holds the last frame the PPU produced.
    dump_path: Option<PathBuf>,
}

impl TacboyCallbacks for Host {
    fn present_frame(&mut self, frame: &[u8]) -> Result<i64, Error> {
        self.frames_presented = self.frames_presented.saturating_add(1);
        self.last_frame_len = frame.len();
        if let Some(path) = &self.dump_path {
            let _ = fs::write(path, frame);
        }
        self.frontend.present(frame);
        Ok(self.frames_presented as i64)
    }

    fn rom_byte(&mut self, offset: i64) -> Result<u8, Error> {
        if offset < 0 || (offset as usize) >= self.rom.len() {
            return Err(Error::HostError(tacit_status::TACIT_STATUS_BAD_ARGUMENT));
        }
        Ok(self.rom[offset as usize])
    }

    fn write_serial(&mut self, byte: u8) -> Result<i64, Error> {
        self.serial.push(byte);
        let stdout = io::stdout();
        let mut h = stdout.lock();
        let _ = h.write_all(&[byte]);
        let _ = h.flush();
        Ok(byte as i64)
    }
}

fn default_rom_path() -> PathBuf {
    PathBuf::from("/home/mike/github/gb-test-roms/cpu_instrs/cpu_instrs.gb")
}

/// Parsed command line: positionals plus the `--frontend`/`--size`/`--color`
/// flags.
struct Args {
    rom_path: PathBuf,
    max_cycles: i64,
    frontend: String,
    /// Terminal size override `(cols, rows)` for the tui frontend.
    size: Option<(u16, u16)>,
    /// Color-mode override (`256`/`truecolor`) for the tui frontend.
    color: Option<String>,
}

/// Parse a cycle budget. Accepts a plain integer, or one of `inf`,
/// `unlimited`, `none`, `0` for an effectively unbounded run.
fn parse_cycles(s: &str) -> Option<i64> {
    match s {
        "inf" | "infinite" | "unlimited" | "none" | "0" => Some(i64::MAX),
        other => other.parse().ok(),
    }
}

/// Parse a `COLSxROWS` terminal size, e.g. `130x32`.
fn parse_size(s: &str) -> Option<(u16, u16)> {
    let (c, r) = s.split_once(['x', 'X'])?;
    Some((c.parse().ok()?, r.parse().ok()?))
}

fn parse_args() -> Args {
    let mut positionals: Vec<String> = Vec::new();
    let mut frontend = String::from("headless");
    let mut size: Option<(u16, u16)> = None;
    let mut color: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--frontend" => {
                frontend = args.next().unwrap_or_else(|| {
                    eprintln!("--frontend requires a value (headless, tui)");
                    std::process::exit(2);
                });
            }
            "--color" => {
                color = Some(args.next().unwrap_or_else(|| {
                    eprintln!("--color requires a value (256, truecolor)");
                    std::process::exit(2);
                }));
            }
            "--size" => {
                let raw = args.next().unwrap_or_else(|| {
                    eprintln!("--size requires a value (e.g. 130x32)");
                    std::process::exit(2);
                });
                size = Some(parse_size(&raw).unwrap_or_else(|| {
                    eprintln!("invalid --size '{raw}' (expected COLSxROWS, e.g. 130x32)");
                    std::process::exit(2);
                }));
            }
            other => positionals.push(other.to_string()),
        }
    }
    Args {
        rom_path: positionals
            .first()
            .map(PathBuf::from)
            .unwrap_or_else(default_rom_path),
        max_cycles: positionals
            .get(1)
            .map(|s| {
                parse_cycles(s).unwrap_or_else(|| {
                    eprintln!("invalid cycle count '{s}' (integer, or inf/unlimited)");
                    std::process::exit(2);
                })
            })
            .unwrap_or(DEFAULT_MAX_CYCLES),
        frontend,
        size,
        color,
    }
}

fn main() {
    let args = parse_args();

    let rom = fs::read(&args.rom_path).unwrap_or_else(|err| {
        eprintln!("failed to read ROM {}: {err}", args.rom_path.display());
        std::process::exit(2);
    });
    let rom_len = rom.len();
    let cycles_label = if args.max_cycles == i64::MAX {
        "unlimited".to_string()
    } else {
        args.max_cycles.to_string()
    };
    eprintln!(
        "loaded {} ({} bytes); max_cycles={}; frontend={}",
        args.rom_path.display(),
        rom_len,
        cycles_label,
        args.frontend,
    );

    // Build the frontend after diagnostics: the terminal frontend clears the
    // screen on construction, so the messages above should print first.
    let frontend = frontend::make(&args.frontend, args.size, args.color.as_deref())
        .unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(2);
    });

    let mut ctx = TacboyContext {
        user: ptr::null_mut(),
        callbacks: ptr::null(),
    };
    ctx.bind_callbacks(Host {
        rom,
        serial: Vec::new(),
        frames_presented: 0,
        last_frame_len: 0,
        frontend,
        dump_path: env::var_os("TACBOY_DUMP").map(PathBuf::from),
    });

    let mut out: i64 = -1;
    let status = unsafe { run(&mut ctx, rom_len as i64, args.max_cycles, &mut out) };

    // Drop the context (and with it the Host and its Frontend) before exiting,
    // so a terminal frontend restores cursor/colors. `process::exit` skips
    // destructors, hence the explicit drop here.
    drop(ctx);

    if status != tacit_status::TACIT_STATUS_OK {
        eprintln!("tacit call failed: {status:?}");
        std::process::exit(2);
    }

    match out {
        0 => {
            eprintln!("\nrun -> HALT");
            std::process::exit(0);
        }
        1 => {
            eprintln!("\nrun -> cycle cap reached ({cycles_label} cycles)");
            std::process::exit(0);
        }
        2 => {
            eprintln!("\nrun -> unknown opcode (diagnostic stashed in regs[13])");
            std::process::exit(0);
        }
        other => {
            eprintln!("\nrun -> unexpected return code {other}");
            std::process::exit(1);
        }
    }
}

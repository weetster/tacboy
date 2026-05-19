use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::ptr;

#[allow(non_camel_case_types, non_snake_case, dead_code)]
mod tacit;
#[allow(dead_code)]
mod shim {
    include!(env!("TACIT_SHIM_PATH"));
}

use shim::{run, TacboyContext};
use tacit::{tacit_status, Error, TacboyCallbacks};

const DEFAULT_MAX_CYCLES: i64 = 5_000_000;

struct Host {
    rom: Vec<u8>,
    serial: Vec<u8>,
    frames_presented: u64,
    last_frame_len: usize,
}

impl TacboyCallbacks for Host {
    fn present_frame(&mut self, frame: &[u8]) -> Result<i64, Error> {
        self.frames_presented = self.frames_presented.saturating_add(1);
        self.last_frame_len = frame.len();
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

fn main() {
    let rom_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_rom_path);
    let max_cycles: i64 = env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_CYCLES);

    let rom = fs::read(&rom_path).unwrap_or_else(|err| {
        eprintln!("failed to read ROM {}: {err}", rom_path.display());
        std::process::exit(2);
    });
    let rom_len = rom.len();
    eprintln!(
        "loaded {} ({} bytes); max_cycles={}",
        rom_path.display(),
        rom_len,
        max_cycles
    );

    let mut ctx = TacboyContext {
        user: ptr::null_mut(),
        callbacks: ptr::null(),
    };
    ctx.bind_callbacks(Host {
        rom,
        serial: Vec::new(),
        frames_presented: 0,
        last_frame_len: 0,
    });

    let mut out: i64 = -1;
    let status = unsafe { run(&mut ctx, rom_len as i64, max_cycles, &mut out) };

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
            eprintln!("\nrun -> cycle cap reached ({max_cycles} cycles)");
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

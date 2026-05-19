use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::ptr;

#[allow(non_camel_case_types, non_snake_case, dead_code)]
mod tacit;
use tacit::*;

const DEFAULT_MAX_CYCLES: i64 = 5_000_000;

struct Host {
    rom: Vec<u8>,
    serial: Vec<u8>,
}

impl TacboyCallbacks for Host {
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

    let mut ctx = tacit_p_230b5804ad38b777_context {
        user: ptr::null_mut(),
        callbacks: ptr::null(),
    };
    ctx.bind_callbacks(Host {
        rom,
        serial: Vec::new(),
    });

    let mut out: i64 = -1;
    let status = unsafe {
        tacit_p_230b5804ad38b777_e_a55cae34fd625b23(&mut ctx, rom_len as i64, max_cycles, &mut out)
    };

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

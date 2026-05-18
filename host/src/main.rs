use std::env;
use std::fs;
use std::path::PathBuf;
use std::ptr;

#[allow(non_camel_case_types, non_snake_case, dead_code)]
mod tacit;
use tacit::*;

struct Host {
    rom: Vec<u8>,
}

impl TacboyCallbacks for Host {
    fn rom_byte(&mut self, offset: i64) -> Result<u8, Error> {
        if offset < 0 || (offset as usize) >= self.rom.len() {
            return Err(Error::HostError(tacit_status::TACIT_STATUS_BAD_ARGUMENT));
        }
        Ok(self.rom[offset as usize])
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

    let rom = fs::read(&rom_path).unwrap_or_else(|err| {
        eprintln!("failed to read ROM {}: {err}", rom_path.display());
        std::process::exit(2);
    });
    let rom_len = rom.len();
    println!("loaded {} ({} bytes)", rom_path.display(), rom_len);

    let mut ctx = tacit_p_2863980d4a431adb_context {
        user: ptr::null_mut(),
        callbacks: ptr::null(),
    };
    ctx.bind_callbacks(Host { rom });

    let mut out: i64 = -1;
    let status = unsafe {
        tacit_p_2863980d4a431adb_e_ee754ecf2110de56(&mut ctx, rom_len as i64, &mut out)
    };

    match status {
        tacit_status::TACIT_STATUS_OK => {
            if out == 0 {
                println!("run -> ok, all memory-map reads matched");
                std::process::exit(0);
            } else {
                eprintln!("run -> mismatch at address 0x{:04X} (return code {})", out - 1, out);
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("tacit call failed: {other:?}");
            std::process::exit(2);
        }
    }
}

use std::ptr;

#[allow(non_camel_case_types, non_snake_case, dead_code)]
mod tacit;
use tacit::*;

struct Host {
    last_byte: Option<u8>,
}

impl TacboyCallbacks for Host {
    fn present_byte(&mut self, byte: u8) -> Result<i64, Error> {
        println!("present_byte = {byte}");
        self.last_byte = Some(byte);
        Ok(byte as i64)
    }
}

fn main() {
    // Program: LD A,5 ; LD B,3 ; ADD A,B ; HALT
    let mut rom: [u8; 6] = [0x3E, 0x05, 0x06, 0x03, 0x80, 0x76];

    let rom_view = tacit_u8vec {
        data: rom.as_mut_ptr(),
        len: rom.len() as u64,
    };

    let mut ctx = tacit_p_7568eb874bcaf7d7_context {
        user: ptr::null_mut(),
        callbacks: ptr::null(),
    };
    ctx.bind_callbacks(Host { last_byte: None });

    let mut out: i64 = -1;
    let status = unsafe {
        tacit_p_7568eb874bcaf7d7_e_98b8cbef6d923734(&mut ctx, rom_view, &mut out)
    };

    match status {
        tacit_status::TACIT_STATUS_OK => {
            println!("run -> status_ok, exit {out}");
            std::process::exit(0);
        }
        other => {
            eprintln!("tacit call failed: {other:?}");
            std::process::exit(2);
        }
    }
}

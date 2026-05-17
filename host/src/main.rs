use std::ffi::c_void;
use std::ptr;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum TacitStatus {
    Ok = 0,
    BadArgument = 1,
    MissingImport = 2,
    HostError = 3,
}

#[repr(C)]
struct TacitCallbacks {
    _unused: u8,
}

#[repr(C)]
struct TacitContext {
    user: *mut c_void,
    callbacks: *const TacitCallbacks,
}

extern "C" {
    fn tacit_p_dcb61de373652ff9_e_0045aea4368f20c0(
        ctx: *mut TacitContext,
        arg0: i64,
        out: *mut i64,
    ) -> TacitStatus;
}

fn main() {
    let callbacks = TacitCallbacks { _unused: 0 };
    let mut ctx = TacitContext {
        user: ptr::null_mut(),
        callbacks: &callbacks as *const _,
    };

    let mut out: i64 = -1;
    let status = unsafe {
        tacit_p_dcb61de373652ff9_e_0045aea4368f20c0(&mut ctx, 0, &mut out)
    };

    match status {
        TacitStatus::Ok => {
            println!("run_demo(0) = {out}");
            std::process::exit(if out == 8 { 0 } else { 1 });
        }
        other => {
            eprintln!("tacit call failed: {other:?}");
            std::process::exit(2);
        }
    }
}

//! Serial sender for Phomemo printers.
//!
//! Opens a serial device, configures it for raw 115200 8N1, writes data
//! from stdin or a file, and closes cleanly.  Keeps a single file
//! descriptor open for the entire session — critical for Bluetooth RFCOMM
//! where each open/close cycle establishes/tears down the radio link.

use std::io::{self, Read};
use std::os::unix::io::RawFd;
use std::process;

// libc constants and functions (no external dependency needed)
extern "C" {
    fn open(path: *const u8, flags: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn tcgetattr(fd: i32, termios: *mut Termios) -> i32;
    fn tcsetattr(fd: i32, action: i32, termios: *const Termios) -> i32;
    fn cfsetspeed(termios: *mut Termios, speed: u64) -> i32;
    fn cfmakeraw(termios: *mut Termios);
    fn tcdrain(fd: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
    fn usleep(usec: u32) -> i32;
}

const O_RDWR: i32 = 0x0002;
const O_NOCTTY: i32 = 0x20000;
const O_NONBLOCK: i32 = 0x0004;
const F_SETFL: i32 = 4;
const TCSANOW: i32 = 0;
const B115200: u64 = 115200;

// macOS termios is 72 bytes
#[repr(C)]
#[derive(Copy, Clone)]
struct Termios {
    c_iflag: u64,
    c_oflag: u64,
    c_cflag: u64,
    c_lflag: u64,
    c_cc: [u8; 20],
    c_ispeed: u64,
    c_ospeed: u64,
}

impl Termios {
    fn zeroed() -> Self {
        Self {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_cc: [0; 20],
            c_ispeed: 0,
            c_ospeed: 0,
        }
    }
}

// macOS CRTSCTS flag
const CRTSCTS: u64 = 0x00030000;

/// Write all bytes to a file descriptor, retrying on short writes and EINTR.
fn write_all(fd: RawFd, data: &[u8]) -> io::Result<()> {
    let mut offset = 0;
    while offset < data.len() {
        let n = unsafe { write(fd, data[offset..].as_ptr(), data.len() - offset) };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        offset += n as usize;
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: phomemo-send <device> [file]");
        process::exit(1);
    }

    let dev_path = format!("{}\0", &args[1]);
    let is_bt = args[1].contains("/cu.Q0");

    // Read input data (from file arg or stdin)
    let mut data = Vec::new();
    if args.len() >= 3 {
        let mut f = std::fs::File::open(&args[2])?;
        f.read_to_end(&mut data)?;
    } else {
        io::stdin().read_to_end(&mut data)?;
    }

    if data.is_empty() {
        return Err("no data to send".into());
    }

    // Open device — O_NONBLOCK prevents blocking on RFCOMM establishment
    let fd = unsafe { open(dev_path.as_ptr(), O_RDWR | O_NOCTTY | O_NONBLOCK) };
    if fd < 0 {
        return Err(format!(
            "failed to open {}: {}",
            &args[1],
            io::Error::last_os_error()
        )
        .into());
    }

    // Clear O_NONBLOCK now that the device is open
    if unsafe { fcntl(fd, F_SETFL, 0) } < 0 {
        let err = io::Error::last_os_error();
        unsafe { close(fd) };
        return Err(format!("fcntl F_SETFL failed: {err}").into());
    }

    // Configure: 115200 baud, 8N1, raw mode, no hardware flow control
    let mut tio = Termios::zeroed();
    if unsafe { tcgetattr(fd, &mut tio) } != 0 {
        let err = io::Error::last_os_error();
        unsafe { close(fd) };
        return Err(format!("tcgetattr failed: {err}").into());
    }

    unsafe { cfmakeraw(&mut tio) };
    unsafe { cfsetspeed(&mut tio, B115200) };
    tio.c_cflag &= !CRTSCTS;

    if unsafe { tcsetattr(fd, TCSANOW, &tio) } != 0 {
        let err = io::Error::last_os_error();
        unsafe { close(fd) };
        return Err(format!("tcsetattr failed: {err}").into());
    }

    // Bluetooth: brief pause for the RFCOMM link to stabilise
    if is_bt {
        unsafe { usleep(500_000) }; // 500ms
    }

    // Write data in chunks
    let chunk_size = if is_bt { 512 } else { 4096 };
    let delay_us = if is_bt { 10_000 } else { 0 }; // 10ms between BT chunks

    for chunk in data.chunks(chunk_size) {
        write_all(fd, chunk)?;
        if delay_us > 0 {
            unsafe { usleep(delay_us) };
        }
    }

    // Drain: wait for kernel transmit buffer to empty.
    // Skip for Bluetooth — tcdrain can block indefinitely on macOS RFCOMM.
    if !is_bt {
        unsafe { tcdrain(fd) };
    } else {
        // Give Bluetooth stack time to transmit remaining buffered data
        unsafe { usleep(2_000_000) }; // 2 seconds
    }

    unsafe { close(fd) };
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: phomemo-send: {e}");
        process::exit(1);
    }
}

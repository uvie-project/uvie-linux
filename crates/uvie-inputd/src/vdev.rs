//! Minimal uinput virtual-keyboard wrapper (raw ioctls, no extra crate).
//!
//! `evdev` 0.12 split uinput out of the crate, so we drive `/dev/uinput`
//! directly: three ioctls to create the device and `write(2)` of
//! `struct input_event` to emit.

use std::io;
use std::mem::size_of;
use std::os::unix::io::RawFd;
use std::time::{SystemTime, UNIX_EPOCH};

// linux/uinput.h
const UINPUT_IOCTL_BASE: u32 = 0x55;
const UI_DEV_CREATE: u64 = ((UINPUT_IOCTL_BASE as u64) << 8) | 1;
const UI_DEV_DESTROY: u64 = ((UINPUT_IOCTL_BASE as u64) << 8) | 2;
const UI_DEV_SETUP: u64 = request_code_write(UINPUT_IOCTL_BASE, 3, size_of::<UinputSetup>());
const UI_SET_EVBIT: u64 = request_code_write(UINPUT_IOCTL_BASE, 100, size_of::<u32>());
const UI_SET_KEYBIT: u64 = request_code_write(UINPUT_IOCTL_BASE, 101, size_of::<u32>());

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const BUS_VIRTUAL: u16 = 0x06;

/// _IOW(type, nr, size) — Linux ioctl request encoding.
const fn request_code_write(ty: u32, nr: u32, size: usize) -> u64 {
    // _IOC_WRITE(1) << 30 | size << 16 | type << 8 | nr
    (1u64 << 30) | ((size as u64) << 16) | ((ty as u64) << 8) | nr as u64
}

/// linux/uinput.h `struct uinput_setup`.
#[repr(C)]
struct UinputSetup {
    id_bustype: u16,
    id_vendor: u16,
    id_product: u16,
    id_version: u16,
    name: [u8; 80],
    ff_effects_max: u32,
}

/// linux/input.h `struct input_event` on 64-bit (timeval = 2×i64).
#[repr(C)]
struct InputEventRaw {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

pub struct Vdev {
    fd: RawFd,
}

impl Vdev {
    /// Create a virtual keyboard named `name` that can emit every key code.
    pub fn create(name: &str) -> io::Result<Self> {
        let fd = unsafe {
            libc::open(
                c"/dev/uinput".as_ptr(),
                libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        let fd = if fd < 0 {
            unsafe {
                libc::open(
                    c"/dev/input/uinput".as_ptr(),
                    libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            }
        } else {
            fd
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let v = Self { fd };
        v.enable_key_events()?;
        v.setup(name)?;
        Ok(v)
    }

    fn ioctl(&self, req: u64, arg: u64) -> io::Result<()> {
        let rc = unsafe { libc::ioctl(self.fd, req as libc::c_ulong, arg) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn enable_key_events(&self) -> io::Result<()> {
        self.ioctl(UI_SET_EVBIT, EV_SYN as u64)?;
        self.ioctl(UI_SET_EVBIT, EV_KEY as u64)?;
        for code in 0..=767u64 {
            self.ioctl(UI_SET_KEYBIT, code)?;
        }
        Ok(())
    }

    fn setup(&self, name: &str) -> io::Result<()> {
        let mut setup = UinputSetup {
            id_bustype: BUS_VIRTUAL,
            id_vendor: 0x1,
            id_product: 0x1,
            id_version: 1,
            name: [0; 80],
            ff_effects_max: 0,
        };
        let bytes = name.as_bytes();
        let n = bytes.len().min(79);
        setup.name[..n].copy_from_slice(&bytes[..n]);
        self.ioctl(UI_DEV_SETUP, &setup as *const _ as u64)?;
        self.ioctl(UI_DEV_CREATE, 0)?;
        Ok(())
    }

    /// Emit one raw input event (key press/release, SYN, ...).
    pub fn emit(&mut self, type_: u16, code: u16, value: i32) -> io::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let ev = InputEventRaw {
            tv_sec: now.as_secs() as i64,
            tv_usec: now.subsec_micros() as i64,
            type_,
            code,
            value,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(&ev as *const _ as *const u8, size_of::<InputEventRaw>())
        };
        let rc = unsafe { libc::write(self.fd, bytes.as_ptr() as *const _, bytes.len()) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Emit an EV_KEY event plus SYN_REPORT — how real keyboards batch.
    pub fn emit_key(&mut self, code: u16, down: bool) -> io::Result<()> {
        self.emit(EV_KEY, code, if down { 1 } else { 0 })?;
        self.emit(EV_SYN, 0, 0)
    }
}

impl Drop for Vdev {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.fd, UI_DEV_DESTROY as libc::c_ulong, 0);
            libc::close(self.fd);
        }
    }
}

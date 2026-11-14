use std::fs::File;
use std::io;
use std::os::fd::{FromRawFd, RawFd};

#[derive(Clone, Debug)]
pub struct PtyError(pub String);

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PtyError {}

impl From<io::Error> for PtyError {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

/// A Unix pseudo-terminal pair. The terminal emulator is descoped; this exists
/// for jobs and channel commands that need a controlling terminal.
pub struct PtyPair {
    pub master: File,
    pub slave: File,
    pub name: String,
}

impl std::fmt::Debug for PtyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyPair").field("name", &self.name).finish()
    }
}

impl PtyPair {
    pub fn open(size: crate::term::Size) -> Result<Self, PtyError> {
        #[cfg(unix)]
        {
            open_unix(size)
        }
        #[cfg(not(unix))]
        {
            let _ = size;
            Err(PtyError("PTY is only implemented on Unix".to_string()))
        }
    }
}

#[cfg(unix)]
fn open_unix(size: crate::term::Size) -> Result<PtyPair, PtyError> {
    use std::os::raw::{c_char, c_int, c_void};

    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }
    #[cfg(target_os = "macos")]
    const TIOCSWINSZ: std::os::raw::c_ulong = 0x8008_7467;
    #[cfg(not(target_os = "macos"))]
    const TIOCSWINSZ: std::os::raw::c_ulong = 0x5414;

    #[cfg(target_os = "macos")]
    extern "C" {
        fn openpty(
            amaster: *mut c_int,
            aslave: *mut c_int,
            name: *mut c_char,
            termp: *mut c_void,
            winp: *mut c_void,
        ) -> c_int;
    }
    #[cfg(not(target_os = "macos"))]
    #[link(name = "util")]
    extern "C" {
        fn openpty(
            amaster: *mut c_int,
            aslave: *mut c_int,
            name: *mut c_char,
            termp: *mut c_void,
            winp: *mut c_void,
        ) -> c_int;
    }
    extern "C" {
        fn ioctl(fd: c_int, request: std::os::raw::c_ulong, ...) -> c_int;
    }

    let mut winsize = Winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let mut name = [0 as c_char; 128];
    // SAFETY: openpty initializes master, slave and the name buffer.
    let result = unsafe {
        openpty(
            &mut master,
            &mut slave,
            name.as_mut_ptr(),
            std::ptr::null_mut(),
            &mut winsize as *mut Winsize as *mut c_void,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: file descriptors are owned by the returned File values.
    let master = unsafe { File::from_raw_fd(master) };
    // SAFETY: same as above.
    let slave = unsafe { File::from_raw_fd(slave) };
    // SAFETY: TIOCSWINSZ reads a valid winsize pointer.
    unsafe {
        ioctl(
            std::os::fd::AsRawFd::as_raw_fd(&slave),
            TIOCSWINSZ,
            &mut winsize as *mut Winsize,
        );
    }
    let name = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    Ok(PtyPair {
        master,
        slave,
        name,
    })
}

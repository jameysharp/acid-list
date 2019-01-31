//! Marginally safer wrappers around libc syscalls.

use std::io;

#[must_use]
pub fn flock(fd: libc::c_int, operation: libc::c_int) -> io::Result<()> {
    // flock poses no memory-safety hazards
    unsafe {
        if libc::flock(fd, operation) == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[must_use]
pub unsafe fn mmap(
    addr: *mut libc::c_void,
    len: libc::size_t,
    prot: libc::c_int,
    flags: libc::c_int,
    fildes: libc::c_int,
    off: libc::off_t,
) -> io::Result<*mut libc::c_void> {
    let pa = libc::mmap(addr, len, prot, flags, fildes, off);
    if pa == libc::MAP_FAILED {
        Err(io::Error::last_os_error())
    } else {
        Ok(pa)
    }
}

#[must_use]
pub unsafe fn msync(
    addr: *mut libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
) -> io::Result<()> {
    if libc::msync(addr, len, flags) == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[must_use]
pub unsafe fn munmap(addr: *mut libc::c_void, len: libc::size_t) -> io::Result<()> {
    if libc::munmap(addr, len) == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

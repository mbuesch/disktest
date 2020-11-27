// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020 Michael Buesch <m@bues.ch>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
//

use anyhow as ah;
use std::fs::File;
use std::path::Path;

#[cfg(target_os="linux")]
fn os_drop_file_caches(file: File,
                       _path: &Path,
                       offset: u64,
                       size: u64) -> ah::Result<()> {
    use libc::{posix_fadvise, POSIX_FADV_DONTNEED, off_t};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    // Try FADV_DONTNEED to drop caches.
    file.sync_all().ok();
    let ret = unsafe { posix_fadvise(file.as_raw_fd(),
                                     offset as off_t,
                                     size as off_t,
                                     POSIX_FADV_DONTNEED) };
    if ret == 0 {
        // fadvise success.
        Ok(())
    } else {
        // Try global drop_caches.

        drop(file);

        let proc_file = "/proc/sys/vm/drop_caches";
        let proc_value = "3\n";

        match OpenOptions::new().write(true).open(proc_file) {
            Ok(mut file) => {
                match file.write_all(proc_value.as_bytes()) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(ah::format_err!("{}", e)),
                }
            },
            Err(e) => Err(ah::format_err!("{}", e)),
        }
    }
}

#[cfg(target_os="windows")]
fn os_drop_file_caches(file: File,
                       path: &Path,
                       _offset: u64,
                       _size: u64) -> ah::Result<()> {
    use winapi::um::{
        fileapi::{CreateFileA, OPEN_EXISTING},
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        winbase::FILE_FLAG_NO_BUFFERING,
        winnt::{GENERIC_READ, FILE_SHARE_READ},
    };
    use std::{
        ptr::null_mut,
        ffi::CString,
    };

    // Close the file before re-opening it.
    drop(file);

    // Open the file with FILE_FLAG_NO_BUFFERING.
    // That drops all caches.
    if let Some(path) = path.to_str() {
        if let Ok(path) = CString::new(path) {
            let h = unsafe { CreateFileA(path.as_ptr(),
                                         GENERIC_READ,
                                         FILE_SHARE_READ,
                                         null_mut(),
                                         OPEN_EXISTING,
                                         FILE_FLAG_NO_BUFFERING,
                                         null_mut()) };
            if h == INVALID_HANDLE_VALUE {
                Err(ah::format_err!("Failed to acquire file handle."))
            } else {
                unsafe { CloseHandle(h) };
                Ok(())
            }
        } else {
            Err(ah::format_err!("Failed to convert file name (CString)."))
        }
    } else {
        Err(ah::format_err!("Failed to convert file name (str)."))
    }
}

#[cfg(not(any(target_os="linux", target_os="windows")))]
fn os_drop_file_caches(_file: File,
                       _path: &Path,
                       _offset: u64,
                       _size: u64) -> ah::Result<()> {
    Err(ah::format_err!("Not supported on this operating system."))
}

/// Consume a file object, close it and try to drop all operating system caches.
pub fn drop_file_caches(file: File,
                        path: &Path,
                        offset: u64,
                        size: u64) -> ah::Result<()> {
    os_drop_file_caches(file, path, offset, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_drop_file_caches() {
        let mut tfile = NamedTempFile::new().unwrap();
        let pstr = String::from(tfile.path().to_str().unwrap());
        let path = Path::new(&pstr);
        let file = tfile.as_file_mut().try_clone().unwrap();
        drop_file_caches(file, path, 0, 4096).unwrap();
    }
}

// vim: ts=4 sw=4 expandtab

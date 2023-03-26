// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020-2023 Michael Buesch <m@bues.ch>
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
use std::path::{Path, PathBuf};

pub const DEFAULT_SECTOR_SIZE: u32 = 512;

// ==============================
// === Lowlevel LINUX OS part ===
// ==============================

#[cfg(not(target_os = "windows"))]
use libc::{c_int, off_t, posix_fadvise, POSIX_FADV_DONTNEED, S_IFBLK, S_IFCHR, S_IFMT};

#[cfg(not(target_os = "windows"))]
use std::{
    fs::{metadata, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{fs::MetadataExt as _, io::AsRawFd as _},
};

/// Raw device I/O for Linux OS.
#[cfg(not(target_os = "windows"))]
struct RawIoOs {
    path: PathBuf,
    file: Option<File>,
    read_mode: bool,
    write_mode: bool,
    is_blk: bool,
    is_chr: bool,
    sector_size: u32,
}

#[cfg(not(target_os = "windows"))]
impl RawIoOs {
    pub fn new(path: &Path, mut create: bool, read: bool, write: bool) -> ah::Result<Self> {
        if path.starts_with("/dev/") {
            // Do not create dev nodes by accident.
            // This check is not meant to catch all possible cases,
            // but only the common ones.
            create = false;
        }

        let file = match OpenOptions::new()
            .create(create)
            .read(read)
            .write(write)
            .open(path)
        {
            Ok(f) => f,
            Err(e) => {
                return Err(ah::format_err!("Failed to open file {:?}: {}", path, e));
            }
        };

        let mut self_ = Self {
            path: path.into(),
            file: Some(file),
            read_mode: read,
            write_mode: write,
            is_blk: false,
            is_chr: false,
            sector_size: 0,
        };

        if let Err(e) = self_.read_disk_geometry() {
            let _ = self_.close();
            return Err(e);
        }

        Ok(self_)
    }

    fn read_disk_geometry(&mut self) -> ah::Result<()> {
        if let Ok(meta) = metadata(&self.path) {
            let mode_ifmt = meta.mode() & S_IFMT;
            if mode_ifmt == S_IFBLK {
                self.is_blk = true;
            }
            if mode_ifmt == S_IFCHR {
                self.is_chr = true;
            }
        }

        if self.is_blk {
            let Some(file) = self.file.as_ref() else {
                return Err(ah::format_err!("No file object"));
            };

            let mut sector_size: c_int = 0;
            let res = unsafe {
                libc::ioctl(
                    file.as_raw_fd(),
                    libc::BLKPBSZGET, // get physical sector size.
                    &mut sector_size as *mut c_int,
                )
            };
            if res < 0 {
                return Err(ah::format_err!(
                    "Get device block size: ioctl(BLKPBSZGET) failed."
                ));
            }
            if sector_size <= 0 {
                return Err(ah::format_err!(
                    "Get device block size: ioctl(BLKPBSZGET) invalid size."
                ));
            }

            self.sector_size = sector_size as u32;
        } else {
            self.sector_size = DEFAULT_SECTOR_SIZE;
        }
        Ok(())
    }

    fn get_sector_size(&self) -> u32 {
        self.sector_size
    }

    fn drop_file_caches(mut self, offset: u64, size: u64) -> ah::Result<()> {
        let Some(file) = self.file.take() else {
            return Ok(());
        };

        if self.is_chr {
            // This is a character device.
            // We're done. Don't flush.
            return Ok(());
        }

        if self.write_mode {
            // fsync()
            if let Err(e) = file.sync_all() {
                return Err(ah::format_err!("Failed to flush: {}", e));
            }
        }

        // Try FADV_DONTNEED to drop caches.
        let ret = unsafe {
            posix_fadvise(
                file.as_raw_fd(),
                offset as off_t,
                size as off_t,
                POSIX_FADV_DONTNEED,
            )
        };

        if ret == 0 {
            // fadvise success.
            Ok(())
        } else {
            // Try global drop_caches.

            drop(file);

            let proc_file = "/proc/sys/vm/drop_caches";
            let proc_value = b"3\n";

            match OpenOptions::new().write(true).open(proc_file) {
                Ok(mut file) => match file.write_all(proc_value) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(ah::format_err!("{}", e)),
                },
                Err(e) => Err(ah::format_err!("{}", e)),
            }
        }
    }

    fn close(&mut self) -> ah::Result<()> {
        let Some(file) = self.file.take() else {
            return Ok(());
        };
        if self.write_mode && !self.is_chr {
            if let Err(e) = file.sync_all() {
                return Err(ah::format_err!("Failed to flush: {}", e));
            }
        }
        Ok(())
    }

    fn sync(&mut self) -> ah::Result<()> {
        if self.write_mode && !self.is_chr {
            let Some(file) = self.file.as_mut() else {
                return Err(ah::format_err!("No file object"));
            };
            file.sync_all()?;
        }
        Ok(())
    }

    fn set_len(&mut self, size: u64) -> ah::Result<()> {
        if !self.write_mode {
            return Err(ah::format_err!("File is opened without write permission."));
        }
        if self.is_chr || self.is_blk {
            return Err(ah::format_err!("Cannot set length of raw device."));
        }
        let Some(file) = self.file.as_mut() else {
            return Err(ah::format_err!("No file object"));
        };
        Ok(file.set_len(size)?)
    }

    fn seek(&mut self, offset: u64) -> ah::Result<u64> {
        let Some(file) = self.file.as_mut() else {
            return Err(ah::format_err!("No file object"));
        };
        Ok(file.seek(SeekFrom::Start(offset))?)
    }

    fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult> {
        if !self.read_mode {
            return Err(ah::format_err!("File is opened without read permission."));
        }
        let Some(file) = self.file.as_mut() else {
            return Err(ah::format_err!("No file object"));
        };
        match file.read(buffer) {
            Ok(count) => Ok(RawIoResult::Ok(count)),
            Err(e) => Err(ah::format_err!("Read error: {}", e)),
        }
    }

    fn write(&mut self, buffer: &[u8]) -> ah::Result<RawIoResult> {
        if !self.write_mode {
            return Err(ah::format_err!("File is opened without write permission."));
        }
        let Some(file) = self.file.as_mut() else {
            return Err(ah::format_err!("No file object"));
        };
        if let Err(e) = file.write_all(buffer) {
            if let Some(err_code) = e.raw_os_error() {
                if err_code == libc::ENOSPC {
                    return Ok(RawIoResult::Enospc);
                }
            }
            return Err(ah::format_err!("Write error: {}", e));
        }
        Ok(RawIoResult::Ok(buffer.len()))
    }
}

// ================================
// === Lowlevel WINDOWS OS part ===
// ================================

#[cfg(target_os = "windows")]
use std::{
    ffi::{CString, OsString},
    mem::size_of,
    os::windows::ffi::OsStringExt,
    ptr::null_mut,
};

#[cfg(target_os = "windows")]
use winapi::{
    ctypes::{c_void, wchar_t},
    shared::{
        minwindef::DWORD,
        ntdef::{LANG_NEUTRAL, LARGE_INTEGER, MAKELANGID, SUBLANG_DEFAULT},
        winerror::{ERROR_DISK_FULL, ERROR_SUCCESS},
    },
    um::{
        errhandlingapi::GetLastError,
        fileapi::{
            CreateFileA, FlushFileBuffers, ReadFile, SetEndOfFile, SetFilePointerEx, WriteFile,
            OPEN_ALWAYS, OPEN_EXISTING,
        },
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        ioapiset::DeviceIoControl,
        winbase::{
            FormatMessageW, FILE_BEGIN, FILE_FLAG_NO_BUFFERING, FORMAT_MESSAGE_FROM_SYSTEM,
            FORMAT_MESSAGE_IGNORE_INSERTS,
        },
        winioctl::{
            DISK_GEOMETRY, FSCTL_LOCK_VOLUME, FSCTL_UNLOCK_VOLUME, IOCTL_DISK_GET_DRIVE_GEOMETRY,
        },
        winnt::{
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE,
            HANDLE,
        },
    },
};

/// Raw device I/O for Windows OS.
#[cfg(target_os = "windows")]
struct RawIoOs {
    path: PathBuf,
    cpath: CString,
    handle: HANDLE,
    read_mode: bool,
    write_mode: bool,
    is_raw: bool,
    volume_locked: bool,
    sector_size: u32,
    disk_size: u64,
    cur_offset: u64,
}

#[cfg(target_os = "windows")]
impl RawIoOs {
    pub fn new(path: &Path, create: bool, read: bool, write: bool) -> ah::Result<Self> {
        let Some(pathstr) = path.to_str() else {
            return Err(ah::format_err!("Failed to convert file name (str)."));
        };
        let Ok(cpath) = CString::new(pathstr) else {
            return Err(ah::format_err!("Failed to convert file name (CString)."));
        };

        let is_raw = Self::is_raw_dev(path);

        let mut access_flags: DWORD = Default::default();
        if read {
            access_flags |= GENERIC_READ;
        }
        if write {
            access_flags |= GENERIC_WRITE;
        }

        let create_mode = if create && !is_raw {
            OPEN_ALWAYS
        } else {
            OPEN_EXISTING
        };

        let handle = unsafe {
            CreateFileA(
                cpath.as_ptr(),
                access_flags,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null_mut(),
                create_mode,
                0,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(ah::format_err!(
                "Failed to open file {:?}: {}",
                path,
                Self::get_last_error_string()
            ));
        };

        let volume_locked = if is_raw {
            let mut result: DWORD = Default::default();
            let ok = unsafe {
                DeviceIoControl(
                    handle,
                    FSCTL_LOCK_VOLUME,
                    null_mut(),
                    0,
                    null_mut(),
                    0,
                    &mut result as _,
                    null_mut(),
                )
            };
            if ok == 0 {
                return Err(ah::format_err!(
                    "Failed to lock the raw volume: {}",
                    Self::get_last_error_string()
                ));
            }

            true
        } else {
            false
        };

        let mut self_ = Self {
            path: path.into(),
            cpath,
            handle,
            read_mode: read,
            write_mode: write,
            is_raw,
            volume_locked,
            sector_size: 0,
            disk_size: 0,
            cur_offset: 0,
        };

        if let Err(e) = self_.read_disk_geometry() {
            let _ = self_.close();
            return Err(e);
        }

        Ok(self_)
    }

    fn is_raw_dev(path: &Path) -> bool {
        let mut is_raw = false;

        if let Some(path) = path.to_str() {
            let re_drvphy = regex::Regex::new(r"^\\\\\.\\[a-zA-Z]:$").unwrap();
            let re_phy = regex::Regex::new(r"^\\\\\.\\(?i:PhysicalDrive)\d+$").unwrap();

            if re_drvphy.is_match(path) || re_phy.is_match(path) {
                is_raw = true;
            }
        }

        is_raw
    }

    fn get_last_error() -> u32 {
        unsafe { GetLastError() as _ }
    }

    fn get_last_error_string() -> String {
        let code = Self::get_last_error();
        if code == ERROR_SUCCESS {
            return "Success".to_string();
        }

        let mut msg: [wchar_t; 512] = [Default::default(); 512];
        let count = unsafe {
            FormatMessageW(
                FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
                null_mut(),
                code,
                MAKELANGID(LANG_NEUTRAL, SUBLANG_DEFAULT) as _,
                msg.as_mut_ptr() as _,
                (msg.len() - 1) as _,
                null_mut(),
            )
        };
        if count == 0 || count as usize >= msg.len() {
            return "FormatMessageW() failed.".to_string();
        }

        let msg = OsString::from_wide(&msg[0..(count as usize)]);

        msg.to_string_lossy().to_string()
    }

    fn read_disk_geometry(&mut self) -> ah::Result<()> {
        if self.is_raw {
            if self.handle == INVALID_HANDLE_VALUE {
                return Err(ah::format_err!("File handle is invalid."));
            }

            let mut dg: DISK_GEOMETRY = Default::default();
            let mut result: DWORD = Default::default();
            let ok = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_DISK_GET_DRIVE_GEOMETRY,
                    null_mut(),
                    0,
                    &mut dg as *mut _ as *mut c_void,
                    size_of::<DISK_GEOMETRY>() as _,
                    &mut result as _,
                    null_mut(),
                )
            };

            if ok == 0 {
                return Err(ah::format_err!(
                    "Failed to get drive geometry: {}",
                    Self::get_last_error_string()
                ));
            }
            self.disk_size = dg.BytesPerSector as u64
                * dg.SectorsPerTrack as u64
                * dg.TracksPerCylinder as u64
                * unsafe { *dg.Cylinders.QuadPart() } as u64;
            self.sector_size = dg.BytesPerSector as u32;
        } else {
            self.disk_size = u64::MAX;
            self.sector_size = DEFAULT_SECTOR_SIZE;
        }
        Ok(())
    }

    fn get_sector_size(&self) -> u32 {
        self.sector_size
    }

    fn drop_file_caches(mut self, _offset: u64, _size: u64) -> ah::Result<()> {
        if self.handle == INVALID_HANDLE_VALUE {
            return Ok(());
        }

        // Flush file buffers and close file.
        self.close()?;

        // Open the file with FILE_FLAG_NO_BUFFERING.
        // That drops all caches.
        let handle = unsafe {
            CreateFileA(
                self.cpath.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_NO_BUFFERING,
                null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            Err(ah::format_err!(
                "Failed to acquire file handle: {}",
                Self::get_last_error_string()
            ))
        } else {
            let ok = unsafe { CloseHandle(handle) };
            if ok == 0 {
                Err(ah::format_err!(
                    "Failed to close file handle: {}",
                    Self::get_last_error_string()
                ))
            } else {
                Ok(())
            }
        }
    }

    fn close(&mut self) -> ah::Result<()> {
        if self.handle == INVALID_HANDLE_VALUE {
            return Ok(());
        }

        // Flush file buffers.
        self.sync()?;

        // Unlock the volume.
        if self.volume_locked {
            let mut result: DWORD = Default::default();
            let ok = unsafe {
                DeviceIoControl(
                    self.handle,
                    FSCTL_UNLOCK_VOLUME,
                    null_mut(),
                    0,
                    null_mut(),
                    0,
                    &mut result as _,
                    null_mut(),
                )
            };
            if ok == 0 {
                eprintln!(
                    "Warning: Failed to unlock the raw volume: {}",
                    Self::get_last_error_string()
                );
            }
            self.volume_locked = false;
        }

        // Close the file.
        let ok = unsafe { CloseHandle(self.handle) };
        if ok == 0 {
            return Err(ah::format_err!(
                "Failed to close file handle: {}",
                Self::get_last_error_string()
            ));
        }
        self.handle = INVALID_HANDLE_VALUE;

        Ok(())
    }

    fn sync(&mut self) -> ah::Result<()> {
        if self.handle == INVALID_HANDLE_VALUE {
            return Err(ah::format_err!("File handle is invalid."));
        }
        if !self.write_mode {
            return Ok(());
        }

        let ok = unsafe { FlushFileBuffers(self.handle) };

        if ok == 0 {
            Err(ah::format_err!(
                "Failed to flush file buffers: {}",
                Self::get_last_error_string()
            ))
        } else {
            Ok(())
        }
    }

    fn set_len(&mut self, size: u64) -> ah::Result<()> {
        if !self.write_mode {
            return Err(ah::format_err!("File is opened without write permission."));
        }
        if self.is_raw {
            return Err(ah::format_err!("Cannot set length of raw device."));
        }
        if self.handle == INVALID_HANDLE_VALUE {
            return Err(ah::format_err!("File handle is invalid."));
        }

        self.seek(size)?;
        let ok = unsafe { SetEndOfFile(self.handle) };

        if ok == 0 {
            Err(ah::format_err!(
                "Failed to truncate file: {}",
                Self::get_last_error_string()
            ))
        } else {
            Ok(())
        }
    }

    fn seek(&mut self, offset: u64) -> ah::Result<u64> {
        if self.handle == INVALID_HANDLE_VALUE {
            return Err(ah::format_err!("File handle is invalid."));
        }

        let mut off_li: LARGE_INTEGER = Default::default();
        assert!(offset <= i64::MAX as u64);
        let ok = unsafe {
            *off_li.QuadPart_mut() = offset as i64;
            SetFilePointerEx(self.handle, off_li, null_mut(), FILE_BEGIN)
        };
        if ok == 0 {
            Err(ah::format_err!(
                "SetFilePointerEx({:?}) seek failed: {}",
                self.path,
                Self::get_last_error_string(),
            ))
        } else {
            self.cur_offset = offset;
            Ok(offset)
        }
    }

    //TODO read/write in chunks of sector size?
    fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult> {
        if !self.read_mode {
            return Err(ah::format_err!("File is opened without read permission."));
        }
        if self.handle == INVALID_HANDLE_VALUE {
            return Err(ah::format_err!("File handle is invalid."));
        }

        let mut read_count: DWORD = Default::default();
        let ok = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr() as _,
                buffer.len() as _,
                &mut read_count as _,
                null_mut(),
            )
        };

        if ok == 0 {
            Err(ah::format_err!(
                "Failed to read to file: {}",
                Self::get_last_error_string()
            ))
        } else {
            self.cur_offset += read_count as u64;
            Ok(RawIoResult::Ok(read_count as usize))
        }
    }

    fn write(&mut self, buffer: &[u8]) -> ah::Result<RawIoResult> {
        if !self.write_mode {
            return Err(ah::format_err!("File is opened without write permission."));
        }
        if self.handle == INVALID_HANDLE_VALUE {
            return Err(ah::format_err!("File handle is invalid."));
        }

        let mut len = buffer.len() as u64;
        if self.cur_offset + len > self.disk_size {
            len = self.disk_size - self.cur_offset;
        }

        let mut write_count: DWORD = Default::default();
        let ok = unsafe {
            WriteFile(
                self.handle,
                buffer.as_ptr() as _,
                len as _,
                &mut write_count as _,
                null_mut(),
            )
        };

        if ok == 0 {
            let err = Self::get_last_error();
            if err == ERROR_SUCCESS && (self.cur_offset + len >= self.disk_size)
                || err == ERROR_DISK_FULL
            {
                Ok(RawIoResult::Enospc)
            } else {
                Err(ah::format_err!(
                    "Failed to write to file: {}",
                    Self::get_last_error_string()
                ))
            }
        } else {
            self.cur_offset += write_count as u64;
            Ok(RawIoResult::Ok(write_count as usize))
        }
    }
}

// ====================
// === Generic part ===
// ====================

impl Drop for RawIoOs {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Warning: Failed to close device: {}", e);
        }
    }
}

/// Raw I/O operation result code.
pub enum RawIoResult {
    /// Ok, number of processed bytes.
    Ok(usize),
    /// Out of disk space.
    Enospc,
}

/// Raw device I/O abstraction.
pub struct RawIo {
    os: RawIoOs,
}

impl RawIo {
    /// Open a file or device.
    pub fn new(path: &Path, create: bool, read: bool, write: bool) -> ah::Result<Self> {
        Ok(Self {
            os: RawIoOs::new(path, create, read, write)?,
        })
    }

    /// Get the physical sector size of the file or device.
    /// May return a default substitution value.
    pub fn get_sector_size(&self) -> u32 {
        self.os.get_sector_size()
    }

    /// Close the file, flush all buffers and drop all caches.
    /// This function ensures that subsequent reads are not read from RAM cache.
    pub fn drop_file_caches(self, offset: u64, size: u64) -> ah::Result<()> {
        self.os.drop_file_caches(offset, size)
    }

    /// Close the file and flush all buffers.
    /// (This does not affect the caches).
    pub fn close(&mut self) -> ah::Result<()> {
        self.os.close()
    }

    /// Flush all buffers.
    /// (This does not affect the caches).
    pub fn sync(&mut self) -> ah::Result<()> {
        self.os.sync()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_len(&mut self, size: u64) -> ah::Result<()> {
        self.os.set_len(size)
    }

    /// Seek to a file offset.
    pub fn seek(&mut self, offset: u64) -> ah::Result<u64> {
        self.os.seek(offset)
    }

    /// Read a chunk of data.
    pub fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult> {
        self.os.read(buffer)
    }

    /// Write a chunk of data.
    pub fn write(&mut self, buffer: &[u8]) -> ah::Result<RawIoResult> {
        self.os.write(buffer)
    }
}

// vim: ts=4 sw=4 expandtab

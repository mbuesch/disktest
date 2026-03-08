// -*- coding: utf-8 -*-
//
// disktest - Storage tester
//
// Copyright 2020-2026 Michael Büsch <m@bues.ch>
//
// Licensed under the Apache License version 2.0
// or the MIT license, at your option.
// SPDX-License-Identifier: Apache-2.0 OR MIT
//

use super::{RawIoOsIntf, RawIoResult};
use anyhow::{self as ah, Context as _};
use std::{
    ffi::{CString, OsString},
    mem::size_of,
    os::windows::ffi::OsStringExt as _,
    path::{Path, PathBuf},
    ptr::null_mut,
};
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
            CreateFileA, FlushFileBuffers, OPEN_ALWAYS, OPEN_EXISTING, ReadFile, SetEndOfFile,
            SetFilePointerEx, WriteFile,
        },
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        ioapiset::DeviceIoControl,
        winbase::{
            FILE_BEGIN, FILE_FLAG_NO_BUFFERING, FORMAT_MESSAGE_FROM_SYSTEM,
            FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW,
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
pub struct RawIoWindows {
    path: PathBuf,
    cpath: CString,
    handle: HANDLE,
    read_mode: bool,
    write_mode: bool,
    is_raw: bool,
    volume_locked: bool,
    sector_size: Option<u32>,
    disk_size: u64,
    cur_offset: u64,
}

impl RawIoWindows {
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
        // SAFETY: GetLastError() is thread safe.
        // Its internal state is managed per-thread.
        // There are no side effects that affect safety.
        unsafe { GetLastError() as _ }
    }

    fn get_last_error_string(code: Option<u32>) -> String {
        let code = code.unwrap_or_else(Self::get_last_error);

        if code == ERROR_SUCCESS {
            return "Success".to_string();
        }

        let mut msg: [wchar_t; 512] = [Default::default(); 512];
        let msg_len = msg.len() - 1; // Minus one should not be needed. Do it anyway.

        // SAFETY: The FormatMessageW() call is safe, because:
        // - The passed buffer pointer points to valid and initialized memory.
        // - The passed buffer length is not bigger than the buffer's size.
        // - All buffers outlive the use.
        // - The flags do not enable buffer allocation.
        // - There are no side effects that affect safety.
        let count = unsafe {
            FormatMessageW(
                FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
                null_mut(),
                code,
                MAKELANGID(LANG_NEUTRAL, SUBLANG_DEFAULT).into(),
                msg.as_mut_ptr().cast(),
                msg_len.try_into().expect("msg_len u32 overflow"),
                null_mut(),
            )
        };
        if count == 0 || count as usize >= msg.len() {
            return "FormatMessageW() failed.".to_string();
        }

        let msg = OsString::from_wide(&msg[0..(count as usize)]);

        msg.to_string_lossy().to_string()
    }

    #[allow(clippy::unnecessary_cast)]
    fn read_disk_geometry(&mut self) -> ah::Result<()> {
        if self.is_raw {
            if self.handle == INVALID_HANDLE_VALUE {
                return Err(ah::format_err!("File handle is invalid."));
            }

            let mut dg: DISK_GEOMETRY = Default::default();
            let dg_size = size_of::<DISK_GEOMETRY>();
            let mut result: DWORD = Default::default();

            // Get the disk geometry information.
            //
            // SAFETY: This DeviceIoControl() is safe, because:
            // - The handle is valid (checked above).
            // - The DISK_GEOMETRY structure outlives the call and is initialized.
            // - The result DWORD outlives the call and is initialized.
            // - All buffers outlive the use.
            // - There are no side effects that affect safety.
            let ok = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_DISK_GET_DRIVE_GEOMETRY,
                    null_mut(),
                    0,
                    (&raw mut dg).cast::<c_void>(),
                    dg_size.try_into().expect("DISK_GEOMETRY u32 overflow"),
                    std::ptr::from_mut(&mut result),
                    null_mut(),
                )
            };

            if ok == 0 {
                return Err(ah::format_err!(
                    "Failed to get drive geometry: {}",
                    Self::get_last_error_string(None)
                ));
            }

            // SAFETY: Reading Cylinders is safe.
            // The memory is properly initialized.
            let cylinders = unsafe { *dg.Cylinders.QuadPart() };
            let cylinders = u64::try_from(cylinders).context("Cylinders u32 overflow")?;

            self.disk_size = u64::from(dg.BytesPerSector)
                * u64::from(dg.SectorsPerTrack)
                * u64::from(dg.TracksPerCylinder)
                * cylinders;
            self.sector_size = Some(dg.BytesPerSector as u32);
        } else {
            self.disk_size = u64::MAX;
            self.sector_size = None;
        }
        Ok(())
    }
}

impl RawIoOsIntf for RawIoWindows {
    fn new(path: &Path, create: bool, read: bool, write: bool) -> ah::Result<Self> {
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

        // Open the device or file.
        //
        // SAFETY: Opening is safe, because:
        // - The passed path is a valid C string with NUL termination.
        // - All buffers outlive the use.
        // - There are no side effects that affect safety.
        // - The returned handle is checked and not used, if it is invalid.
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
                "Failed to open file {}: {}",
                path.display(),
                Self::get_last_error_string(None)
            ));
        }

        let volume_locked = if is_raw {
            let mut result: DWORD = Default::default();
            // Lock the volume to get exclusive access.
            //
            // SAFETY: Volume locking is safe, because:
            // - The handle is valid (checked above).
            // - The DWORD holding the result is initialized memory.
            // - All buffers outlive the use.
            // - There are no side effects that affect safety.
            let ok = unsafe {
                DeviceIoControl(
                    handle,
                    FSCTL_LOCK_VOLUME,
                    null_mut(),
                    0,
                    null_mut(),
                    0,
                    std::ptr::from_mut(&mut result),
                    null_mut(),
                )
            };
            if ok == 0 {
                return Err(ah::format_err!(
                    "Failed to lock the raw volume: {}",
                    Self::get_last_error_string(None)
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
            sector_size: None,
            disk_size: 0,
            cur_offset: 0,
        };

        if let Err(e) = self_.read_disk_geometry() {
            let _ = self_.close();
            return Err(e);
        }

        Ok(self_)
    }

    fn get_sector_size(&self) -> Option<u32> {
        self.sector_size
    }

    fn drop_file_caches(&mut self, _offset: u64, _size: u64) -> ah::Result<()> {
        if self.handle == INVALID_HANDLE_VALUE {
            return Ok(());
        }

        // Flush file buffers and close file.
        self.close()?;

        // Open the file with FILE_FLAG_NO_BUFFERING.
        // That drops all caches.
        //
        // SAFETY: This open is the same as the initial CreateFileA()
        // but with an additional FILE_FLAG_NO_BUFFERING.
        // - The passed path is a valid C string with NUL termination.
        // - There are no side effects that affect safety.
        // - The returned handle is checked and not used, if it is invalid.
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
                Self::get_last_error_string(None)
            ))
        } else {
            // SAFETY: CloseHandle() is safe, because:
            // - The handle is valid (checked above).
            // - There are no side effects that affect safety.
            let ok = unsafe { CloseHandle(handle) };
            if ok == 0 {
                Err(ah::format_err!(
                    "Failed to close file handle: {}",
                    Self::get_last_error_string(None)
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

            // SAFETY: Volume unlocking is safe, because:
            // - Unlocking is only performed, if the volume is locked.
            // - The handle is valid (checked above).
            // - The DWORD holding the result is initialized memory.
            // - All buffers outlive the use.
            // - There are no side effects that affect safety.
            let ok = unsafe {
                DeviceIoControl(
                    self.handle,
                    FSCTL_UNLOCK_VOLUME,
                    null_mut(),
                    0,
                    null_mut(),
                    0,
                    std::ptr::from_mut(&mut result),
                    null_mut(),
                )
            };
            if ok == 0 {
                eprintln!(
                    "Warning: Failed to unlock the raw volume: {}",
                    Self::get_last_error_string(None)
                );
            }
            self.volume_locked = false;
        }

        // Close the file.
        //
        // SAFETY: CloseHandle() is safe, because:
        // - The handle is valid (checked above).
        // - self.handle is set to invalid after closing.
        // - There are no side effects that affect safety.
        let ok = unsafe { CloseHandle(self.handle) };

        if ok == 0 {
            return Err(ah::format_err!(
                "Failed to close file handle: {}",
                Self::get_last_error_string(None)
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

        // Synchronize to disk.
        //
        // SAFETY: FlushFileBuffers() is safe, because:
        // - The handle is valid (checked above).
        // - There are no side effects that affect safety.
        let ok = unsafe { FlushFileBuffers(self.handle) };

        if ok == 0 {
            Err(ah::format_err!(
                "Failed to flush file buffers: {}",
                Self::get_last_error_string(None)
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

        // SAFETY: SetEndOfFile() is safe, because:
        // - The handle is valid (checked above).
        // - There are no side effects that affect safety.
        let ok = unsafe { SetEndOfFile(self.handle) };

        if ok == 0 {
            Err(ah::format_err!(
                "Failed to truncate file: {}",
                Self::get_last_error_string(None)
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
        let offset_i64 = i64::try_from(offset).context("File offset i64 overflow")?;

        // SAFETY: The LARGE_INTEGER is properly allocated,
        // correctly sized and outlives the use. The offset value
        // range is checked above.
        unsafe { *off_li.QuadPart_mut() = offset_i64 };

        // SAFETY: SetFilePointerEx() is safe, because:
        // - The handle is valid (checked above).
        // - There are no side effects that affect safety.
        let ok = unsafe { SetFilePointerEx(self.handle, off_li, null_mut(), FILE_BEGIN) };

        if ok == 0 {
            Err(ah::format_err!(
                "SetFilePointerEx({}) seek failed: {}",
                self.path.display(),
                Self::get_last_error_string(None),
            ))
        } else {
            self.cur_offset = offset;
            Ok(offset)
        }
    }

    fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult> {
        if !self.read_mode {
            return Err(ah::format_err!("File is opened without read permission."));
        }
        if self.handle == INVALID_HANDLE_VALUE {
            return Err(ah::format_err!("File handle is invalid."));
        }
        if buffer.is_empty() {
            return Ok(RawIoResult::Ok(0));
        }

        let mut read_count: DWORD = Default::default();
        let buffer_len = buffer.len();

        // SAFETY: The ReadFile() call is safe, because:
        // - The passed buffer pointer points to valid and initialized memory.
        // - The passed buffer length is not bigger than the buffer's size.
        // - The passed read_count points to valid and initialized memory.
        // - All buffers outlive the use.
        // - There are no side effects that affect safety.
        let ok = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr().cast(),
                buffer_len
                    .try_into()
                    .context("Read buffer length u32 overflow")?,
                std::ptr::from_mut(&mut read_count),
                null_mut(),
            )
        };

        self.cur_offset += u64::from(read_count);

        if ok == 0 {
            if self.cur_offset >= self.disk_size {
                Ok(RawIoResult::Ok(read_count as usize))
            } else {
                Err(ah::format_err!(
                    "Failed to read from file: {}",
                    Self::get_last_error_string(None)
                ))
            }
        } else {
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
        if buffer.is_empty() {
            return Ok(RawIoResult::Ok(0));
        }

        let mut write_count: DWORD = Default::default();
        let buffer_len = buffer.len();

        // SAFETY: The WriteFile() call is safe, because:
        // - The passed buffer pointer points to valid and initialized memory.
        // - The passed buffer length is not bigger than the buffer's size.
        // - The passed write_count points to valid and initialized memory.
        // - All buffers outlive the use.
        // - There are no side effects that affect safety.
        let ok = unsafe {
            WriteFile(
                self.handle,
                buffer.as_ptr().cast(),
                buffer_len
                    .try_into()
                    .context("Write buffer length u32 overflow")?,
                std::ptr::from_mut(&mut write_count),
                null_mut(),
            )
        };

        self.cur_offset += u64::from(write_count);

        if ok == 0 {
            if self.cur_offset >= self.disk_size {
                if write_count == 0 {
                    Ok(RawIoResult::Enospc)
                } else {
                    Ok(RawIoResult::Ok(write_count as usize))
                }
            } else {
                let code = Self::get_last_error();
                if code == ERROR_DISK_FULL {
                    Ok(RawIoResult::Enospc)
                } else {
                    Err(ah::format_err!(
                        "Failed to write to file: {}",
                        Self::get_last_error_string(Some(code))
                    ))
                }
            }
        } else {
            Ok(RawIoResult::Ok(write_count as usize))
        }
    }
}

impl Drop for RawIoWindows {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Warning: Failed to close device: {e}");
        }
    }
}

// vim: ts=4 sw=4 expandtab

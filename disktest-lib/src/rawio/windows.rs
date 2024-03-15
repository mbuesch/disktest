// -*- coding: utf-8 -*-
//
// disktest - Storage tester
//
// Copyright 2020-2024 Michael Büsch <m@bues.ch>
//
// Licensed under the Apache License version 2.0
// or the MIT license, at your option.
// SPDX-License-Identifier: Apache-2.0 OR MIT
//

use super::{RawIoOsIntf, RawIoResult};
use anyhow as ah;
use std::{
    ffi::{CString, OsString},
    mem::size_of,
    os::windows::ffi::OsStringExt,
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
                Self::get_last_error_string(None)
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

    fn get_last_error_string(code: Option<u32>) -> String {
        let code = code.unwrap_or_else(Self::get_last_error);

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

    #[allow(clippy::unnecessary_cast)]
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
                    Self::get_last_error_string(None)
                ));
            }
            self.disk_size = dg.BytesPerSector as u64
                * dg.SectorsPerTrack as u64
                * dg.TracksPerCylinder as u64
                * unsafe { *dg.Cylinders.QuadPart() } as u64;
            self.sector_size = Some(dg.BytesPerSector as u32);
        } else {
            self.disk_size = u64::MAX;
            self.sector_size = None;
        }
        Ok(())
    }
}

impl RawIoOsIntf for RawIoWindows {
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
                    Self::get_last_error_string(None)
                );
            }
            self.volume_locked = false;
        }

        // Close the file.
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
        assert!(offset <= i64::MAX as u64);
        let ok = unsafe {
            *off_li.QuadPart_mut() = offset as i64;
            SetFilePointerEx(self.handle, off_li, null_mut(), FILE_BEGIN)
        };
        if ok == 0 {
            Err(ah::format_err!(
                "SetFilePointerEx({:?}) seek failed: {}",
                self.path,
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
        let ok = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr() as _,
                buffer.len() as _,
                &mut read_count as _,
                null_mut(),
            )
        };
        self.cur_offset += read_count as u64;
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
        let ok = unsafe {
            WriteFile(
                self.handle,
                buffer.as_ptr() as _,
                buffer.len() as _,
                &mut write_count as _,
                null_mut(),
            )
        };
        self.cur_offset += write_count as u64;
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
            eprintln!("Warning: Failed to close device: {}", e);
        }
    }
}

// vim: ts=4 sw=4 expandtab

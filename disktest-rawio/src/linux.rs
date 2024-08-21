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
use libc::{c_int, off_t, POSIX_FADV_DONTNEED};
use std::{
    fs::{metadata, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{fs::MetadataExt as _, io::AsRawFd as _},
    path::{Path, PathBuf},
};

#[allow(clippy::unnecessary_cast)]
const S_IFBLK: u32 = libc::S_IFBLK as u32;
#[allow(clippy::unnecessary_cast)]
const S_IFCHR: u32 = libc::S_IFCHR as u32;
#[allow(clippy::unnecessary_cast)]
const S_IFMT: u32 = libc::S_IFMT as u32;

/// Raw device I/O for Linux OS.
pub struct RawIoLinux {
    path: PathBuf,
    file: Option<File>,
    read_mode: bool,
    write_mode: bool,
    is_blk: bool,
    is_chr: bool,
    sector_size: Option<u32>,
}

impl RawIoLinux {
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
            sector_size: None,
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

            // SAFETY: The ioctl call is safe, because:
            // - The raw file descriptor is valid. (Closing sets self.file to None).
            // - sector_size points to a valid and initialized c_int.
            // - The ioctl only fetches the sector size and has no other side effects.
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

            self.sector_size = Some(sector_size as u32);
        } else {
            self.sector_size = None;
        }
        Ok(())
    }
}

impl RawIoOsIntf for RawIoLinux {
    fn get_sector_size(&self) -> Option<u32> {
        self.sector_size
    }

    fn drop_file_caches(&mut self, offset: u64, size: u64) -> ah::Result<()> {
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
        //
        // SAFETY: The ioctl call is safe, because:
        // - The raw file descriptor is valid. (Closing sets self.file to None).
        // - fadvise DONTNEED has no safety relevant side effects.
        let ret = unsafe {
            libc::posix_fadvise(
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

impl Drop for RawIoLinux {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Warning: Failed to close device: {}", e);
        }
    }
}

// vim: ts=4 sw=4 expandtab

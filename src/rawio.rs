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

use anyhow as ah;
use std::path::Path;

#[cfg(not(target_os = "windows"))]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

pub const DEFAULT_SECTOR_SIZE: u32 = 512;

/// OS interface for raw I/O.
trait RawIoOsIntf {
    fn get_sector_size(&self) -> Option<u32>;
    fn drop_file_caches(&mut self, offset: u64, size: u64) -> ah::Result<()>;
    fn close(&mut self) -> ah::Result<()>;
    fn sync(&mut self) -> ah::Result<()>;
    fn set_len(&mut self, size: u64) -> ah::Result<()>;
    fn seek(&mut self, offset: u64) -> ah::Result<u64>;
    fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult>;
    fn write(&mut self, buffer: &[u8]) -> ah::Result<RawIoResult>;
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
    os: Box<dyn RawIoOsIntf>,
}

impl RawIo {
    /// Open a file or device.
    pub fn new(path: &Path, create: bool, read: bool, write: bool) -> ah::Result<Self> {
        #[cfg(not(target_os = "windows"))]
        let os = Box::new(linux::RawIoLinux::new(path, create, read, write)?);

        #[cfg(target_os = "windows")]
        let os = Box::new(windows::RawIoWindows::new(path, create, read, write)?);

        Ok(Self { os })
    }

    /// Get the physical sector size of the file or device.
    /// Returns None, if this is not a raw device.
    pub fn get_sector_size(&self) -> Option<u32> {
        self.os.get_sector_size()
    }

    /// Close the file, flush all buffers and drop all caches.
    /// This function ensures that subsequent reads are not read from RAM cache.
    pub fn drop_file_caches(mut self, offset: u64, size: u64) -> ah::Result<()> {
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

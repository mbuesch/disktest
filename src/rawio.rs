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
use std::path::Path;

#[cfg(not(target_os = "windows"))]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

pub const DEFAULT_SECTOR_SIZE: u32 = 512;

/// OS interface for raw I/O.
trait RawIoOsIntf {
    fn get_sector_size(&self) -> u32;
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
    /// May return a default substitution value.
    pub fn get_sector_size(&self) -> u32 {
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

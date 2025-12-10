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

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
std::compile_error!(
    "Your operating system is not supported, yet. \
     Please open an issue on GitHub: \
     https://github.com/mbuesch/disktest/issues"
);

use anyhow as ah;
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

pub const DEFAULT_SECTOR_SIZE: u32 = 512;

/// OS interface for raw I/O.
pub trait RawIoOsIntf: Sized {
    /// Open a file or device.
    fn new(path: &Path, create: bool, read: bool, write: bool) -> ah::Result<Self>;

    /// Get the physical sector size of the file or device.
    /// Returns None, if this is not a raw device.
    fn get_sector_size(&self) -> Option<u32>;

    /// Close the file, flush all buffers and drop all caches.
    /// This function ensures that subsequent reads are not read from RAM cache.
    fn drop_file_caches(&mut self, offset: u64, size: u64) -> ah::Result<()>;

    /// Close the file and flush all buffers.
    /// (This does not affect the caches).
    fn close(&mut self) -> ah::Result<()>;

    /// Flush all buffers.
    /// (This does not affect the caches).
    fn sync(&mut self) -> ah::Result<()>;

    /// Truncate or extend the file length to the given size.
    /// This method is for unit testing only.
    fn set_len(&mut self, size: u64) -> ah::Result<()>;

    /// Seek to a file offset.
    fn seek(&mut self, offset: u64) -> ah::Result<u64>;

    /// Read a chunk of data.
    fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult>;

    /// Write a chunk of data.
    fn write(&mut self, buffer: &[u8]) -> ah::Result<RawIoResult>;
}

/// Raw I/O operation result code.
pub enum RawIoResult {
    /// Ok, number of processed bytes.
    Ok(usize),
    /// Out of disk space.
    Enospc,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub use crate::linux::RawIoLinux as RawIo;

#[cfg(target_os = "windows")]
pub use crate::windows::RawIoWindows as RawIo;

// vim: ts=4 sw=4 expandtab

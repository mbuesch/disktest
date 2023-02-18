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
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;

#[cfg(not(target_os="windows"))]
const ENOSPC: i32 = libc::ENOSPC;
#[cfg(target_os="windows")]
const ENOSPC: i32 = winapi::shared::winerror::ERROR_DISK_FULL as i32;

fn is_raw_device(path: &Path) -> bool {
    let mut is_raw = false;

    #[cfg(not(target_os="windows"))]
    {
        use libc::{S_IFMT, S_IFBLK, S_IFCHR};
        use std::fs::metadata;
        use std::os::unix::fs::MetadataExt;

        if let Ok(meta) = metadata(path) {
            let mode_ifmt = meta.mode() & S_IFMT;
            if mode_ifmt == S_IFBLK || mode_ifmt == S_IFCHR {
                is_raw = true;
            }
        }
    }

    #[cfg(target_os="windows")]
    {
        use regex::Regex;

        if let Some(path) = path.to_str() {
            let re_drvphy = Regex::new(r"^\\\\\.\\[a-zA-Z]:$").unwrap();
            let re_phy = Regex::new(r"^\\\\\.PhysicalDrive\d+$").unwrap();

            if re_drvphy.is_match(path) || re_phy.is_match(path) {
                is_raw = true;
            }
        }
    }

    is_raw
}

pub enum RawIoResult {
    Ok(usize),
    Enospc,
}

pub struct RawIo {
    file: File,
    #[allow(dead_code)]
    is_raw: bool,
}

impl RawIo {
    pub fn new(
        path: &Path,
        create: bool,
        read: bool,
        write: bool,
    ) -> ah::Result<Self> {
        let is_raw = is_raw_device(path);

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
        Ok(Self { file, is_raw })
    }
    
    pub fn into_file(self) -> File {
        self.file
    }

    #[cfg(test)]
    pub fn set_len(&mut self, size: u64) -> ah::Result<()> {
        Ok(self.file.set_len(size)?)
    }

    pub fn seek(&mut self, offset: u64) -> ah::Result<u64> {
        Ok(self.file.seek(SeekFrom::Start(offset))?)
    }
    
    pub fn sync(&mut self) -> ah::Result<()> {
        Ok(self.file.sync_all()?)
    }
    
    pub fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult> {
        match self.file.read(buffer) {
            Ok(count) => Ok(RawIoResult::Ok(count)),
            Err(e) => Err(ah::format_err!("Read error: {}", e)),
        }
    }

    pub fn write(&mut self, buffer: &[u8]) -> ah::Result<RawIoResult> {
        if let Err(e) = self.file.write_all(buffer) {
            if let Some(err_code) = e.raw_os_error() {
                if err_code == ENOSPC {
                    return Ok(RawIoResult::Enospc);
                }
            }
            return Err(ah::format_err!("Write error: {}", e));
        }
        Ok(RawIoResult::Ok(buffer.len()))
    }
}

// vim: ts=4 sw=4 expandtab

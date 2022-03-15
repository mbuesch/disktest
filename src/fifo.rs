// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020-2022 Michael Buesch <m@bues.ch>
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
use memmap::MmapMut;
use std::fs::{self, File, OpenOptions};
use std::marker::PhantomData;
use std::mem::size_of;
use std::path::Path;

const ALLOC_COUNT: usize = 1024;

/// A file backed FIFO.
pub struct BackedFifo<'a, T> {
    /// Pop index.
    front:      usize,
    /// Push index.
    back:       usize,
    /// Allocated capacity.
    capacity:   usize,
    /// Memory mapped backing file.
    map:        Option<MmapMut>,
    /// Backing file.
    file:       Option<File>,
    /// Path to backing file.
    path:       &'a Path,
    /// Use T.
    _marker:    PhantomData<T>,
}

impl<'a, T> BackedFifo<'a, T> {
    /// Create a new FIFO with the specified path as file backing.
    pub fn create(path: &'a Path) -> ah::Result<BackedFifo<'a, T>> {
        // Truncate the file.
        OpenOptions::new()
            .read(true).write(true).create(true)
            .truncate(true)
            .open(path)?
            .sync_all()?;
        let mut self_ = BackedFifo {
            front:      0,
            back:       0,
            capacity:   0,
            map:        None,
            file:       None,
            path,
            _marker:    PhantomData,
        };
        self_.reallocate()?;
        Ok(self_)
    }

    /// Open an existing FIFO with the specified path as file backing.
    pub fn open(path: &'a Path) -> ah::Result<BackedFifo<'a, T>> {
        let capacity = fs::metadata(path)?.len() as usize;
        let mut self_ = BackedFifo {
            front:      0,
            back:       capacity,
            capacity,
            map:        None,
            file:       None,
            path,
            _marker:    PhantomData,
        };
        self_.reallocate()?;
        Ok(self_)
    }

    /// Allocate space for at least one new element.
    /// This actually allocates more space to avoid frequent re-allocations.
    fn reallocate(&mut self) -> ah::Result<()> {
        let elem_size = size_of::<T>();
        if self.capacity - self.back < elem_size ||
           self.map.is_none() ||
           self.file.is_none() {
            self.close()?;
            let file = OpenOptions::new()
                .read(true).write(true).create(true)
                .open(self.path)?;
            let capacity;
            if self.capacity - self.back < elem_size {
                capacity = self.capacity + elem_size * ALLOC_COUNT;
                file.set_len(capacity as u64)?;
            } else {
                capacity = self.capacity;
            }
            let map = unsafe { MmapMut::map_mut(&file)? };

            self.map = Some(map);
            self.file = Some(file);
            self.capacity = capacity;
        }
        Ok(())
    }

    /// Close/flush the mapping and the file.
    pub fn close(&mut self) -> ah::Result<()> {
        if let Some(map) = self.map.take() {
            map.flush()?;
        }
        if let Some(file) = self.file.take() {
            file.set_len(self.back as u64)?;
            file.sync_all()?;
        }
        Ok(())
    }
}

impl<'a, T> BackedFifo<'a, T>
    where T: AsRef<[u8]> + AsMut<[u8]> + Default
{
    /// Insert a new element into the FIFO.
    pub fn push_back(&mut self, elem: T) -> ah::Result<()> {
        self.reallocate()?;
        if let Some(map) = self.map.as_mut() {
            let elem_size = size_of::<T>();
            map[self.back..self.back+elem_size].copy_from_slice(elem.as_ref());
            self.back += elem_size;
            Ok(())
        } else {
            Err(ah::format_err!("Failed to map storage."))
        }
    }

    /// Pop and element from the FIFO.
    /// However, this does not actually remove it from the file backing!
    pub fn pop_front(&mut self) -> Option<T> {
        if let Some(map) = self.map.as_mut() {
            let elem_size = size_of::<T>();
            if self.front + elem_size <= self.back {
                let mut ret: T = Default::default();
                ret.as_mut().copy_from_slice(&map[self.front..self.front+elem_size]);
                self.front += elem_size;
                Some(ret)
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl<'a, T> Drop for BackedFifo<'a, T> {
    /// Close/flush the FIFO.
    /// This panics on close/flush failure.
    /// If you want to handle close/flush failures, call close() before dropping.
    fn drop(&mut self) {
        self.close().expect("Failed to close file.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backedfifo() {
        let tdir = tempdir().unwrap();
        let path = tdir.path().join("test_backedfifo");
        // Write to fifo.
        {
            let mut a: BackedFifo<[u8; 4]> = BackedFifo::create(&path).unwrap();
            for _ in 0..ALLOC_COUNT {
                a.push_back([1, 2, 3, 4]).unwrap();
            }
            a.push_back([0xA, 0xB, 0xC, 0xD]).unwrap();
        }
        // Read twice. The pop shouldn't remove from the file.
        for _ in 0..2 {
            let mut a: BackedFifo<[u8; 4]> = BackedFifo::open(&path).unwrap();
            for _ in 0..ALLOC_COUNT {
                assert_eq!(a.pop_front().unwrap(), [1, 2, 3, 4]);
            }
            assert_eq!(a.pop_front().unwrap(), [0xA, 0xB, 0xC, 0xD]);
            assert_eq!(a.pop_front(), None);
        }
    }
}

// vim: ts=4 sw=4 expandtab

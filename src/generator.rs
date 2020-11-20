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

mod chacha;

use anyhow as ah;
use crate::util::prettybytes;

pub use crate::generator::chacha::GeneratorChaCha8;
pub use crate::generator::chacha::GeneratorChaCha12;
pub use crate::generator::chacha::GeneratorChaCha20;

pub trait NextRandom {
    /// Get the size of the next() output with count = 1, in bytes.
    fn get_base_size(&self) -> usize;

    /// Generate the next chunks.
    /// count: The number of chunks to return.
    /// Returns all chunks concatenated in a Vec.
    fn next(&mut self, count: usize) -> Vec<u8>;

    /// Seek the algorithm to the specified offset.
    fn seek(&mut self, byte_offset: u64) -> ah::Result<()> {
        if byte_offset == 0 {
            Ok(())
        } else {
            Err(ah::format_err!("The selected random number generator \
                                does not support seeking to byte offset {}.",
                                prettybytes(byte_offset, true, true)))
        }
    }
}

// vim: ts=4 sw=4 expandtab

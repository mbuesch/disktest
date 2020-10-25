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

use std::cmp::max;

/// Hasher work buffer.
pub struct Buffer {
    data:                   Vec<u8>,
    count:                  u64,
    hashinput_len:          usize,
    prevhash_slice_begin:   usize,
    prevhash_slice_end:     usize,
    count_slice_begin:      usize,
    count_slice_end:        usize,
}

impl Buffer {
    pub const SERIALSIZE: usize = 16 / 8;
    pub const COUNTSIZE: usize = 64 / 8;

    /// Create new hash buffer.
    ///
    /// * `seed`: The seed to use for hash generation.
    /// * `serial`: The serial number of this hash/buffer.
    /// * `hash_size`: The size of the hash that uses this buffer.
    /// * `hash_prevsize`: The hash result slice length to incorporate into the next hash.
    pub fn new(seed: &Vec<u8>,
               serial: u16,
               hash_size: usize,
               hash_prevsize: usize) -> Buffer {
        let seed_len = seed.len();
        assert!(seed_len > 0);
        assert!(hash_size > 0);
        assert!(hash_prevsize <= hash_size);

        /* Allocate input buffer with layout:
         *   [ SEED,        SERIAL,       PREVHASH,     COUNT,    PADDING ]
         *     ^            ^             ^             ^
         *     first slice  second slice  third slice   fourth slice
         *
         * The PREVHASH+COUNT+PADDING slices are also used as output buffer.
         */
        let mut data = vec![0; seed_len +
                               Buffer::SERIALSIZE +
                               max(hash_prevsize + Buffer::COUNTSIZE, hash_size)];
        // Copy seed to first slice.
        data[..seed_len].copy_from_slice(seed);
        // Copy serial to second slice.
        data[seed_len..seed_len+Buffer::SERIALSIZE].copy_from_slice(&serial.to_le_bytes());

        let prevhash_slice_begin = seed_len + Buffer::SERIALSIZE;
        let prevhash_slice_end = seed_len + Buffer::SERIALSIZE + hash_size;

        let count_slice_begin = seed_len + Buffer::SERIALSIZE + hash_prevsize;
        let count_slice_end = count_slice_begin + Buffer::COUNTSIZE;

        let hashinput_len = seed_len + Buffer::SERIALSIZE + hash_prevsize + Buffer::COUNTSIZE;

        Buffer {
            data,
            count: 0,
            hashinput_len,
            prevhash_slice_begin,
            prevhash_slice_end,
            count_slice_begin,
            count_slice_end,
        }
    }

    /// Increment the count and update the hash input data accordingly.
    #[inline]
    pub fn next_count(&mut self) {
        // Get the current count and increment it.
        let count_bytes = self.count.to_le_bytes();
        self.count += 1;

        // Add the count to the input buffer.
        // This overwrites part of the previous hash.
        self.data[self.count_slice_begin..self.count_slice_end].copy_from_slice(&count_bytes);
    }

    /// Get a reference to the hash input data.
    #[inline]
    pub fn hashalg_input(&self) -> &[u8] {
        &self.data[..self.hashinput_len]
    }

    /// Get a mutable reference to the hash output buffer.
    /// The hash shall write its output data to this slice.
    #[inline]
    pub fn hashalg_output(&mut self) -> &mut [u8] {
        &mut self.data[self.prevhash_slice_begin..self.prevhash_slice_end]
    }

    /// Get the result data chunk.
    #[inline]
    pub fn get_result(&mut self) -> &[u8] {
        &self.data[self.prevhash_slice_begin..self.prevhash_slice_end]
    }
}

// vim: ts=4 sw=4 expandtab

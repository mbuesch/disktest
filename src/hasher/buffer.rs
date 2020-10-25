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

pub const SERIALSIZE: usize = 16 / 8;
pub const COUNTSIZE: usize = 64 / 8;

pub fn alloc_buffer(seed: &Vec<u8>,
                    serial: u16,
                    hash_size: usize,
                    hash_prevsize: usize) -> Vec<u8> {
    /* Allocate input buffer with layout:
     *   [ SEED,        SERIAL,       PREVHASH,     COUNT,    PADDING ]
     *     ^            ^             ^             ^
     *     first slice  second slice  third slice   fourth slice
     *
     * The PREVHASH+COUNT+PADDING slices are also used as output buffer.
     */
    let mut buffer = vec![0; seed.len() +
                             SERIALSIZE +
                             max(hash_prevsize + COUNTSIZE, hash_size)];
    // Copy seed to first slice.
    buffer[..seed.len()].copy_from_slice(seed);
    // Copy serial to second slice.
    buffer[seed.len()..seed.len()+SERIALSIZE].copy_from_slice(&serial.to_le_bytes());

    buffer
}

// vim: ts=4 sw=4 expandtab

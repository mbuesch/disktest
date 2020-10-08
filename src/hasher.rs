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

use crypto::{sha2::Sha512, digest::Digest};

pub struct Hasher {
    alg:        Sha512,
    seed_len:   usize,
    count:      u64,
    inbuf:      Vec<u8>,
}

impl Hasher {
    const SIZE: usize = 512 / 8;
    const PREVSIZE: usize = Hasher::SIZE / 2;
    pub const OUTSIZE: usize = Hasher::SIZE;

    pub fn new(seed: &Vec<u8>) -> Hasher {

        /* Allocate input buffer with layout:
         *   [ SEED,        PREVHASH,     COUNTER,    PADDING ]
         *     ^            ^             ^
         *     first slice  second slice  third slice
         *
         * The PREVHASH+COUNTER+PADDING slices are also used as output buffer.
         */
        let mut inbuf = vec![0; seed.len() + max(Hasher::PREVSIZE + 8, Hasher::SIZE)];
        // Copy seed to first slice.
        inbuf[..seed.len()].copy_from_slice(seed);

        return Hasher {
            alg:        Sha512::new(),
            seed_len:   seed.len(),
            count:      0,
            inbuf:      inbuf,
        }
    }

    pub fn next(&mut self) -> &[u8] {
        let seed_len = self.seed_len;

        // Get the current count and increment it.
        let count_bytes = self.count.to_le_bytes();
        let count_bytes_len = count_bytes.len();
        self.count += 1;

        // Add the count to the input buffer.
        // This overwrites part of the previous hash.
        let offs = seed_len + Hasher::PREVSIZE;
        self.inbuf[offs..offs+count_bytes_len].copy_from_slice(&count_bytes);

        // Calculate the next hash.
        let inp_len = seed_len + Hasher::PREVSIZE + count_bytes_len;
        self.alg.input(&self.inbuf[..inp_len]);

        // Get the hash and store it into the input buffer (for next iteration).
        self.alg.result(&mut self.inbuf[seed_len..seed_len+Hasher::SIZE]);
        self.alg.reset();

        // Return the generated hash (slice of input buffer).
        return &self.inbuf[seed_len..seed_len+Hasher::OUTSIZE];
    }
}

// vim: ts=4 sw=4 expandtab

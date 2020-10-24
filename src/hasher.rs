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

use crypto::{sha2::Sha512, digest::Digest};
use std::cmp::max;

pub trait NextHash {
    fn get_size(&self) -> usize;

    fn next(&mut self) -> &[u8];

    fn next_chunk(&mut self, chunk_buffer: &mut Vec<u8>, count: usize) {
        for _ in 0..count {
            chunk_buffer.extend(self.next());
        }
    }
}

pub struct HasherSHA512 {
    alg:        Sha512,
    seed_len:   usize,
    count:      u64,
    inbuf:      Vec<u8>,
}

impl HasherSHA512 {
    const SIZE: usize = 512 / 8;
    const PREVSIZE: usize = HasherSHA512::SIZE / 2;
    const SERIALSIZE: usize = 16 / 8;
    const COUNTSIZE: usize = 64 / 8;

    pub const OUTSIZE: usize = HasherSHA512::SIZE;

    pub fn new(seed: &Vec<u8>, serial: u16) -> HasherSHA512 {

        /* Allocate input buffer with layout:
         *   [ SEED,        SERIAL,       PREVHASH,     COUNT,    PADDING ]
         *     ^            ^             ^             ^
         *     first slice  second slice  third slice   fourth slice
         *
         * The PREVHASH+COUNT+PADDING slices are also used as output buffer.
         */
        let mut inbuf = vec![0; seed.len() +
                                HasherSHA512::SERIALSIZE +
                                max(HasherSHA512::PREVSIZE + HasherSHA512::COUNTSIZE, HasherSHA512::SIZE)];
        // Copy seed to first slice.
        inbuf[..seed.len()].copy_from_slice(seed);
        // Copy serial to second slice.
        inbuf[seed.len()..seed.len()+HasherSHA512::SERIALSIZE].copy_from_slice(&serial.to_le_bytes());

        HasherSHA512 {
            alg:        Sha512::new(),
            seed_len:   seed.len(),
            count:      0,
            inbuf:      inbuf,
        }
    }
}

impl NextHash for HasherSHA512 {
    fn get_size(&self) -> usize {
        HasherSHA512::OUTSIZE
    }

    fn next(&mut self) -> &[u8] {
        let seed_len = self.seed_len;

        // Get the current count and increment it.
        let count_bytes = self.count.to_le_bytes();
        self.count += 1;

        // Add the count to the input buffer.
        // This overwrites part of the previous hash.
        let count_offs = seed_len + HasherSHA512::SERIALSIZE + HasherSHA512::PREVSIZE;
        self.inbuf[count_offs..count_offs+HasherSHA512::COUNTSIZE].copy_from_slice(&count_bytes);

        // Calculate the next hash.
        let inp_len = seed_len + HasherSHA512::SERIALSIZE + HasherSHA512::PREVSIZE + HasherSHA512::COUNTSIZE;
        self.alg.input(&self.inbuf[..inp_len]);

        // Get the hash and store it into the input buffer (for next iteration).
        let prevhash_offs = seed_len + HasherSHA512::SERIALSIZE;
        self.alg.result(&mut self.inbuf[prevhash_offs..prevhash_offs+HasherSHA512::SIZE]);
        self.alg.reset();

        // Return the generated hash (slice of input buffer).
        return &self.inbuf[prevhash_offs..prevhash_offs+HasherSHA512::OUTSIZE];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmp_result() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        assert_eq!(a.next(),
                   vec![84, 52, 250, 213, 185, 106, 59, 187,
                        10, 70, 136, 246, 77, 253, 73, 207,
                        168, 148, 129, 52, 74, 15, 82, 104,
                        102, 111, 115, 107, 93, 132, 211, 183,
                        41, 52, 55, 186, 2, 227, 90, 173,
                        248, 19, 32, 107, 97, 107, 250, 91,
                        220, 127, 176, 121, 168, 137, 43, 247,
                        254, 65, 46, 159, 150, 84, 132, 120]);
        assert_eq!(a.next(),
                   vec![124, 226, 58, 33, 133, 106, 31, 219,
                        199, 201, 140, 81, 106, 17, 79, 177,
                        209, 237, 39, 218, 187, 153, 197, 217,
                        141, 91, 117, 133, 76, 5, 246, 160,
                        197, 245, 191, 215, 155, 5, 135, 211,
                        166, 91, 149, 118, 190, 197, 48, 141,
                        87, 240, 121, 126, 152, 177, 117, 179,
                        49, 96, 153, 213, 109, 47, 237, 114]);
    }

    #[test]
    fn test_params_equal() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        let mut b = HasherSHA512::new(&vec![1,2,3], 0);
        let mut res_a = vec![];
        let mut res_b = vec![];
        for _ in 0..2 {
            res_a.push(a.next().to_vec());
            res_b.push(b.next().to_vec());
        }
        assert_eq!(res_a[0], res_b[0]);
        assert_eq!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }

    #[test]
    fn test_seed_diff() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        let mut b = HasherSHA512::new(&vec![1,2,4], 0);
        let mut res_a = vec![];
        let mut res_b = vec![];
        for _ in 0..2 {
            res_a.push(a.next().to_vec());
            res_b.push(b.next().to_vec());
        }
        assert_ne!(res_a[0], res_b[0]);
        assert_ne!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }

    #[test]
    fn test_serial_diff() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        let mut b = HasherSHA512::new(&vec![1,2,3], 1);
        let mut res_a = vec![];
        let mut res_b = vec![];
        for _ in 0..2 {
            res_a.push(a.next().to_vec());
            res_b.push(b.next().to_vec());
        }
        assert_ne!(res_a[0], res_b[0]);
        assert_ne!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }
}

// vim: ts=4 sw=4 expandtab
